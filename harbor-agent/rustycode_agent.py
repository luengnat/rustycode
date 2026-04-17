"""RustyCode native agent for Harbor / Terminal Bench 2.0.

Runs the compiled rustycode binary inside Docker/Podman containers for TB 2.0
tasks. Uses RustyCode's own LLM provider, tool system, and agent loop.

Architecture:
  Your Mac → Harbor CLI → Podman VM (arm64) → Docker container
                                          → /usr/local/bin/rustycode run --auto

Usage:
    # Build binary first:
    ./scripts/build-release.sh linux-arm64

    # Run single task (sequential config):
    PYTHONPATH=. harbor run -c config.yaml -d terminal-bench@2.0 \\
        --model zai/glm-4.7 --job-name my-test -y

    # Run full batch (parallel config):
    PYTHONPATH=. harbor run -c config-parallel.yaml -d terminal-bench@2.0 \\
        --model zai/glm-4.7 --job-name batch -y
"""

import base64
import json
import os
import platform
import re
import shlex

from harbor.agents.installed.base import BaseInstalledAgent, with_prompt_template
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext


class RustyCodeAgent(BaseInstalledAgent):
    """Harbor agent that runs the rustycode binary natively.

    Copies a pre-built rustycode binary into the container and runs it in
    headless auto mode (`rustycode run --auto`). The binary uses RustyCode's
    own LLM provider and tool execution loop — no external CLI dependency.
    """

    @staticmethod
    def name() -> str:
        return "rustycode"

    def version(self) -> str | None:
        return "0.2.0"

    async def install(self, environment: BaseEnvironment) -> None:
        """Install runtime dependencies in the container."""
        # Install: bash (persistent shell), procps (nproc), python3 (some tasks),
        # expect (TTY/unbuffer), coreutils (timeout command for bash tool)
        await self.exec_as_root(
            environment,
            command=(
                "apt-get update -qq && "
                "apt-get install -y -qq bash procps python3 expect coreutils 2>/dev/null || true"
            ),
        )

    async def setup(self, environment: BaseEnvironment) -> None:
        """Copy rustycode binary and set up environment."""
        await super().setup(environment)

        # Note: apt-get install is handled by install() called via super().setup().
        # No need to repeat it here — that was causing setup timeouts in some containers.

        # Create nproc wrapper if not available (TB 2.0 verified fix)
        await self.exec_as_root(
            environment,
            command=(
                'which nproc 2>/dev/null || '
                'echo "#!/bin/sh\\necho 2" > /usr/local/bin/nproc && '
                'chmod +x /usr/local/bin/nproc 2>/dev/null || true'
            ),
        )

        # Copy the rustycode binary into the container
        # Primary: target/dist/ (output of cross-compilation)
        # Fallback: harbor-agent/ directory itself (survives cargo clean)
        agent_dir = os.path.dirname(os.path.abspath(__file__))
        dist_dir = os.path.join(agent_dir, "..", "target", "dist")
        binary_path = os.environ.get("RUSTYCODE_BINARY", "")

        if not binary_path:
            # Detect container architecture by running uname -m inside the container.
            # TB 2.0 images may be x86_64 even on arm64 hosts (running via QEMU emulation).
            # The host's platform.machine() is NOT reliable for picking the binary.
            try:
                result = await self.exec_as_root(
                    environment, command="uname -m", timeout_sec=10
                )
                container_arch = result.stdout.strip() if result else ""
            except Exception:
                container_arch = ""

            if container_arch == "x86_64":
                binary_path = os.path.join(dist_dir, "rustycode-linux-amd64")
            elif container_arch == "aarch64":
                binary_path = os.path.join(dist_dir, "rustycode-linux-arm64")
            else:
                # Fallback: prefer amd64 since most TB 2.0 images are x86_64
                binary_path = os.path.join(dist_dir, "rustycode-linux-amd64")

        binary_path = os.path.abspath(binary_path)

        # Fallback: check harbor-agent directory for binaries
        if not os.path.exists(binary_path):
            fallback = os.path.join(agent_dir, f"rustycode-linux-{('amd64' if 'amd64' in binary_path else 'arm64')}")
            if os.path.exists(fallback):
                binary_path = fallback

        if not os.path.exists(binary_path):
            raise FileNotFoundError(
                f"RustyCode binary not found at {binary_path}. "
                f"Build with: ./scripts/build-release.sh linux-arm64"
            )

        await environment.upload_file(
            source_path=binary_path,
            target_path="/usr/local/bin/rustycode",
        )

        await self.exec_as_root(
            environment,
            command="chmod +x /usr/local/bin/rustycode",
        )

    @with_prompt_template
    async def run(
        self, instruction: str, environment: BaseEnvironment, context: AgentContext
    ) -> None:
        """Run rustycode to complete the task.

        Uses Base64 encoding for the instruction to avoid shell escaping issues
        with complex multi-line task descriptions (quotes, special chars, etc.).
        """
        # Base64-encode the instruction to avoid shell escaping problems
        instruction_b64 = base64.b64encode(instruction.encode()).decode()

        # Build environment for the agent
        # Fall back to AUTH_TOKEN if API_KEY isn't explicitly set
        api_key = (
            os.environ.get("ANTHROPIC_API_KEY", "")
            or os.environ.get("ANTHROPIC_AUTH_TOKEN", "")
        )
        env = {
            "ANTHROPIC_API_KEY": api_key,
            "ANTHROPIC_AUTH_TOKEN": os.environ.get("ANTHROPIC_AUTH_TOKEN", ""),
            "ANTHROPIC_BASE_URL": os.environ.get("ANTHROPIC_BASE_URL", ""),
            "RUSTYCODE_SANDBOX": "container",
        }

        # Pass model name directly to RustyCode
        if self.model_name:
            raw_model = self.model_name.split("/")[-1]
            env["RUSTYCODE_MODEL_OVERRIDE"] = raw_model
            env["RUSTYCODE_PROVIDER_OVERRIDE"] = "anthropic"

        # Remove empty values
        env = {k: v for k, v in env.items() if v}

        # Create /logs directory for agent output
        await self.exec_as_root(
            environment,
            command="mkdir -p /logs/agent",
        )

        # Run rustycode in headless auto mode
        # Decode instruction from base64 inside the container
        command = (
            f'INSTRUCTION=$(echo "{instruction_b64}" | base64 -d) && '
            f"rustycode run --auto --format json -- \"$INSTRUCTION\" "
            f"2>&1 | tee /logs/agent/rustycode.txt"
        )

        await self.exec_as_agent(
            environment,
            command=command,
            env=env,
            cwd="/app",
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        """Parse rustycode log output to extract token usage metrics."""
        log_path = self.logs_dir / "rustycode.txt"
        if not log_path.exists():
            return

        try:
            log_text = log_path.read_text(errors="replace")
        except Exception:
            return

        # Extract token counts from JSON output lines
        input_tokens = 0
        output_tokens = 0
        for line in log_text.splitlines():
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                obj = json.loads(line)
                if obj.get("type") == "metrics" or "input_tokens" in obj:
                    input_tokens += obj.get("input_tokens", 0)
                    output_tokens += obj.get("output_tokens", 0)
            except (json.JSONDecodeError, ValueError):
                continue

        # Regex fallback for non-JSON lines
        if input_tokens == 0:
            for m in re.finditer(r"input_tokens[\":\s]+(\d+)", log_text):
                input_tokens += int(m.group(1))
        if output_tokens == 0:
            for m in re.finditer(r"output_tokens[\":\s]+(\d+)", log_text):
                output_tokens += int(m.group(1))

        context.n_input_tokens = input_tokens
        context.n_output_tokens = output_tokens
