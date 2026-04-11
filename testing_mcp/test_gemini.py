#!/usr/bin/env python3
"""
Test that Gemini can use the MCP polyhedral server to compute dual descriptions.

Flow:
  1. Send Gemini a prompt asking it to compute a dual description.
  2. Gemini responds with a function call to dual_description.
  3. We forward that call to the MCP server.
  4. We send the result back to Gemini.
  5. We verify Gemini's final answer and the mathematical result.

Requires:
  - GEMINI_API_KEY environment variable
  - Docker image 'mcp-polyhedral' built, or --binary pointing to local server

Usage:
    export GEMINI_API_KEY=...
    python test_gemini.py
    python test_gemini.py --binary /path/to/mcp-polyhedral
"""

import argparse
import json
import os
import sys

try:
    from google import genai
    from google.genai import types
except ImportError:
    sys.exit("Install the Google GenAI SDK: pip install google-genai")

from mcp_client import McpClient


PROMPT = (
    "Use the dual_description function to compute the dual description of the "
    "positive orthant in R^3. The input matrix is the 3x3 identity matrix: "
    "[[1,0,0],[0,1,0],[0,0,1]]. "
    "After getting the result, tell me how many extreme rays were found "
    "and what the output matrix is."
)

DUAL_DESCRIPTION_DECL = types.FunctionDeclaration(
    name="dual_description",
    description=(
        "Compute the dual description of a polyhedral cone. "
        "Given an H-representation (rows are integer inequalities a*x >= 0), "
        "returns the V-representation (rows are extreme rays), or vice versa."
    ),
    parameters=types.Schema(
        type="OBJECT",
        properties={
            "matrix": types.Schema(
                type="ARRAY",
                description="The input matrix (list of rows, each row is a list of integers).",
                items=types.Schema(
                    type="ARRAY",
                    items=types.Schema(type="INTEGER"),
                ),
            ),
        },
        required=["matrix"],
    ),
)


def run_test(mcp_kwargs: dict):
    api_key = os.environ.get("GEMINI_API_KEY")
    if not api_key:
        sys.exit("Error: GEMINI_API_KEY environment variable not set")

    client = genai.Client(api_key=api_key)
    tool = types.Tool(function_declarations=[DUAL_DESCRIPTION_DECL])

    print("=== Gemini MCP Test ===")
    print("  Sending prompt to Gemini...")

    response = client.models.generate_content(
        model="gemini-2.0-flash",
        contents=PROMPT,
        config=types.GenerateContentConfig(tools=[tool]),
    )

    # Check if Gemini made a function call
    tool_called = False
    part = response.candidates[0].content.parts[0]

    if part.function_call:
        fc = part.function_call
        tool_called = True
        print(f"  Gemini called function: {fc.name}")
        call_args = dict(fc.args)
        print(f"  Arguments: {json.dumps(call_args, indent=2)}")

        assert fc.name == "dual_description", \
            f"Expected dual_description, got {fc.name}"

        # Forward to MCP server
        print("  Forwarding to MCP server...")
        with McpClient(**mcp_kwargs) as mcp:
            mcp.initialize()
            # Gemini may send matrix values as floats; convert to int
            matrix = [[int(v) for v in row] for row in call_args["matrix"]]
            mcp_result = mcp.dual_description(matrix)

        output_matrix = mcp_result["matrix"]
        print(f"  MCP returned {len(output_matrix)} rays")

        assert len(output_matrix) == 3, \
            f"Expected 3 rays, got {len(output_matrix)}"

        # Send result back to Gemini
        function_response = types.Part.from_function_response(
            name="dual_description",
            response=mcp_result,
        )

        response = client.models.generate_content(
            model="gemini-2.0-flash",
            contents=[
                types.Content(parts=[types.Part.from_text(text=PROMPT)], role="user"),
                response.candidates[0].content,
                types.Content(parts=[function_response], role="user"),
            ],
            config=types.GenerateContentConfig(tools=[tool]),
        )

        final_text = response.text
        print(f"\n  Gemini's response:\n    {final_text[:300]}")
        assert "3" in final_text, "Gemini's response should mention 3 extreme rays"
    else:
        final_text = response.text
        print(f"  Gemini responded with text (no tool call):\n    {final_text[:300]}")

    assert tool_called, "Gemini did not call the dual_description function"
    print("\n  Gemini MCP test: PASSED")


def main():
    parser = argparse.ArgumentParser(description="Test Gemini with MCP server")
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
