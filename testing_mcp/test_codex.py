#!/usr/bin/env python3
"""
Test that OpenAI/Codex can use the MCP polyhedral server to compute dual descriptions.

Flow:
  1. Send GPT-4o a prompt asking it to compute a dual description.
  2. GPT-4o responds with a function call to dual_description.
  3. We forward that call to the MCP server.
  4. We send the result back to GPT-4o.
  5. We verify GPT-4o's final answer and the mathematical result.

Requires:
  - OPENAI_API_KEY environment variable
  - Docker image 'mcp-polyhedral' built, or --binary pointing to local server

Usage:
    export OPENAI_API_KEY=sk-...
    python test_codex.py
    python test_codex.py --binary /path/to/mcp-polyhedral
"""

import argparse
import json
import os
import sys

try:
    from openai import OpenAI
except ImportError:
    sys.exit("Install the OpenAI SDK: pip install openai")

from mcp_client import McpClient, DUAL_DESCRIPTION_OPENAI_TOOL


PROMPT = (
    "Use the dual_description function to compute the dual description of the "
    "positive orthant in R^3. The input matrix is the 3x3 identity matrix: "
    "[[1,0,0],[0,1,0],[0,0,1]]. "
    "After getting the result, tell me how many extreme rays were found "
    "and what the output matrix is."
)


def run_test(mcp_kwargs: dict):
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        sys.exit("Error: OPENAI_API_KEY environment variable not set")

    client = OpenAI(api_key=api_key)

    print("=== OpenAI/Codex MCP Test ===")
    print("  Sending prompt to GPT-4o...")

    messages = [{"role": "user", "content": PROMPT}]

    response = client.chat.completions.create(
        model="gpt-4o",
        messages=messages,
        tools=[DUAL_DESCRIPTION_OPENAI_TOOL],
    )

    msg = response.choices[0].message
    tool_called = False

    if msg.tool_calls:
        tc = msg.tool_calls[0]
        tool_called = True
        print(f"  GPT-4o called function: {tc.function.name}")
        call_args = json.loads(tc.function.arguments)
        print(f"  Arguments: {json.dumps(call_args, indent=2)}")

        assert tc.function.name == "dual_description", \
            f"Expected dual_description, got {tc.function.name}"

        # Forward to MCP server
        print("  Forwarding to MCP server...")
        with McpClient(**mcp_kwargs) as mcp:
            mcp.initialize()
            mcp_result = mcp.dual_description(**call_args)

        output_matrix = mcp_result["matrix"]
        print(f"  MCP returned {len(output_matrix)} rays")

        assert len(output_matrix) == 3, \
            f"Expected 3 rays, got {len(output_matrix)}"

        # Send result back to GPT-4o
        messages.append(msg.model_dump())
        messages.append({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": json.dumps(mcp_result),
        })

        response = client.chat.completions.create(
            model="gpt-4o",
            messages=messages,
            tools=[DUAL_DESCRIPTION_OPENAI_TOOL],
        )

        final_text = response.choices[0].message.content
        print(f"\n  GPT-4o's response:\n    {final_text[:300]}")
        assert "3" in final_text, "GPT-4o's response should mention 3 extreme rays"
    else:
        final_text = msg.content
        print(f"  GPT-4o responded with text (no tool call):\n    {final_text[:300]}")

    assert tool_called, "GPT-4o did not call the dual_description function"
    print("\n  OpenAI/Codex MCP test: PASSED")


def main():
    parser = argparse.ArgumentParser(description="Test OpenAI/Codex with MCP server")
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
