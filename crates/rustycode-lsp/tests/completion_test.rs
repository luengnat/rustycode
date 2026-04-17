//! Test LSP completion functionality

use lsp_types::{Position, Url};
use rustycode_lsp::{LspClient, LspClientConfig};

#[tokio::test]
async fn test_lsp_completion() {
    // Create a temporary Rust project
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_dir = temp_dir.path();

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
    let code = r#"fn main() {
    let x = 42;
    println!("{}", x);
}"#; // For completion testing, we'll trigger on 'x.'
    std::fs::write(&main_rs, code).unwrap();

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

    println!("Starting LSP...");
    if client.start().await.is_err() {
        println!("rust-analyzer not available, skipping");
        return;
    }

    println!("Opening document...");
    if client
        .open_document(uri.clone(), "rust", 1, code)
        .await
        .is_err()
    {
        println!("Failed to open document");
        return;
    }

    // Wait for rust-analyzer to initialize
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // First test hover to make sure rust-analyzer is working
    println!("Testing hover at position (line 2, col 8) - should be on 'x' variable...");
    let hover_result = client.hover(uri.clone(), Position::new(2, 8)).await;
    match hover_result {
        Ok(Some(_hover)) => {
            println!("✓ Hover works - rust-analyzer is responding");
        }
        Ok(None) => {
            println!("✗ Hover returned None");
        }
        Err(e) => {
            println!("✗ Hover error: {}", e);
        }
    }

    println!("Testing completion with different configurations...");

    use lsp_types::CompletionContext;
    use lsp_types::CompletionTriggerKind;

    // Try without context first
    println!("1. Testing without context...");
    match client
        .completion(uri.clone(), Position::new(2, 12), None)
        .await
    {
        Ok(Some(completion)) => {
            println!("   ✓ Got completion!");
            print_completion_items(&completion);
        }
        Ok(None) => println!("   ✗ No completion result"),
        Err(e) => println!("   ✗ Error: {}", e),
    }

    // Try with explicit trigger character
    println!("2. Testing with context (trigger character '.')...");
    let context = CompletionContext {
        trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
        trigger_character: Some(".".to_string()),
    };
    match client
        .completion(uri.clone(), Position::new(2, 12), Some(context))
        .await
    {
        Ok(Some(completion)) => {
            println!("   ✓ Got completion!");
            print_completion_items(&completion);
        }
        Ok(None) => println!("   ✗ No completion result"),
        Err(e) => println!("   ✗ Error: {}", e),
    }

    // Try at beginning of line with Invoked trigger
    println!("3. Testing at line start with Invoked trigger...");
    let context = CompletionContext {
        trigger_kind: CompletionTriggerKind::INVOKED,
        trigger_character: None,
    };
    match client
        .completion(uri.clone(), Position::new(1, 0), Some(context))
        .await
    {
        Ok(Some(completion)) => {
            println!("   ✓ Got completion!");
            print_completion_items(&completion);
        }
        Ok(None) => println!("   ✗ No completion result"),
        Err(e) => println!("   ✗ Error: {}", e),
    }

    fn print_completion_items(completion: &lsp_types::CompletionResponse) {
        match completion {
            lsp_types::CompletionResponse::Array(items) => {
                println!("     Completion items: {}", items.len());
                for (i, item) in items.iter().take(5).enumerate() {
                    println!("       {}. {:?}", i, item.label);
                }
            }
            lsp_types::CompletionResponse::List(list) => {
                println!("     Completion list: {}", list.items.len());
                for (i, item) in list.items.iter().take(5).enumerate() {
                    println!("       {}. {:?}", i, item.label);
                }
            }
        }
    }

    println!("Shutting down...");
    let _ = client.shutdown().await;
    let _ = client.exit().await;

    println!("Test complete");
}
