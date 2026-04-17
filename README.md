# RustyCode

Autonomous development framework.

## Installation

### Unix (Linux/macOS)
```bash
curl -sSL https://raw.githubusercontent.com/luengnat/rustycode/main/scripts/install.sh | bash
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/luengnat/rustycode/main/scripts/install.ps1 | iex
```

## Quick Start
After installation, the binary is available at `~/.local/bin/rustycode` (or `%USERPROFILE%\.local\bin\rustycode.exe` on Windows).

### Build Requirements
Ensure you have the following system dependencies installed:
- **Linux**: `protobuf-compiler`, `libssl-dev`, `pkg-config`
- **macOS**: `protobuf` (via Homebrew)
