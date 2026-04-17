use std::path::PathBuf;
use std::process;

fn main() {
    // Parse simple CLI arguments before starting TUI
    let args: Vec<String> = std::env::args().collect();
    let mut resume = false;
    let mut reconfigure = false;
    let mut model_override: Option<String> = None;

    let mut iter = args.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("RustyCode - AI Code Assistant TUI\n");
                println!("USAGE:");
                println!("  rustycode-tui [OPTIONS] [PATH]\n");
                println!("ARGS:");
                println!("  <PATH>    Working directory [default: .]\n");
                println!("OPTIONS:");
                println!("  -h, --help       Print help");
                println!("  -V, --version    Print version");
                println!("  --reconfigure    Re-run first-setup wizard");
                println!("  --resume         Resume most recent session");
                println!("  --model <MODEL>  Override the AI model for this session");
                println!("MODES (set via RUSTYCODE_MODE env var):");
                println!("  ask              Ask questions, no auto-execution (default)");
                println!("  code             Auto-execute safe tools");
                println!("  agent            Full autonomous agent mode");
                println!("SLASH COMMANDS (inside TUI):");
                println!("  /help            Show keyboard shortcuts and features");
                println!("  /model <name>    Switch AI model");
                println!("  /compact         Compact conversation context");
                println!("  /r               Regenerate last response");
                println!("  /cost            Show session token usage and cost");
                println!("  /quit            Exit (or Ctrl+D)\n");
                println!("Full documentation: https://github.com/nat-rustycode/rustycode");
                return;
            }
            "--version" | "-V" => {
                println!("rustycode-tui {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--reconfigure" => reconfigure = true,
            "--resume" => resume = true,
            "--model" | "-m" => {
                model_override = iter.next().cloned();
                if model_override.is_none() {
                    eprintln!("Error: --model requires a model name argument");
                    process::exit(1);
                }
            }
            _ if arg.starts_with('-') => {
                eprintln!("Unknown option: {}", arg);
                eprintln!("Try 'rustycode-tui --help' for more information.");
                process::exit(1);
            }
            _ => {} // Treat as path argument
        }
    }

    // Determine working directory from args
    let cwd = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    // Apply model override via env var so all provider loading picks it up
    if let Some(ref model) = model_override {
        std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", model);
    }

    if let Err(err) = rustycode_tui::run(cwd, reconfigure, resume) {
        eprintln!("Error: {err}");
        process::exit(1);
    }
}
