#!/usr/bin/env python3
"""
ACP Client - Control ACP servers as subprocesses

Sends JSON-RPC requests to ACP servers, manages sessions,
and orchestrates multi-agent workflows.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional
import threading
import queue


class ACPClient:
    """Client for interacting with ACP servers"""

    def __init__(
        self,
        server: str = "rustycode-acp",
        cwd: Optional[str] = None,
        timeout: int = 120000,
        debug: bool = False
    ):
        self.server = server
        self.cwd = cwd or os.getcwd()
        self.timeout = timeout
        self.debug = debug
        self.request_id = 0
        self.process: Optional[subprocess.Popen] = None
        self.session_id: Optional[str] = None

    def log(self, message: str) -> None:
        """Log debug message"""
        if self.debug:
            print(f"[DEBUG] {message}", file=sys.stderr)

    def spawn_server(self) -> subprocess.Popen:
        """Spawn ACP server as subprocess"""
        # Find server binary
        server_path = self._find_server()
        self.log(f"Spawning server: {server_path}")

        # Spawn process with stdio pipes
        proc = subprocess.Popen(
            [server_path, "--cwd", self.cwd],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1
        )

        return proc

    def _find_server(self) -> str:
        """Find ACP server binary"""
        # Check environment variable
        if "ACP_SERVER_PATH" in os.environ:
            return os.environ["ACP_SERVER_PATH"]

        # Check if server is in PATH
        server_path = shutil.which(self.server)
        if server_path:
            return server_path

        # Check local build directory
        local_path = Path(__file__).parent.parent / "target" / "release" / "examples" / self.server
        if local_path.exists():
            return str(local_path)

        raise FileNotFoundError(f"ACP server not found: {self.server}")

    def send_request(self, method: str, params: Dict[str, Any]) -> Dict[str, Any]:
        """Send JSON-RPC request to server"""
        if not self.process:
            self.process = self.spawn_server()

        self.request_id += 1

        request = {
            "jsonrpc": "2.0",
            "id": self.request_id,
            "method": method,
            "params": params
        }

        self.log(f"Sending request: {json.dumps(request, indent=2)}")

        # Send request
        request_json = json.dumps(request) + "\n"
        self.process.stdin.write(request_json)
        self.process.stdin.flush()

        # Read response (skip non-JSON lines like logs)
        while True:
            response_line = self.process.stdout.readline()
            if not response_line:
                raise RuntimeError("Server closed connection")

            response_line = response_line.strip()
            if response_line.startswith("{") or response_line.startswith("["):
                break
            # Skip non-JSON lines (logs, etc.)
            self.log(f"Skipping non-JSON line: {response_line}")

        response = json.loads(response_line)
        self.log(f"Received response: {json.dumps(response, indent=2)}")

        # Check for errors
        if "error" in response:
            raise RuntimeError(f"Server error: {response['error']}")

        return response.get("result", {})

    def initialize(self) -> Dict[str, Any]:
        """Initialize ACP session"""
        result = self.send_request("initialize", {"protocol_version": 1})
        self.log(f"Initialized: {result.get('server', {}).get('name')}")
        return result

    def create_session(self, **kwargs) -> str:
        """Create new session"""
        params = {"cwd": kwargs.get("cwd", self.cwd)}
        if "model" in kwargs:
            params["model"] = kwargs["model"]
        if "mode" in kwargs:
            params["mode"] = kwargs["mode"]

        result = self.send_request("session/new", params)
        self.session_id = result.get("session_id")
        self.log(f"Created session: {self.session_id}")
        return self.session_id

    def load_session(self, session_id: str) -> Dict[str, Any]:
        """Load existing session"""
        self.session_id = session_id
        result = self.send_request("session/load", {"session_id": session_id})
        self.log(f"Loaded session: {session_id}")
        return result

    def send_prompt(self, messages: List[Dict[str, Any]]) -> Dict[str, Any]:
        """Send prompt to session"""
        if not self.session_id:
            self.create_session()

        result = self.send_request("session/prompt", {
            "session_id": self.session_id,
            "messages": messages
        })
        self.log("Prompt sent")
        return result

    def close(self):
        """Close server process"""
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
            self.process = None


class ParallelACPExecutor:
    """Execute multiple ACP servers in parallel"""

    def __init__(self, max_workers: int = 4):
        self.max_workers = max_workers
        self.results: Dict[str, Any] = {}

    def execute(self, config: Dict[str, Any]) -> Dict[str, Any]:
        """Execute parallel ACP requests"""
        servers = config.get("servers", [])
        dependencies = config.get("dependencies", {})

        # Build dependency graph
        completed = set()
        results = {}

        while len(completed) < len(servers):
            # Find servers ready to execute (all dependencies completed)
            ready = []
            for server in servers:
                server_id = server["id"]
                if server_id in completed:
                    continue

                deps = dependencies.get(server_id, [])
                if all(dep in completed for dep in deps):
                    ready.append(server)

            if not ready:
                raise RuntimeError("Circular dependency detected")

            # Execute ready servers (up to max_workers)
            batch = ready[:self.max_workers]
            threads = []
            output_queue = queue.Queue()

            for server in batch:
                thread = threading.Thread(
                    target=self._execute_server,
                    args=(server, output_queue)
                )
                threads.append(thread)
                thread.start()

            # Wait for batch completion
            for thread in threads:
                thread.join()

            # Collect results
            while not output_queue.empty():
                server_id, result = output_queue.get()
                results[server_id] = result
                completed.add(server_id)

        return results

    def _execute_server(self, server: Dict[str, Any], output_queue: queue.Queue):
        """Execute single ACP server"""
        server_id = server["id"]
        try:
            client = ACPClient(
                server=server.get("server", "rustycode-acp"),
                cwd=server.get("cwd", os.getcwd()),
                debug=server.get("debug", False)
            )

            # Initialize and create session
            client.initialize()
            client.create_session()

            # Send request
            request = server.get("request", {})
            if "messages" in request:
                result = client.send_prompt(request["messages"])
            else:
                result = client.send_request(
                    request.get("method", "session/prompt"),
                    request.get("params", {})
                )

            output_queue.put((server_id, {
                "success": True,
                "result": result
            }))

            client.close()

        except Exception as e:
            output_queue.put((server_id, {
                "success": False,
                "error": str(e)
            }))


def parse_parallel_config(input_text: str) -> Dict[str, Any]:
    """Parse parallel execution config from HEREDOC"""
    servers = []
    dependencies = {}
    current_server = None
    in_request = False

    for line in input_text.split("\n"):
        if line.startswith("---SERVER---"):
            if current_server:
                servers.append(current_server)
            current_server = {"id": "", "server": "rustycode-acp", "cwd": os.getcwd()}
            in_request = False

        elif line.startswith("---REQUEST---"):
            in_request = True
            current_server["request"] = {}

        elif current_server and in_request:
            if "request_text" not in current_server["request"]:
                current_server["request"]["request_text"] = ""
            current_server["request"]["request_text"] += line + "\n"

        elif current_server:
            if line.startswith("id: "):
                current_server["id"] = line.split(": ", 1)[1].strip()
            elif line.startswith("server: "):
                current_server["server"] = line.split(": ", 1)[1].strip()
            elif line.startswith("cwd: "):
                current_server["cwd"] = line.split(": ", 1)[1].strip()
            elif line.startswith("dependencies: "):
                deps = line.split(": ", 1)[1].strip().split(", ")
                dependencies[current_server["id"]] = [d.strip() for d in deps if d.strip()]

    if current_server:
        servers.append(current_server)

    # Parse request JSON for each server
    for server in servers:
        if "request" in server and "request_text" in server["request"]:
            try:
                server["request"] = json.loads(server["request"]["request_text"].strip())
            except json.JSONDecodeError:
                pass  # Keep raw text

    return {"servers": servers, "dependencies": dependencies}


def main():
    parser = argparse.ArgumentParser(description="ACP Client - Control ACP servers")
    parser.add_argument("--server", default="rustycode-acp", help="ACP server binary")
    parser.add_argument("--cwd", default=os.getcwd(), help="Working directory")
    parser.add_argument("--resume", help="Resume session ID")
    parser.add_argument("--parallel", action="store_true", help="Parallel execution mode")
    parser.add_argument("--timeout", type=int, default=120000, help="Request timeout (ms)")
    parser.add_argument("--debug", action="store_true", help="Enable debug logging")
    parser.add_argument("--max-parallel", type=int, default=4, help="Max parallel workers")

    args = parser.parse_args()

    # Read request from stdin
    input_text = sys.stdin.read()

    try:
        if args.parallel:
            # Parallel execution mode
            config = parse_parallel_config(input_text)
            executor = ParallelACPExecutor(max_workers=args.max_parallel)
            results = executor.execute(config)
            print(json.dumps(results, indent=2))

        else:
            # Single server mode
            client = ACPClient(
                server=args.server,
                cwd=args.cwd,
                timeout=args.timeout,
                debug=args.debug
            )

            # Initialize
            client.initialize()

            # Resume or create session
            if args.resume:
                client.load_session(args.resume)

            # Parse and send request
            try:
                request = json.loads(input_text.strip())
                method = request.get("method", "session/prompt")
                params = request.get("params", {})

                if method == "session/new":
                    session_id = client.create_session(**params)
                    print(json.dumps({"session_id": session_id}, indent=2))

                elif method == "session/prompt":
                    result = client.send_prompt(params.get("messages", []))
                    print(json.dumps(result, indent=2))

                else:
                    result = client.send_request(method, params)
                    print(json.dumps(result, indent=2))

            except json.JSONDecodeError:
                # Treat as plain text prompt
                result = client.send_prompt([{
                    "role": "user",
                    "parts": [{"type": "text", "text": input_text.strip()}]
                }])
                print(json.dumps(result, indent=2))

            client.close()

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    import shutil  # Required for _find_server
    main()
