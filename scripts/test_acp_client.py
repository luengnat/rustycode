#!/usr/bin/env python3
"""Test ACP client with full workflow"""

import sys
import os
import json

# Add scripts directory to path
script_dir = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, script_dir)

from acp_client import ACPClient

def main():
    print("=== ACP Client Test ===\n")

    # Create client
    client = ACPClient(
        server="./target/release/examples/acp_server",
        cwd=".",
        debug=True
    )

    try:
        # Initialize
        print("1. Initializing server...")
        result = client.initialize()
        print(f"   ✓ Server: {result.get('server', {}).get('name')} {result.get('server', {}).get('version')}\n")

        # Create session
        print("2. Creating session...")
        session_id = client.create_session()
        print(f"   ✓ Session: {session_id}\n")

        # Send prompt
        print("3. Sending prompt...")
        result = client.send_prompt([{
            "role": "user",
            "parts": [{
                "type": "text",
                "text": "Hello! What can you do?"
            }]
        }])
        print(f"   ✓ Response: {result}\n")

        print("=== Test Complete ===")

    except Exception as e:
        print(f"✗ Error: {e}")
        sys.exit(1)
    finally:
        client.close()

if __name__ == "__main__":
    main()
