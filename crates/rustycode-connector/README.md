# rustycode-connector

Terminal connector abstraction for rustycode with support for multiple terminal backends.

## Supported Connectors

| Connector | Platform | Performance | Status |
|-----------|----------|-------------|--------|
| **tmux** | Cross-platform | ⭐⭐⭐⭐⭐ (Best) | Production Ready |
| **iterm2-native** | macOS | ⭐⭐⭐⭐ | New - Fast iTerm2 option |
| **iterm2-applescript** | macOS | ⭐⭐ | Limited support |
| **it2 CLI** | macOS | ⭐ | Slow, Python-based |

## Quick Start

### Check Connector Status

```bash
# Check all connectors
cargo run --package rustycode-connector --example connector_detect

# Check specific connector
cargo run --package rustycode-connector --example connector_detect -- tmux
cargo run --package rustycode-connector --example connector_detect -- iterm2-native
cargo run --package rustycode-connector --example connector_detect -- it2
```

### Run Benchmarks

```bash
cargo run --package rustycode-connector --example connector_benchmark
```

## Installation

### tmux (Recommended)

**macOS:**
```bash
brew install tmux
```

**Ubuntu/Debian:**
```bash
sudo apt install tmux
```

**Fedora:**
```bash
sudo dnf install tmux
```

### iTerm2 Native (macOS only)

1. Install iTerm2:
   ```bash
   brew install --cask iterm2
   ```

2. Open iTerm2 and enable the API:
   - Go to **iTerm2 > Settings > General**
   - Check **"Enable API server"**

3. The API socket will be created automatically at:
   `~/Library/Application Support/iTerm2/iterm2-socket`

### it2 CLI (macOS only)

```bash
pip install it2-iterm2
```

## Usage

```rust
use rustycode_connector::{
    DetectedConnector, TerminalConnector, SplitDirection,
    check_connector, InstallStatus, print_connector_status,
};

// Auto-detect best available connector
if let Some(mut conn) = DetectedConnector::detect() {
    println!("Using connector: {}", conn.name());

    // Create a session
    let session = conn.create_session("my-session")?;

    // Split panes
    conn.split_pane(&session, 0, SplitDirection::Horizontal)?;
    conn.split_pane(&session, 0, SplitDirection::Vertical)?;

    // Send commands
    conn.send_keys(&session, 0, "echo 'Hello from pane 0'")?;
    conn.send_keys(&session, 1, "echo 'Hello from pane 1'")?;

    // Capture output
    let content = conn.capture_output(&session, 0)?;
    println!("Pane 0 output: {}", content);

    // Clean up
    conn.close_session(&session)?;
}

// Check connector installation status
print_connector_status();

// Check specific connector
match check_connector("tmux") {
    Some(InstallStatus::Installed) => println!("tmux is ready!"),
    Some(InstallStatus::NotInstalled { install_command, .. }) => {
        println!("Install tmux with: {}", install_command);
    }
    _ => {}
}
```

## Connector Comparison

### Performance (lower is better)

| Operation | tmux | iterm2-native | iterm2-applescript | it2 CLI |
|-----------|------|---------------|---------------------|---------|
| Session create | ~5ms | ~270ms* | ~270ms | ~730ms |
| Split pane | ~8ms | ~15ms | ~90ms | ~310ms |
| Send keys | ~4ms | ~8ms | ~35ms | ~290ms |
| Capture output | ~8ms | ~15ms | ❌ N/A | ~575ms |
| **Total** | **~43ms** | **~350ms** | **~570ms** | **~2600ms** |

*Window creation uses AppleScript fallback (iTerm2 API limitation)

### Feature Support

| Feature | tmux | iterm2-native | iterm2-applescript | it2 CLI |
|---------|------|---------------|---------------------|---------|
| create_session | ✅ | ✅* | ✅ | ✅ |
| split_pane | ✅ | ✅ | ✅ | ✅ |
| send_keys | ✅ | ✅ | ✅ | ✅ |
| capture_output | ✅ | ✅ | ❌ | ✅ |
| set_pane_title | ✅ | ✅ | ⚠️ Window only | ✅ |
| select_pane | ✅ | ✅* | ✅ | ⚠️ Session only |
| kill_pane | ✅ | ❌ | ❌ | ❌ |
| close_session | ✅ | ✅* | ✅ | ✅ |
| wait_for_output | ✅ | ✅ | ❌ | ✅ |

\* Uses AppleScript fallback

## Architecture

### Native iTerm2 Connector

```
┌─────────────────────────────────────────┐
│           iTerm2 Application            │
│  ┌─────────────────────────────────┐   │
│  │           Window 0              │   │
│  │  ┌───────────────────────────┐  │   │
│  │  │         Tab 0             │  │   │
│  │  │  ┌──────┬──────────────┐  │  │   │
│  │  │  │Pane 0│    Pane 1    │  │  │   │
│  │  │  └──────┴──────────────┘  │  │   │
│  │  └───────────────────────────┘  │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
         │
         │ Unix Domain Socket + Protobuf
         │ (minimal overhead)
         │
┌─────────────────────────┐
│ ITerm2NativeConnector   │
│ - Native Rust client    │
│ - iterm2-client crate   │
│ - AppleScript fallback  │
│   (window create/close) │
└─────────────────────────┘
```

## Environment Variables

- `ITERM2_COOKIE` - iTerm2 API authentication cookie
- `ITERM2_KEY` - iTerm2 API key (alternative to cookie)
- `TMUX` - Set when running inside tmux
- `TERM_PROGRAM` - Terminal type identifier

## License

MIT
