#!/usr/bin/env python3
"""
Run all MCP tests: direct server test + all AI providers.

Skips provider tests if the corresponding API key is not set.

Required environment variables (set those you want to test):
  - ANTHROPIC_API_KEY  (for Claude)
  - GEMINI_API_KEY     (for Gemini)
  - OPENAI_API_KEY     (for OpenAI/Codex)

Usage:
    python test_all.py
    python test_all.py --binary /path/to/mcp-polyhedral
"""

import argparse
import os
import subprocess
import sys


TESTS = [
    ("Direct MCP", "test_mcp_direct.py", None),
    ("Claude",     "test_claude.py",     "ANTHROPIC_API_KEY"),
    ("Gemini",     "test_gemini.py",     "GEMINI_API_KEY"),
    ("OpenAI",     "test_codex.py",      "OPENAI_API_KEY"),
]


def main():
    parser = argparse.ArgumentParser(description="Run all MCP tests")
    parser.add_argument("--binary", help="Path to local mcp-polyhedral binary")
    parser.add_argument("--docker-image", default="mcp-polyhedral",
                        help="Docker image name (default: mcp-polyhedral)")
    args = parser.parse_args()

    extra_args = []
    if args.binary:
        extra_args = ["--binary", args.binary]
    else:
        extra_args = ["--docker-image", args.docker_image]

    script_dir = os.path.dirname(os.path.abspath(__file__))
    results = []

    for name, script, env_var in TESTS:
        if env_var and not os.environ.get(env_var):
            print(f"\n--- {name}: SKIPPED ({env_var} not set) ---")
            results.append((name, "SKIPPED"))
            continue

        print(f"\n--- {name} ---")
        script_path = os.path.join(script_dir, script)
        rc = subprocess.call([sys.executable, script_path] + extra_args)
        status = "PASSED" if rc == 0 else "FAILED"
        results.append((name, status))

    print("\n" + "=" * 40)
    print("Summary:")
    for name, status in results:
        print(f"  {name:15s} {status}")
    print("=" * 40)

    if any(s == "FAILED" for _, s in results):
        sys.exit(1)


if __name__ == "__main__":
    main()
