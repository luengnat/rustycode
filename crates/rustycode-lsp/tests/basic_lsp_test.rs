//! Basic LSP communication test

use lsp_types::Url;
use rustycode_lsp::{LspClient, LspClientConfig};

#[tokio::test]
async fn test_lsp_basic_communication() {
    // Use a simple workspace with a real Rust project
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    // Create minimal Cargo.toml
    std::fs::write(
        project_dir.join("Cargo.toml"),
        r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    let src_dir = project_dir.join("src");
    std::fs::create_dir(&src_dir).unwrap();

    let main_rs = src_dir.join("main.rs");
    std::fs::write(&main_rs, "fn main() { println!(\"Hello\"); }").unwrap();

    let uri = Url::from_file_path(&main_rs).unwrap();
    let root_uri = Url::from_directory_path(project_dir)
        .ok()
        .map(|u| u.to_string());

    let mut client = LspClient::new(LspClientConfig {
        server_name: "rust-analyzer".to_string(),
        command: "rust-analyzer".to_string(),
        args: vec![],
        root_uri,
        capabilities: lsp_types::ClientCapabilities::default(),
    });

    println!("Starting LSP client...");
    if client.start().await.is_err() {
        println!("rust-analyzer not available, skipping");
        return;
    }

    println!("Opening document...");
    // Open with INVALID Rust code to trigger diagnostics
    let invalid_code = r#"fn main() {
    let x: i32 = "string";  // Type error!
    nonexistent();         // Undefined function!
}"#;

    if client
        .open_document(uri.clone(), "rust", 1, invalid_code)
        .await
        .is_err()
    {
        println!("Failed to open document");
        return;
    }

    println!("Waiting for diagnostics...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let diags = client.get_diagnostics(&uri).await;
    println!("Diagnostics count: {}", diags.len());

    for diag in &diags {
        println!("  - {:?}", diag);
    }

    println!("Shutting down...");
    let _ = client.shutdown().await;
    let _ = client.exit().await;

    println!("Test complete");
}
