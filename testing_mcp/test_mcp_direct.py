#!/usr/bin/env python3
"""
Direct test of the MCP server (no AI provider involved).

Verifies that the MCP server correctly handles the JSON-RPC protocol
and computes dual descriptions.

Usage:
    # Via Docker (default):
    python test_mcp_direct.py

    # Via local binary:
    python test_mcp_direct.py --binary /path/to/mcp-polyhedral
"""

import argparse
import sys

from mcp_client import McpClient


def test_initialize(client: McpClient):
    resp = client.initialize()
    result = resp["result"]
    assert "protocolVersion" in result, "Missing protocolVersion"
    assert result["serverInfo"]["name"] == "mcp-polyhedral"
    print("  initialize: OK")


def test_list_tools(client: McpClient):
    tools = client.list_tools()
    names = [t["name"] for t in tools]
    assert "dual_description" in names, f"dual_description not in {names}"
    print(f"  tools/list: OK ({len(tools)} tools: {names})")


def test_dual_description_identity(client: McpClient):
    """The positive orthant in R^3 is self-dual: I_3 -> I_3 (up to scaling)."""
    identity = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    result = client.dual_description(identity)
    matrix = result["matrix"]
    assert len(matrix) == 3, f"Expected 3 rays, got {len(matrix)}"
    for row in matrix:
        assert len(row) == 3, f"Expected 3 columns, got {len(row)}"
    print("  dual_description (I_3 self-dual): OK")


def test_dual_description_2d(client: McpClient):
    """Positive quadrant in R^2: 2 inequalities -> 2 extreme rays."""
    matrix = [[1, 0], [0, 1]]
    result = client.dual_description(matrix)
    rays = result["matrix"]
    assert len(rays) == 2, f"Expected 2 rays, got {len(rays)}"
    print("  dual_description (R^2 quadrant): OK")


def test_dual_description_schlafli(client: McpClient):
    """Schlafli polytope: 27 vertices (7 cols) -> 99 facets."""
    vertices = [
        [1, 0, 0, 0, 0, 0, 0],
        [1, -1, 0, 0, 0, 0, -1],
        [1, 0, -1, 0, 0, 0, -1],
        [1, 0, 0, -1, 0, 0, -1],
        [1, 0, 0, 0, -1, 0, -1],
        [1, 0, 0, 0, 0, -1, -1],
        [1, 1, 1, 0, 0, 0, 1],
        [1, 1, 0, 1, 0, 0, 1],
        [1, 1, 0, 0, 1, 0, 1],
        [1, 1, 0, 0, 0, 1, 1],
        [1, 0, 1, 1, 0, 0, 1],
        [1, 0, 1, 0, 1, 0, 1],
        [1, 0, 1, 0, 0, 1, 1],
        [1, 0, 0, 1, 1, 0, 1],
        [1, 0, 0, 1, 0, 1, 1],
        [1, 0, 0, 0, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 3],
        [1, 1, 0, 0, 0, 0, 0],
        [1, 0, 1, 0, 0, 0, 0],
        [1, 0, 0, 1, 0, 0, 0],
        [1, 0, 0, 0, 1, 0, 0],
        [1, 0, 0, 0, 0, 1, 0],
        [1, 0, 1, 1, 1, 1, 2],
        [1, 1, 0, 1, 1, 1, 2],
        [1, 1, 1, 0, 1, 1, 2],
        [1, 1, 1, 1, 0, 1, 2],
        [1, 1, 1, 1, 1, 0, 2],
    ]
    result = client.dual_description(vertices)
    facets = result["matrix"]
    assert len(facets) == 99, f"Expected 99 facets, got {len(facets)}"
    for row in facets:
        assert len(row) == 7, f"Expected 7 columns, got {len(row)}"
    print("  dual_description (Schlafli 27->99): OK")


def main():
    parser = argparse.ArgumentParser(description="Direct MCP server test")
    parser.add_argument("--binary", help="Path to local mcp-polyhedral binary")
    parser.add_argument("--docker-image", default="mcp-polyhedral",
                        help="Docker image name (default: mcp-polyhedral)")
    args = parser.parse_args()

    kwargs = {}
    if args.binary:
        kwargs["binary_path"] = args.binary
        kwargs["docker_image"] = None
    else:
        kwargs["docker_image"] = args.docker_image

    print("=== Direct MCP Server Tests ===")
    with McpClient(**kwargs) as client:
        test_initialize(client)
        test_list_tools(client)
        test_dual_description_identity(client)
        test_dual_description_2d(client)
        test_dual_description_schlafli(client)

    print("\nAll direct tests passed.")


if __name__ == "__main__":
    main()
