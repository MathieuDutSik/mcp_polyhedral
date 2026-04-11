#!/usr/bin/env python3
"""
Test that Claude can use the MCP polyhedral server to compute dual descriptions.

Flow:
  1. Send Claude a prompt asking it to compute a dual description.
  2. Claude responds with a tool_use block calling dual_description.
  3. We forward that call to the MCP server.
  4. We send the result back to Claude.
  5. We verify Claude's final answer and the mathematical result.

Requires:
  - ANTHROPIC_API_KEY environment variable
  - Docker image 'mcp-polyhedral' built, or --binary pointing to local server

Usage:
    export ANTHROPIC_API_KEY=sk-ant-...
    python test_claude.py
    python test_claude.py --binary /path/to/mcp-polyhedral
"""

import argparse
import json
import os
import sys

try:
    from anthropic import Anthropic
except ImportError:
    sys.exit("Install the Anthropic SDK: pip install anthropic")

from mcp_client import McpClient, DUAL_DESCRIPTION_TOOL_SCHEMA


PROMPT = (
    "Use the dual_description tool to compute the dual description of the "
    "positive orthant in R^3. The input matrix is the 3x3 identity matrix: "
    "[[1,0,0],[0,1,0],[0,0,1]]. "
    "After getting the result, tell me how many extreme rays were found "
    "and what the output matrix is."
)


def run_test(mcp_kwargs: dict):
    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        sys.exit("Error: ANTHROPIC_API_KEY environment variable not set")

    client = Anthropic(api_key=api_key)

    # Anthropic tool format
    tools = [{
        "name": DUAL_DESCRIPTION_TOOL_SCHEMA["name"],
        "description": DUAL_DESCRIPTION_TOOL_SCHEMA["description"],
        "input_schema": DUAL_DESCRIPTION_TOOL_SCHEMA["input_schema"],
    }]

    print("=== Claude MCP Test ===")
    print(f"  Sending prompt to Claude...")

    messages = [{"role": "user", "content": PROMPT}]

    response = client.messages.create(
        model="claude-sonnet-4-6",
        max_tokens=1024,
        tools=tools,
        messages=messages,
    )

    # Process the response - Claude may call the tool
    tool_called = False
    while response.stop_reason == "tool_use":
        # Find the tool_use block
        tool_use_block = None
        text_blocks = []
        for block in response.content:
            if block.type == "tool_use":
                tool_use_block = block
            elif block.type == "text":
                text_blocks.append(block.text)

        if tool_use_block is None:
            break

        tool_called = True
        print(f"  Claude called tool: {tool_use_block.name}")
        print(f"  Arguments: {json.dumps(tool_use_block.input, indent=2)}")

        assert tool_use_block.name == "dual_description", \
            f"Expected dual_description, got {tool_use_block.name}"

        # Forward the tool call to the MCP server
        print(f"  Forwarding to MCP server...")
        with McpClient(**mcp_kwargs) as mcp:
            mcp.initialize()
            mcp_result = mcp.dual_description(**tool_use_block.input)

        output_matrix = mcp_result["matrix"]
        print(f"  MCP returned {len(output_matrix)} rays")

        # Verify the mathematical result
        assert len(output_matrix) == 3, \
            f"Expected 3 rays, got {len(output_matrix)}"

        # Send the tool result back to Claude
        messages.append({"role": "assistant", "content": response.content})
        messages.append({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": tool_use_block.id,
                "content": json.dumps(mcp_result),
            }]
        })

        response = client.messages.create(
            model="claude-sonnet-4-6",
            max_tokens=1024,
            tools=tools,
            messages=messages,
        )

    # Extract final text response
    final_text = ""
    for block in response.content:
        if block.type == "text":
            final_text += block.text

    print(f"\n  Claude's response:\n    {final_text[:300]}")

    assert tool_called, "Claude did not call the dual_description tool"
    assert "3" in final_text, "Claude's response should mention 3 extreme rays"

    print("\n  Claude MCP test: PASSED")


def main():
    parser = argparse.ArgumentParser(description="Test Claude with MCP server")
    parser.add_argument("--binary", help="Path to local mcp-polyhedral binary")
    parser.add_argument("--docker-image", default="mcp-polyhedral",
                        help="Docker image name (default: mcp-polyhedral)")
    args = parser.parse_args()

    mcp_kwargs = {}
    if args.binary:
        mcp_kwargs["binary_path"] = args.binary
        mcp_kwargs["docker_image"] = None
    else:
        mcp_kwargs["docker_image"] = args.docker_image

    run_test(mcp_kwargs)


if __name__ == "__main__":
    main()
