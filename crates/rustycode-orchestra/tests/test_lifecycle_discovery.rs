#[cfg(test)]
mod lifecycle_test {
    use rustycode_orchestra::state_derivation::StateDeriver;
    use std::fs;

    #[test]
    fn test_lifecycle_discovery() {
        // Create test project
        let test_dir = tempfile::tempdir().unwrap();
        let project_root = test_dir.path();

        // Create .orchestra structure
        let orchestra_dir = project_root.join(".orchestra");
        let milestone_dir = orchestra_dir.join("milestones").join("M01");
        let slices_dir = milestone_dir.join("slices");

        fs::create_dir_all(slices_dir.join("S01").join("tasks").join("T01")).unwrap();
        fs::create_dir_all(slices_dir.join("S02").join("tasks")).unwrap();

        // Create ROADMAP.md - uses checkbox format for slices
        let roadmap = r#"# Milestone M01

## Slices

- [ ] S01: Project Setup
- [ ] S02: Implementation
"#;
        fs::write(milestone_dir.join("ROADMAP.md"), roadmap).unwrap();

        // Create S01 PLAN
        let plan = r#"# S01: Project Setup

**Goal:** Setup

## Tasks

- [ ] **T01: Create project** `est:10m`
  Create the project structure

- [ ] **T02: Add dependencies** `est:5m`
  Add required dependencies
"#;
        fs::write(slices_dir.join("S01").join("PLAN.md"), plan).unwrap();

        // Test discovery
        let deriver = StateDeriver::new(project_root.to_path_buf());
        let state = deriver.derive_state().expect("Should derive state");

        println!("\n=== Discovered State ===");
        println!(
            "Milestone: {:?}",
            state.active_milestone.as_ref().map(|m| &m.id)
        );
        println!("Slice: {:?}", state.active_slice.as_ref().map(|s| &s.id));
        println!("Task: {:?}", state.active_task.as_ref().map(|t| &t.id));

        // Assertions
        assert!(
            state.active_milestone.is_some(),
            "Should have active milestone"
        );
        assert!(state.active_slice.is_some(), "Should have active slice");
        assert!(state.active_task.is_some(), "Should have active task");

        assert_eq!(state.active_milestone.as_ref().unwrap().id, "M01");
        assert_eq!(state.active_slice.as_ref().unwrap().id, "S01");
        assert_eq!(state.active_task.as_ref().unwrap().id, "T01");

        // First task should NOT be done
        assert!(!state.active_task.as_ref().unwrap().done);
    }

    #[test]
    fn test_lifecycle_task_completion() {
        // Test that completing a task moves to the next one
        let test_dir = tempfile::tempdir().unwrap();
        let project_root = test_dir.path();

        let orchestra_dir = project_root.join(".orchestra");
        let milestone_dir = orchestra_dir.join("milestones").join("M01");
        let slices_dir = milestone_dir.join("slices");

        fs::create_dir_all(slices_dir.join("S01").join("tasks").join("T01")).unwrap();
        fs::create_dir_all(slices_dir.join("S01").join("tasks").join("T02")).unwrap();

        let roadmap = r#"# Milestone M01

## Slices

- [ ] S01: Project Setup
"#;
        fs::write(milestone_dir.join("ROADMAP.md"), roadmap).unwrap();

        // Create task plans - T01 done, T02 pending
        let plan = r#"# S01: Project Setup

**Goal:** Setup

## Tasks

- [x] **T01: Create project** `est:10m`
  Completed

- [ ] **T02: Add dependencies** `est:5m`
  Next task
"#;
        fs::write(slices_dir.join("S01").join("PLAN.md"), plan).unwrap();

        let deriver = StateDeriver::new(project_root.to_path_buf());
        let state = deriver.derive_state().expect("Should derive state");

        // Should skip T01 and find T02
        assert_eq!(state.active_task.as_ref().unwrap().id, "T02");
        assert!(!state.active_task.as_ref().unwrap().done);
    }

    #[test]
    fn test_lifecycle_slice_completion() {
        // Test that completing a slice moves to the next one
        let test_dir = tempfile::tempdir().unwrap();
        let project_root = test_dir.path();

        let orchestra_dir = project_root.join(".orchestra");
        let milestone_dir = orchestra_dir.join("milestones").join("M01");
        let slices_dir = milestone_dir.join("slices");

        // S01 with completed task
        fs::create_dir_all(slices_dir.join("S01").join("tasks").join("T01")).unwrap();
        // S02 with pending task
        fs::create_dir_all(slices_dir.join("S02").join("tasks").join("T01")).unwrap();

        let roadmap = r#"# Milestone M01

## Slices

- [x] S01: Project Setup
- [ ] S02: Implementation
"#;
        fs::write(milestone_dir.join("ROADMAP.md"), roadmap).unwrap();

        // S01 - all tasks done
        let plan_s01 = r#"# S01: Project Setup

**Goal:** Setup

## Tasks

- [x] **T01: Create project** `est:10m`
  Done
"#;
        fs::write(slices_dir.join("S01").join("PLAN.md"), plan_s01).unwrap();

        // S02 - pending task
        let plan_s02 = r#"# S02: Implementation

**Goal:** Build

## Tasks

- [ ] **T01: Implement feature** `est:30m`
  To do
"#;
        fs::write(slices_dir.join("S02").join("PLAN.md"), plan_s02).unwrap();

        let deriver = StateDeriver::new(project_root.to_path_buf());
        let state = deriver.derive_state().expect("Should derive state");

        // Should be on S02/T01
        assert_eq!(state.active_slice.as_ref().unwrap().id, "S02");
        assert_eq!(state.active_task.as_ref().unwrap().id, "T01");
    }

    #[test]
    fn test_lifecycle_all_complete() {
        // Test that no active task when everything is done
        let test_dir = tempfile::tempdir().unwrap();
        let project_root = test_dir.path();

        let orchestra_dir = project_root.join(".orchestra");
        let milestone_dir = orchestra_dir.join("milestones").join("M01");
        let slices_dir = milestone_dir.join("slices");

        fs::create_dir_all(slices_dir.join("S01").join("tasks").join("T01")).unwrap();

        let roadmap = r#"# Milestone M01

## Slices

- [x] S01: Project Setup
"#;
        fs::write(milestone_dir.join("ROADMAP.md"), roadmap).unwrap();

        let plan = r#"# S01: Project Setup

**Goal:** Setup

## Tasks

- [x] **T01: Create project** `est:10m`
  Done
"#;
        fs::write(slices_dir.join("S01").join("PLAN.md"), plan).unwrap();

        let deriver = StateDeriver::new(project_root.to_path_buf());
        let state = deriver.derive_state().expect("Should derive state");

        // No active task when all complete
        assert!(state.active_task.is_none());
        assert!(state.active_slice.is_none());
    }

    #[test]
    fn test_lifecycle_multiple_milestones() {
        // Test progression across milestones
        let test_dir = tempfile::tempdir().unwrap();
        let project_root = test_dir.path();

        let orchestra_dir = project_root.join(".orchestra");
        let m1_dir = orchestra_dir.join("milestones").join("M01");
        let m2_dir = orchestra_dir.join("milestones").join("M02");

        // M01 - complete
        let m1_slices = m1_dir.join("slices");
        fs::create_dir_all(m1_slices.join("S01").join("tasks").join("T01")).unwrap();

        // M02 - pending
        let m2_slices = m2_dir.join("slices");
        fs::create_dir_all(m2_slices.join("S01").join("tasks").join("T01")).unwrap();

        let roadmap_m1 = r#"# Milestone M01

## Slices

- [x] S01: Foundation
"#;
        fs::write(m1_dir.join("ROADMAP.md"), roadmap_m1).unwrap();

        let roadmap_m2 = r#"# Milestone M02

## Slices

- [ ] S01: Features
"#;
        fs::write(m2_dir.join("ROADMAP.md"), roadmap_m2).unwrap();

        let plan_m1 = r#"# S01: Foundation

**Goal:** Base

## Tasks

- [x] **T01: Setup** `est:10m`
  Done
"#;
        fs::write(m1_slices.join("S01").join("PLAN.md"), plan_m1).unwrap();

        let plan_m2 = r#"# S01: Features

**Goal:** Build

## Tasks

- [ ] **T01: Feature** `est:30m`
  Todo
"#;
        fs::write(m2_slices.join("S01").join("PLAN.md"), plan_m2).unwrap();

        let deriver = StateDeriver::new(project_root.to_path_buf());
        let state = deriver.derive_state().expect("Should derive state");

        // Should be on M02
        assert_eq!(state.active_milestone.as_ref().unwrap().id, "M02");
        assert_eq!(state.active_slice.as_ref().unwrap().id, "S01");
        assert_eq!(state.active_task.as_ref().unwrap().id, "T01");
    }
}
