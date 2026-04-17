# rustycode-git

Comprehensive git integration layer for RustyCode with complete git workflow support, conflict detection, and repository management.

## Features

- **Complete Git Operations**: Branch management, committing, merging, stashing, and more
- **Staging Operations**: Full staging area management with add, reset, and remove operations
- **Conflict Detection**: Advanced conflict detection and resolution guidance
- **Git Hooks**: Extensible hook system for custom workflows (pre-commit, post-merge, etc.)
- **Type-Safe Operations**: Enum-based operation types for compile-time safety
- **Comprehensive Testing**: Full test coverage with 64+ tests (36 unit tests + 28 doctests)
- **Thread-Safe**: Safe concurrent access to git operations
- **Rich Error Handling**: Detailed error messages with context using `anyhow` and `thiserror`

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
rustycode-git = { version = "0.1.0", path = "../rustycode-git" }
```

## Quick Start

```rust
use rustycode_git::GitClient;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // Create git client
    let git = GitClient::new(Path::new("/path/to/repo"))?;

    // Get repository status
    let status = git.get_status()?;
    println!("Current branch: {:?}", status.branch);

    // Create and switch to a new branch
    git.create_branch("feature-new", None)?;
    git.switch_branch("feature-new", false)?;

    // Stage and commit changes
    git.stage_files(&["src/main.rs"], false)?;
    git.commit_changes("Add new feature", None, None)?;

    // Get diff
    let diff = git.get_diff(None, false, None)?;
    println!("Diff:\n{}", diff);

    Ok(())
}
```

## Git Operations

### Branch Management

```rust
// Create a new branch
git.create_branch("feature-branch", None)?;

// Create from specific base
git.create_branch("feature-from-main", Some("main"))?;

// Switch branches
git.switch_branch("feature-branch", false)?;

// Force switch (discards changes)
git.switch_branch("main", true)?;

// List all branches
let branches = git.list_branches()?;

// Delete branch
git.delete_branch("old-feature", false)?;

// Force delete unmerged branch
git.delete_branch("unmerged", true)?;
```

### Commit Operations

```rust
// Standard commit
git.commit_changes("Add feature", None, None)?;

// Amend last commit
git.commit_changes("Fix typo", Some(true), None)?;

// Allow empty commit
git.commit_changes("Initial commit", None, Some(true))?;
```

### Staging Operations

```rust
// Stage specific files
git.stage_files(&["src/main.rs", "README.md"], false)?;

// Stage all changes
git.stage_all()?;

// Update index (only track already tracked files)
git.stage_files(&["*.rs"], true)?;

// Unstage files
git.unstage_files(&["src/main.rs"])?;

// Unstage all
git.unstage_all()?;
```

### Merge Operations

```rust
// Merge a branch
git.merge_branch("feature-branch", false, false)?;

// Merge without committing
git.merge_branch("feature", true, false)?;

// Squash merge
git.merge_branch("feature", false, true)?;

// Continue merge after resolving conflicts
git.continue_merge()?;

// Abort merge
git.abort_merge()?;
```

### Diff Operations

```rust
// Get unstaged changes
let diff = git.get_diff(None, false, None)?;

// Get staged changes
let diff = git.get_diff(None, true, None)?;

// Get diff for specific file
let diff = git.get_diff(Some("src/main.rs".to_string()), false, None)?;

// Get diff with more context
let diff = git.get_diff(None, false, Some(10))?;

// Get diff between commits
let diff = git.get_diff_commits("HEAD~1", "HEAD", None)?;
```

### Remote Operations

```rust
// Fetch from remote
git.fetch("origin", None)?;

// Fetch specific refspec
git.fetch("origin", Some("refs/heads/main:refs/remotes/origin/main"))?;

// Pull from remote
git.pull("origin", None)?;

// Pull specific branch
git.pull("origin", Some("main"))?;

// Push to remote
git.push("origin", "main", false)?;

// Force push
git.push("origin", "feature-branch", true)?;
```

### Stash Operations

```rust
// Stash changes with message
git.stash(Some("Work in progress"), false)?;

// Stash keeping index
git.stash(None, true)?;

// Pop most recent stash
git.stash_pop(None)?;

// Pop specific stash
git.stash_pop(Some("stash@{1}"))?;
```

### Reset Operations

```rust
use rustycode_git::ResetMode;

// Soft reset (keep changes)
git.reset(ResetMode::Soft, None)?;

// Mixed reset (default)
git.reset(ResetMode::Mixed, None)?;

// Hard reset (discard changes)
git.reset(ResetMode::Hard, Some("HEAD~1"))?;

// Merge reset
git.reset(ResetMode::Merge, None)?;

// Keep reset
git.reset(ResetMode::Keep, None)?;
```

### Rebase Operations

```rust
// Rebase onto upstream
git.rebase("main", None, false)?;

// Interactive rebase
git.rebase("main", None, true)?;

// Rebase specific branch
git.rebase("main", Some("feature"), false)?;
```

## Git Hooks

Implement custom hooks for workflow automation:

```rust
use rustycode_git::{GitClient, GitHook, GitHookType, HookContext, HookResult};
use anyhow::Result;

struct PreCommitHook;

impl GitHook for PreCommitHook {
    fn execute(&self, context: &HookContext) -> Result<HookResult> {
        // Run tests before commit
        println!("Running tests before commit...");

        Ok(HookResult {
            passed: true,
            output: "Tests passed".to_string(),
            error: None,
        })
    }

    fn hook_type(&self) -> GitHookType {
        GitHookType::PreCommit
    }
}

// Register hook
let git = GitClient::new(Path::new("/path/to/repo"))?;
git.register_hook(Box::new(PreCommitHook));
```

### Available Hook Types

- `PreCommit` - Before commit is created
- `PrePush` - Before pushing to remote
- `PreRebase` - Before rebase operation
- `CommitMsg` - Commit message validation
- `PostCommit` - After commit is created
- `PostMerge` - After merge completes
- `PostCheckout` - After checkout/branch switch
- `PreMerge` - Before merge operation

## Conflict Detection

The crate includes advanced conflict detection:

```rust
use rustycode_git::GitClient;

let git = GitClient::new(Path::new("/path/to/repo"))?;

// Get conflict detector
if let Some(detector) = git.conflict_detector() {
    // Detect current conflicts
    let report = detector.detect_conflicts()?;
    println!("Conflicts found: {}", report.conflict_count());

    // Check for conflicts with a branch before merging
    let report = detector.detect_conflicts_with_branch("feature-branch")?;
    if report.conflict_count() > 0 {
        println!("Potential conflicts detected!");
        for conflict in &report.conflicts {
            println!("  - {} ({})", conflict.file_path.display(), conflict.conflict_type);
            println!("    Severity: {}", conflict.severity);
            println!("    Suggestion: {}", conflict.resolution_strategy);
        }
    }
}
```

### Conflict Types

- **MarkerConflict**: Standard git conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`)
- **BothModified**: Same file modified in both branches
- **DeleteModify**: File deleted in one branch, modified in another
- **RenameModify**: File renamed in one branch, modified in another
- **BinaryConflict**: Binary file conflicts
- **SubmoduleConflict**: Submodule reference conflicts

### Severity Levels

- **Low**: Minor conflicts, easy to resolve
- **Medium**: Standard conflicts requiring manual intervention
- **High**: Complex conflicts involving significant changes
- **Critical**: Binary or submodule conflicts that block merging

## Type-Safe Operations

All operations are represented by the `GitOperation` enum:

```rust
use rustycode_git::GitOperation;

let operation = GitOperation::CreateBranch {
    name: "feature".to_string(),
    base: None,
};

println!("Operation: {}", operation);
// Output: create_branch(feature)
```

## Error Handling

All operations return `Result<T>` for proper error handling:

```rust
use rustycode_git::{GitError, GitClient};

let git = GitClient::new(Path::new("/path/to/repo"))?;

match git.switch_branch("nonexistent", false) {
    Ok(result) => println!("Success: {:?}", result),
    Err(e) => {
        if let Some(git_err) = e.downcast_ref::<GitError>() {
            match git_err {
                GitError::BranchNotFound(name) => {
                    println!("Branch '{}' not found", name);
                }
                GitError::ConflictDetected(msg) => {
                    println!("Conflict detected: {}", msg);
                }
                _ => println!("Git error: {}", git_err),
            }
        }
    }
}
```

## Git Status Inspection

```rust
use rustycode_git::inspect;

let status = inspect(Path::new("/path/to/repo"))?;

println!("Repository root: {:?}", status.root);
println!("Current branch: {:?}", status.branch);
println!("Is dirty: {:?}", status.dirty);
println!("Is worktree: {}", status.worktree);
```

## Testing

Run tests:

```bash
cargo test -p rustycode-git
```

Run tests with output:

```bash
cargo test -p rustycode-git -- --nocapture
```

Run with race detection:

```bash
cargo test -p rustycode-git -- --test-threads=1
```

## Test Coverage

The crate includes comprehensive test coverage:

- **36 unit tests** covering all major operations
- **28 doctests** documenting usage examples
- **Edge case handling** for error conditions
- **Integration tests** for complete workflows

## Thread Safety

`GitClient` uses internal synchronization (`Arc<RwLock<...>>`) for thread-safe hook management. Git operations execute external commands and should be synchronized externally if used concurrently.

## Implementation Details

### Architecture

The git integration is organized into several layers:

1. **GitOperation Type**: Type-safe enum representing all git operations
2. **GitClient**: Main client for executing git operations
3. **Staging Operations**: Specialized methods for staging area management
4. **Conflict Detection**: Integration with conflict detection system
5. **Git Hooks**: Extensible hook system for custom workflows

### Performance

- Efficient git command execution via std::process::Command
- Minimal overhead for operation tracking
- Thread-safe hook management
- Fast conflict detection using git status parsing

## License

MIT

## Contributing

Contributions are welcome! Please ensure:

- All tests pass: `cargo test -p rustycode-git`
- Code is formatted: `cargo fmt --package rustycode-git`
- Clippy warnings are addressed: `cargo clippy --package rustycode-git`

## Related Crates

- `rustycode-protocol`: Shared types and protocols
- `rustycode-id`: Unique identifier generation
- `rustycode-storage`: Persistent storage layer
- `rustycode-bus`: Event bus system
