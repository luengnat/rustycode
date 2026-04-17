// Autonomous Mode Autonomous Execution Demo
//
// This example demonstrates the complete Autonomous Mode autonomous execution engine.
//
// Usage:
//   1. Set ANTHROPIC_API_KEY
//   2. cargo run --example orchestra_demo
//
// The executor will:
//   1. Read .orchestra/STATE.md to find current unit
//   2. Pre-load context (plans, summaries)
//   3. Execute with LLM (streaming)
//   4. Detect and execute tools
//   5. Continue multi-turn conversations
//   6. Write summary when complete
//   7. Update STATE.md with next unit
//   8. Loop to next unit

use rustycode_orchestra::WorkingExecutor;
use rustycode_llm::{create_provider_v2, load_provider_type_from_config, load_model_from_config};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Autonomous Mode Autonomous Execution Demo");
    println!("================================\n");

    // Get project directory from command line or use default
    let project_root = std::env::current_dir()?;

    println!("📁 Project: {}", project_root.display());

    // Check if .orchestra/STATE.md exists
    let state_path = project_root.join(".orchestra/STATE.md");
    if !state_path.exists() {
        println!("❌ Error: .orchestra/STATE.md not found in project directory");
        println!("\n💡 To set up a Orchestra project:");
        println!("   1. Create .orchestra/STATE.md");
        println!("   2. Create .orchestra/milestones/{M}/slices/{S}/tasks/{T}-PLAN.md");
        println!("   3. Run this demo again");
        println!("\n📖 See Orchestra_V2_IMPLEMENTATION_GUIDE.md for details");
        return Ok(());
    }

    // Load LLM configuration
    let provider_type = load_provider_type_from_config()?;
    let model = load_model_from_config()?;

    println!("🤖 Provider: {}", provider_type);
    println!("🧠 Model: {}", model);

    // Create LLM provider
    let provider = create_provider_v2(&provider_type, &model)?;

    println!("\n🔧 Creating executor...");
    let executor = WorkingExecutor::new(
        project_root.clone(),
        provider, // Don't double-wrap with Arc::new
        model,
    );

    println!("✅ Executor created");
    println!("\n🎯 Starting autonomous execution...");
    println!("   The executor will:");
    println!("   • Read .orchestra/STATE.md");
    println!("   • Pre-load context (plans, summaries)");
    println!("   • Execute with LLM");
    println!("   • Detect and execute tools");
    println!("   • Write summaries");
    println!("   • Update STATE.md");
    println!("   • Loop to next unit");
    println!("\n⏳ Running...\n");

    // Run the executor
    executor.run().await?;

    println!("\n✅ Execution complete!");

    Ok(())
}
