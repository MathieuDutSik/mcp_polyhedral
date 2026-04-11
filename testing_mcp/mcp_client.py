"""
Lightweight MCP client that communicates with the mcp-polyhedral server
over stdin/stdout (JSON-RPC 2.0).

Can run the server either as a local binary or inside a Docker container.
"""

import json
import subprocess
import sys


class McpClient:
    """Manages a child MCP server process and sends JSON-RPC requests."""

    def __init__(self, *, docker_image: str | None = "mcp-polyhedral",
                 binary_path: str | None = None):
        if binary_path:
            cmd = [binary_path]
        elif docker_image:
            cmd = ["docker", "run", "-i", "--rm", docker_image]
        else:
            raise ValueError("Provide either docker_image or binary_path")

        self._proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self._next_id = 1

    # -- low-level ----------------------------------------------------------

    def _send(self, msg: dict) -> dict | None:
        line = json.dumps(msg) + "\n"
        self._proc.stdin.write(line.encode())
        self._proc.stdin.flush()

        # Notifications produce no response
        if "id" not in msg:
            return None

        raw = self._proc.stdout.readline()
        if not raw:
            raise RuntimeError("MCP server closed stdout unexpectedly")
        return json.loads(raw)

    def _request(self, method: str, params: dict | None = None) -> dict:
        msg_id = self._next_id
        self._next_id += 1
        msg = {"jsonrpc": "2.0", "id": msg_id, "method": method}
        if params is not None:
            msg["params"] = params
        resp = self._send(msg)
        if resp is None:
            raise RuntimeError(f"Expected response for {method}, got None")
        return resp

    def _notify(self, method: str, params: dict | None = None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        self._send(msg)

    # -- MCP lifecycle ------------------------------------------------------

    def initialize(self) -> dict:
        resp = self._request("initialize", {})
        self._notify("initialized")
        return resp

    def list_tools(self) -> list[dict]:
        resp = self._request("tools/list")
        return resp["result"]["tools"]

    def call_tool(self, name: str, arguments: dict) -> dict:
        resp = self._request("tools/call", {"name": name, "arguments": arguments})
        return resp["result"]

    # -- convenience --------------------------------------------------------

    def dual_description(self, matrix: list[list[int]], **kwargs) -> dict:
        args = {"matrix": matrix, **kwargs}
        result = self.call_tool("dual_description", args)
        if result.get("isError"):
            text = result["content"][0]["text"]
            raise RuntimeError(f"Tool error: {text}")
        text = result["content"][0]["text"]
        return json.loads(text)

    def close(self):
        if self._proc.stdin:
            self._proc.stdin.close()
        self._proc.wait(timeout=10)

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()


# -- Tool schema for AI providers ------------------------------------------

DUAL_DESCRIPTION_TOOL_SCHEMA = {
    "name": "dual_description",
    "description": (
        "Compute the dual description of a polyhedral cone. "
        "Given an H-representation (rows are integer inequalities a·x >= 0), "
        "returns the V-representation (rows are extreme rays), or vice versa."
    ),
    "input_schema": {
        "type": "object",
        "properties": {
            "matrix": {
                "type": "array",
                "description": "The input matrix (list of rows, each row is a list of integers).",
                "items": {
                    "type": "array",
                    "items": {"type": "integer"}
                }
            },
            "arithmetic": {
                "type": "string",
                "description": "Arithmetic type. Defaults to 'safe_rational'.",
                "enum": ["safe_rational", "rational", "cpp_rational", "mpq_rational"],
            },
            "method": {
                "type": "string",
                "description": "Dual-description method. Defaults to 'cdd'.",
                "enum": ["cdd", "lrs", "ppl_ext", "cdd_ext", "normaliz", "glrs"],
            }
        },
        "required": ["matrix"]
    }
}

# OpenAI/Codex function-calling format
DUAL_DESCRIPTION_OPENAI_TOOL = {
    "type": "function",
    "function": {
        "name": "dual_description",
        "description": DUAL_DESCRIPTION_TOOL_SCHEMA["description"],
        "parameters": {
            "type": "object",
            "properties": DUAL_DESCRIPTION_TOOL_SCHEMA["input_schema"]["properties"],
            "required": ["matrix"]
        }
    }
}

# Gemini function-calling format
DUAL_DESCRIPTION_GEMINI_DECL = {
    "name": "dual_description",
    "description": DUAL_DESCRIPTION_TOOL_SCHEMA["description"],
    "parameters": {
        "type": "object",
        "properties": DUAL_DESCRIPTION_TOOL_SCHEMA["input_schema"]["properties"],
        "required": ["matrix"]
    }
}
