//! Memory Integration Test
//!
//! Verify that the tiered memory system correctly persists and retrieves data
//! across shared, private, and project tiers.

use rustycode_orchestra::memory::{LayeredMemory, MemoryEntry, MemoryStore};
use std::sync::Arc;

#[tokio::test]
async fn test_layered_memory_persistence() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let memory = Arc::new(LayeredMemory::new(temp_dir.path().to_path_buf()));

    // 1. Test Project Memory
    let project_mem = memory.get_project();
    let goal = MemoryEntry {
        content: "build a great product".to_string(),
        last_accessed: chrono::Utc::now(),
        access_count: 0,
        created_at: chrono::Utc::now(),
    };
    project_mem.write("user_goal", goal.clone());
    
    // 2. Test Shared Memory
    let shared_mem = memory.get_shared();
    let task = MemoryEntry {
        content: "refactor engine".to_string(),
        last_accessed: chrono::Utc::now(),
        access_count: 0,
        created_at: chrono::Utc::now(),
    };
    shared_mem.write("current_task", task.clone());

    // 3. Test Private Memory
    let agent_id = "test_agent";
    let private_mem: Arc<dyn MemoryStore> = memory.get_private(agent_id).await;
    let scratchpad = MemoryEntry {
        content: "do it quietly".to_string(),
        last_accessed: chrono::Utc::now(),
        access_count: 0,
        created_at: chrono::Utc::now(),
    };
    private_mem.write("scratchpad", scratchpad.clone());

    // 4. Persistence check
    project_mem.persist()?;
    shared_mem.persist()?;
    private_mem.persist()?;

    assert_eq!(project_mem.read("user_goal").unwrap().content, "build a great product");
    assert_eq!(shared_mem.read("current_task").unwrap().content, "refactor engine");
    assert_eq!(private_mem.read("scratchpad").unwrap().content, "do it quietly");

    // 5. Test Compaction
    let mut old_goal = goal;
    old_goal.created_at = chrono::Utc::now() - chrono::Duration::hours(100);
    project_mem.write("old_goal", old_goal);
    
    project_mem.compact(1.0); // Should prune old_goal
    assert!(project_mem.read("old_goal").is_none());
    assert!(project_mem.read("user_goal").is_some());

    Ok(())
}
