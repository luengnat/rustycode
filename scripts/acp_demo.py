#!/usr/bin/env python3
"""
Demo: Control ACP server to do work

This demonstrates using the ACP client to control a RustyCode ACP server
to perform tasks like analyzing code, generating documentation, etc.
"""

import sys
import os

# Add scripts directory to path
script_dir = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, script_dir)

from acp_client import ACPClient

def main():
    print("=== ACP Client Demo: Making the Server Work ===\n")

    # Create client
    client = ACPClient(
        server="./target/release/examples/acp_server",
        cwd=".",
        debug=False
    )

    try:
        # Initialize
        print("1. Connecting to ACP server...")
        result = client.initialize()
        print(f"   ✓ Connected to {result.get('server', {}).get('name')}\n")

        # Create session
        print("2. Creating session...")
        session_id = client.create_session(mode="code")
        print(f"   ✓ Session: {session_id[:8]}...\n")

        # Task 1: List files
        print("3. Task: List files in current directory")
        result = client.send_prompt([{
            "role": "user",
            "parts": [{
                "type": "text",
                "text": "List the Rust source files in the src/ directory"
            }]
        }])
        print(f"   ✓ Task completed\n")

        # Task 2: Analyze code
        print("4. Task: Analyze code structure")
        result = client.send_prompt([{
            "role": "user",
            "parts": [{
                "type": "text",
                "text": "What does the main function in src/main.rs do?"
            }]
        }])
        print(f"   ✓ Task completed\n")

        # Task 3: Generate documentation
        print("5. Task: Generate documentation")
        result = client.send_prompt([{
            "role": "user",
            "parts": [{
                "type": "text",
                "text": "Write brief documentation for this project based on the Cargo.toml"
            }]
        }])
        print(f"   ✓ Task completed\n")

        print("=== All Tasks Completed Successfully ===")
        print(f"\nSession ID: {session_id}")
        print("You can resume this session later with:")
        print(f"  ./scripts/acp_client --resume {session_id}")

    except Exception as e:
        print(f"✗ Error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
    finally:
        client.close()

if __name__ == "__main__":
    main()
