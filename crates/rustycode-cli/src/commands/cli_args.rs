//! CLI argument definitions for rustycode
//!
//! This module contains all clap subcommand enums, keeping them separate
//! from the handler logic in main.rs.

use clap::Subcommand;

/// Configuration management commands
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum ConfigCommand {
    /// Show current configuration
    #[command(about = "Show current configuration")]
    Show,
    /// Get a specific configuration value
    #[command(about = "Get a specific configuration value")]
    Get {
        /// Configuration key (e.g., "model", "provider", "log_level")
        key: String,
    },
    /// Set a configuration value
    #[command(about = "Set a configuration value")]
    Set {
        /// Configuration key (e.g., "model", "provider", "log_level")
        key: String,
        /// Configuration value
        value: String,
    },
}

/// Tool management commands
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum ToolsCommand {
    /// List all registered tools with their descriptions.
    #[command(about = "List all registered tools with their descriptions")]
    List,
    /// Invoke a tool directly and print the result.
    #[command(about = "Invoke a tool directly and print the result")]
    Call {
        /// Tool name (e.g. read_file, bash, git_status)
        name: String,
        /// JSON object of parameters (e.g. '{"path":"src/main.rs"}')
        #[arg(long, default_value = "{}")]
        params: String,
    },
}

/// Git worktree management commands
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum WorktreeCommand {
    /// Create a new git worktree for isolated development
    #[command(about = "Create a new git worktree for isolated development")]
    Create {
        /// Name for the worktree (used for directory name)
        name: String,
        /// Branch name (default: feature/`<name>`)
        #[arg(short, long)]
        branch: Option<String>,
        /// Worktree type: session, feature, bugfix, experiment (default: feature)
        #[arg(long, default_value = "feature")]
        worktree_type: String,
    },
    /// List all worktrees with their status
    #[command(about = "List all worktrees with their status")]
    List {
        /// Show detailed information
        #[arg(long)]
        detailed: bool,
    },
    /// Delete a worktree and clean up its branch
    #[command(about = "Delete a worktree and clean up its branch")]
    Delete {
        /// Worktree name or ID
        name: String,
        /// Force deletion even if worktree has changes
        #[arg(long)]
        force: bool,
        /// Keep the branch after deleting worktree
        #[arg(long)]
        keep_branch: bool,
    },
    /// Prune stale worktrees (auto-cleanup)
    #[command(about = "Prune stale worktrees (auto-cleanup)")]
    Prune {
        /// Maximum age in days (default: 30)
        #[arg(long, default_value_t = 30)]
        max_age_days: usize,
    },
}

/// Session management commands
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum SessionsCommand {
    /// List recent sessions
    #[command(about = "List recent sessions")]
    List {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Show details of a specific session
    #[command(about = "Show details of a specific session")]
    Show { id: String },
}

#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum PlanCommand {
    /// Preview the plan for a task without executing it.
    Preview {
        /// Task description to generate a plan for.
        task: String,
    },
    /// Create a new planning session and write a skeleton plan.md.
    #[command(about = "Create a new planning session and write a skeleton plan.md")]
    New {
        /// Task description to plan for.
        task: String,
    },
    /// List recent plans (most recent first).
    #[command(about = "List recent plans (most recent first)")]
    List {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Show details of a plan by its UUID.
    #[command(about = "Show details of a plan by its UUID")]
    Show {
        /// Plan UUID (from `plan list` output).
        id: String,
    },
    /// Approve a plan and transition the session to Executing mode.
    #[command(about = "Approve a plan and transition the session to Executing mode")]
    Approve {
        /// Session ID (from `plan new` output).
        session_id: String,
    },
    /// Reject a plan.
    #[command(about = "Reject a plan")]
    Reject {
        /// Session ID (from `plan new` output).
        session_id: String,
    },
    /// Execute the next step in an approved plan.
    #[command(about = "Execute the next step in an approved plan")]
    Execute {
        /// Session ID for the plan to execute.
        session_id: String,
    },
    /// Check execution status of an approved plan.
    #[command(about = "Check execution status of an approved plan")]
    Status {
        /// Session ID for the plan to check.
        session_id: String,
    },
}

#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum AgentCommand {
    /// Start a new agentic session for autonomous task execution.
    New {
        /// Task description for the agent.
        task: String,
        /// Working mode: code, debug, ask, orchestrate, plan, test, team
        #[arg(long, default_value = "auto")]
        mode: Option<String>,
    },
    /// Execute one step in an agentic session.
    #[command(about = "Execute one step in an agentic session")]
    Step {
        /// Session ID to continue.
        session_id: String,
    },
    /// Reset/clear an agentic session.
    #[command(about = "Reset/clear an agentic session")]
    Reset {
        /// Session ID to reset.
        session_id: String,
    },
}

/// OMO (One Model Outperforms Many) - multi-agent analysis commands
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum OmoCommand {
    /// Analyze code with multiple specialized agents in parallel.
    #[command(about = "Analyze code with multiple specialized agents in parallel")]
    Analyze {
        /// File path to analyze (optional: reads from stdin if not provided).
        #[arg(short, long)]
        file: Option<String>,
        /// Specific agent roles to use (comma-separated). If not specified, all roles are used.
        #[arg(short, long, value_delimiter = ',')]
        roles: Option<Vec<String>>,
        /// Maximum number of agents to run in parallel (default: 5).
        #[arg(short, long, default_value_t = 5)]
        parallelism: usize,
        /// Additional context for the analysis.
        #[arg(short, long)]
        context: Option<String>,
        /// Additional instructions for agents.
        #[arg(short, long)]
        instructions: Option<String>,
    },
    /// List available agent roles.
    #[command(about = "List available agent roles")]
    ListRoles,
}

/// Orchestra (Get Sh*t Done) - project management and execution commands
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum OrchestraCommand {
    /// Initialize a new Orchestra project
    #[command(about = "Initialize a new Orchestra project")]
    Init {
        /// Project name
        name: String,
        /// Project description
        #[arg(long)]
        description: String,
        /// Project vision
        #[arg(long)]
        vision: String,
    },
    /// Show current project progress
    #[command(about = "Show current project progress")]
    Progress,
    /// Show detailed current state
    #[command(about = "Show detailed current state")]
    State,
    /// Create a new milestone
    #[command(about = "Create a new milestone")]
    NewMilestone {
        /// Milestone ID (e.g., M001)
        id: String,
        /// Milestone title
        title: String,
        /// Milestone vision
        #[arg(long)]
        vision: String,
    },
    /// List all milestones
    #[command(about = "List all milestones")]
    ListMilestones,
    /// Plan a new phase (slice)
    #[command(about = "Plan a new phase (slice)")]
    PlanPhase {
        /// Phase ID (e.g., S01)
        id: String,
        /// Phase title
        title: String,
        /// Phase goal
        #[arg(long)]
        goal: String,
        /// Demo scenario
        #[arg(long)]
        demo: String,
        /// Risk level: high, medium, or low (default: medium)
        #[arg(long, default_value = "medium")]
        risk: String,
    },
    /// Execute autonomous development workflow (Autonomous Mode style)
    #[command(about = "Execute autonomous development workflow (Autonomous Mode style)")]
    Auto {
        /// Budget limit in USD (default: 100)
        #[arg(long, default_value = "100")]
        budget: f64,
        /// Maximum number of units to execute (default: 100)
        #[arg(long, default_value = "100")]
        max_units: u32,
    },
    /// Execute current phase
    #[command(about = "Execute current phase")]
    ExecutePhase,
    /// Verify current phase
    #[command(about = "Verify current phase")]
    VerifyPhase,
    /// Show Orchestra documentation
    #[command(about = "Show Orchestra documentation")]
    Docs,
    /// Check project health
    #[command(about = "Check project health")]
    Health,
    /// Execute a quick task
    #[command(about = "Execute a quick task")]
    Quick {
        /// Task description
        task: String,
    },
    /// Add a new phase to the milestone
    #[command(about = "Add a new phase to the milestone")]
    AddPhase {
        /// Phase ID (e.g., S02)
        id: String,
        /// Phase title
        title: String,
        /// Phase goal
        #[arg(long)]
        goal: String,
        /// Demo scenario
        #[arg(long)]
        demo: String,
        /// Risk level: high, medium, or low (default: medium)
        #[arg(long, default_value = "medium")]
        risk: String,
    },
    /// Insert a phase between existing phases
    #[command(about = "Insert a phase between existing phases")]
    InsertPhase {
        /// Phase ID (e.g., S72.1)
        id: String,
        /// Phase title
        title: String,
        /// Phase goal
        #[arg(long)]
        goal: String,
        /// Insert after this phase ID
        #[arg(long)]
        after_phase: String,
        /// Risk level: high, medium, or low (default: medium)
        #[arg(long, default_value = "medium")]
        risk: String,
    },
    /// Remove a phase
    #[command(about = "Remove a phase")]
    RemovePhase {
        /// Phase ID to remove
        id: String,
    },
    /// Complete and archive current milestone
    #[command(about = "Complete and archive current milestone")]
    CompleteMilestone {
        /// Milestone ID
        id: String,
    },
    /// Cleanup old activity files
    #[command(about = "Cleanup old activity files")]
    Cleanup {
        /// Maximum age in days (default: 30)
        #[arg(long, default_value_t = 30)]
        max_age_days: usize,
    },
    /// Add a todo item
    #[command(about = "Add a todo item")]
    AddTodo {
        /// Todo description
        description: String,
    },
    /// List pending todos
    #[command(about = "List pending todos")]
    ListTodos,
    /// Complete a todo
    #[command(about = "Complete a todo")]
    CompleteTodo {
        /// Todo description to match
        description: String,
    },
    /// Remove completed todos
    #[command(about = "Remove completed todos")]
    CleanupTodos,
    /// Set model profile (quality/balanced/budget)
    #[command(about = "Set model profile (quality/balanced/budget)")]
    SetProfile {
        /// Model profile: quality, balanced, or budget
        profile: String,
    },
    /// Show current configuration
    #[command(about = "Show current configuration")]
    ShowConfig,
    /// Plan a phase with AI assistance
    #[command(about = "Plan a phase with AI assistance")]
    AgentPlan {
        /// Phase ID (e.g., S01)
        id: String,
        /// Milestone ID (e.g., M001)
        #[arg(long)]
        milestone: String,
        /// Phase title
        title: String,
        /// Phase goal
        #[arg(long)]
        goal: String,
        /// Demo scenario
        #[arg(long)]
        demo: String,
        /// Risk level: high, medium, or low (default: medium)
        #[arg(long, default_value = "medium")]
        risk: String,
    },
    /// Execute a phase with autonomous agents
    #[command(about = "Execute a phase with autonomous agents")]
    AgentExecute {
        /// Phase ID (e.g., S01)
        id: String,
        /// Milestone ID (e.g., M001)
        #[arg(long)]
        milestone: String,
    },
    /// Verify a phase with AI assistance
    #[command(about = "Verify a phase with AI assistance")]
    AgentVerify {
        /// Phase ID (e.g., S01)
        id: String,
        /// Milestone ID (e.g., M001)
        #[arg(long)]
        milestone: String,
    },
    /// Map codebase structure and organization
    #[command(about = "Map codebase structure and organization")]
    MapCodebase,
    /// Add tests for completed work
    #[command(about = "Add tests for completed work")]
    AddTests {
        /// Phase ID (e.g., S01)
        id: String,
    },
    /// Diagnose issues in the project
    #[command(about = "Diagnose issues in the project")]
    DiagnoseIssues,
    /// Research phase before planning
    #[command(about = "Research phase before planning")]
    ResearchPhase {
        /// Phase ID (e.g., S01)
        id: String,
        /// Research topic
        topic: String,
    },
    /// Pause work and save context
    #[command(about = "Pause work and save context")]
    PauseWork {
        /// Optional note about what you're working on
        #[arg(long)]
        note: Option<String>,
    },
    /// Resume work from previous pause
    #[command(about = "Resume work from previous pause")]
    ResumeWork,
    /// Discuss phase through interactive questioning
    #[command(about = "Discuss phase through interactive questioning")]
    DiscussPhase {
        /// Phase ID (e.g., S01)
        id: String,
    },
    /// Enhanced project initialization with comprehensive context
    #[command(about = "Enhanced project initialization with comprehensive context")]
    NewProjectEnhanced {
        /// Project name
        name: String,
        /// Project description
        #[arg(long)]
        description: String,
        /// Project vision
        #[arg(long)]
        vision: String,
        /// Interactive mode with tips
        #[arg(long)]
        interactive: bool,
    },
    /// Plan milestone gaps - identify missing phases
    #[command(about = "Plan milestone gaps - identify missing phases")]
    PlanMilestoneGaps {
        /// Milestone ID (e.g., M001)
        id: String,
    },
    /// Suggest next workflows based on current state
    #[command(about = "Suggest next workflows based on current state")]
    Suggest,
    /// Visualize project progress with ASCII art
    #[command(about = "Visualize project progress with ASCII art")]
    Visualize,
    /// Execute a workflow chain (multiple workflows in sequence)
    #[command(about = "Execute a workflow chain (multiple workflows in sequence)")]
    Chain {
        /// Chain name (full-milestone-setup, health-check, or custom chain)
        name: String,
        /// Chain arguments
        #[arg(raw = true)]
        args: Vec<String>,
        /// Interactive mode with confirmations
        #[arg(long)]
        interactive: bool,
        /// Dry run mode (preview without executing)
        #[arg(long)]
        dry_run: bool,
        /// Verbose mode (detailed output)
        #[arg(long)]
        verbose: bool,
    },
    /// List all available chains
    #[command(about = "List all available chains")]
    ListChains,
    /// Create a new custom chain template
    #[command(about = "Create a new custom chain template")]
    CreateChain {
        /// Chain name
        name: String,
    },
    /// List available chain templates
    #[command(about = "List available chain templates")]
    ListChainTemplates,
    /// Create chain from template
    #[command(about = "Create chain from template")]
    CreateChainFromTemplate {
        /// Template name
        template: String,
        /// New chain name
        name: String,
        /// Template variables (e.g., phase_id=S03)
        #[arg(short = 'v', long, value_name = "KEY=VALUE", num_args = 0..)]
        vars: Vec<String>,
    },
    /// Validate a custom chain
    #[command(about = "Validate a custom chain")]
    ValidateChain {
        /// Chain name
        name: String,
    },
    /// Export a chain to formatted documentation
    #[command(about = "Export a chain to formatted documentation")]
    ExportChain {
        /// Chain name
        name: String,
        /// Output format (markdown, text, json)
        #[arg(short = 'f', long, default_value = "markdown")]
        format: String,
        /// Output file path (optional)
        #[arg(short = 'o', long)]
        output: Option<String>,
    },
    /// Show chain execution statistics
    #[command(about = "Show chain execution statistics")]
    ChainStats {
        /// Chain name (optional, shows all if not specified)
        name: Option<String>,
    },
    /// Reset chain execution statistics
    #[command(about = "Reset chain execution statistics")]
    ResetChainStats {
        /// Chain name (optional, resets all if not specified)
        name: Option<String>,
    },
    /// Compare two chains
    #[command(about = "Compare two chains")]
    CompareChains {
        /// First chain name
        chain1: String,
        /// Second chain name
        chain2: String,
    },
    /// Bulk export all chains
    #[command(about = "Bulk export all chains")]
    BulkExportChains {
        /// Output format (markdown, text, json)
        #[arg(short = 'f', long, default_value = "markdown")]
        format: String,
        /// Output directory (optional)
        #[arg(short = 'o', long)]
        output_dir: Option<String>,
    },
    /// Bulk validate all chains
    #[command(about = "Bulk validate all chains")]
    BulkValidateChains,
}

/// Parse key=value pairs
pub fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let mut parts = s.splitn(2, '=');
    let key = parts.next().ok_or("Missing key".to_string())?;
    let value = parts.next().ok_or("Missing value".to_string())?;
    Ok((key.to_string(), value.to_string()))
}

/// Event bus monitoring commands
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum EventsCommand {
    /// Watch events in real-time with optional filtering
    #[command(about = "Watch events in real-time with optional filtering")]
    Watch {
        #[arg(long, default_value = "*")]
        pattern: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        tool: Option<String>,
        #[arg(long)]
        plan: Option<String>,
        #[arg(long)]
        approve_session: Option<String>,
        #[arg(long)]
        reject_session: Option<String>,
        #[arg(long, default_value = "{}")]
        params: String,
    },
}

/// History command for conversation history management
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum HistoryCommand {
    /// List recent conversations (most recent first)
    #[command(about = "List recent conversations (most recent first)")]
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Search conversations by query text, tags, model, or date range
    #[command(about = "Search conversations by query text, tags, model, or date range")]
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
    },
    /// Show full details of a specific conversation
    #[command(about = "Show full details of a specific conversation")]
    Show { id: String },
    /// Export a conversation to JSON or Markdown
    #[command(about = "Export a conversation to JSON or Markdown")]
    Export {
        id: String,
        #[arg(long, default_value = "markdown")]
        format: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Delete a conversation
    #[command(about = "Delete a conversation")]
    Delete {
        id: String,
        #[arg(long)]
        force: bool,
    },
}

/// Skills command for skill management
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum SkillsCommand {
    /// List all available skills
    #[command(about = "List all available skills")]
    List {
        #[arg(long)]
        detailed: bool,
    },
    /// Run a skill with variable substitution
    #[command(about = "Run a skill with variable substitution")]
    Run {
        name: String,
        #[arg(short = 'v', long, value_name = "KEY=VALUE", num_args = 0..)]
        vars: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Create a new custom skill from a template
    #[command(about = "Create a new custom skill from a template")]
    Create {
        name: String,
        #[arg(long)]
        description: String,
        #[arg(long)]
        prompt: String,
        #[arg(long)]
        variables: Option<String>,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Validate a skill definition file
    #[command(about = "Validate a skill definition file")]
    Validate { path: String },
}

/// Team learnings management (view, add, remove project memory)
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum LearningsCommand {
    /// Show all project learnings
    #[command(about = "Show all project learnings")]
    Show,
    /// List all project learnings (alias for show)
    #[command(about = "List all project learnings (alias for show)")]
    List,
    /// Add a new learning
    #[command(about = "Add a new learning")]
    Add {
        /// Learning category: user-preference, codebase-quirk, what-worked, what-failed
        #[arg(long, default_value = "what-worked")]
        category: String,
        /// Learning content
        content: String,
    },
    /// Remove a learning by content match
    #[command(about = "Remove a learning by content match")]
    Remove {
        /// Learning category
        #[arg(long)]
        category: String,
        /// Learning content to match
        content: String,
    },
    /// Clear all learnings (use with caution)
    #[command(about = "Clear all learnings (use with caution)")]
    Clear {
        /// Require confirmation
        #[arg(long)]
        yes: bool,
    },
}

/// Memory debugging and management commands
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum MemoryCommand {
    /// Show memory metrics and effectiveness report
    #[command(about = "Show memory metrics and effectiveness report")]
    Stats,
    /// Search memory with a query
    #[command(about = "Search memory with a query")]
    Search {
        /// Query string to search for
        query: String,
        /// Maximum number of results
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// List all memories
    #[command(about = "List all memories")]
    List {
        /// Filter by memory type
        #[arg(short, long)]
        memory_type: Option<String>,
    },
    /// Show details of a specific memory
    #[command(about = "Show details of a specific memory")]
    Show {
        /// Memory ID to show
        memory_id: String,
    },
    /// Test memory retrieval with a query
    #[command(about = "Test memory retrieval with a query")]
    Test {
        /// Query to test
        query: String,
    },
    /// Export memories to a file
    #[command(about = "Export memories to a file")]
    Export {
        /// Path to export JSON file
        path: std::path::PathBuf,
    },
    /// Import memories from a file
    #[command(about = "Import memories from a file")]
    Import {
        /// Path to import JSON file
        path: std::path::PathBuf,
    },
    /// Reset/prune unused memories
    #[command(about = "Reset/prune unused memories")]
    Prune {
        /// Show what would be pruned without making changes
        #[arg(short, long)]
        dry_run: bool,
    },
}

/// SWE-bench evaluation arguments (used by the Swebench subcommand in main.rs)
#[derive(Debug, clap::Args)]
pub struct SweBenchCliArgs {
    /// Path to SWE-bench instances JSON file
    #[arg(long)]
    pub instances: std::path::PathBuf,

    /// Output path for predictions
    #[arg(long, default_value = "predictions.json")]
    pub output: std::path::PathBuf,

    /// Cost budget per instance (dollars)
    #[arg(long, default_value = "0.50")]
    pub budget: f64,

    /// Number of instances to run in parallel
    #[arg(long, default_value = "1")]
    pub parallel: usize,

    /// Specific instance IDs to run (comma-separated)
    #[arg(long)]
    pub instance_ids: Option<String>,

    /// Output format: json (array) or jsonl (one per line)
    #[arg(long, default_value = "json")]
    pub format: String,
}

/// Harness framework commands for long-running agent tasks
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum HarnessCommand {
    /// Initialize harness files in a project directory
    #[command(about = "Initialize harness files in a project directory")]
    Init {
        /// Project directory (default: current directory)
        #[arg(default_value = ".")]
        path: String,
    },
    /// Start/resume harness execution
    #[command(about = "Start/resume harness execution")]
    Run,
    /// Show current harness status and progress
    #[command(about = "Show current harness status and progress")]
    Status,
    /// Add a task to the harness task list
    #[command(about = "Add a task to the harness task list")]
    Add {
        /// Task description
        description: String,
        /// Task priority: P0, P1, P2 (default: P1)
        #[arg(long, default_value = "P1")]
        priority: String,
        /// Validation command to verify task completion
        #[arg(long)]
        validation: Option<String>,
        /// Validation timeout in seconds (default: 120)
        #[arg(long, default_value = "120")]
        timeout: u64,
        /// Comma-separated list of task IDs this depends on
        #[arg(long)]
        depends_on: Option<String>,
    },
}

/// Benchmark runner commands for agent evaluation.
#[derive(Debug, Subcommand)]
#[non_exhaustive]
pub enum BenchCommand {
    /// Run benchmark tasks against an agent.
    #[command(about = "Run benchmark tasks against an agent")]
    Run {
        /// Dataset name or `name@version` (searches ~/.cache/harbor/tasks/)
        #[arg(long)]
        dataset: Option<String>,
        /// Direct path to a task or dataset directory
        #[arg(long)]
        path: Option<std::path::PathBuf>,
        /// Agent type: oracle, nop, code (default: oracle)
        #[arg(long, default_value = "oracle")]
        agent: String,
        /// Model to use for code agent (e.g. claude-sonnet-4-6, gpt-4o)
        #[arg(long, default_value = "claude-sonnet-4-6")]
        model: String,
        /// LLM provider: anthropic, openai (default: anthropic)
        #[arg(long, default_value = "anthropic")]
        provider: String,
        /// Number of concurrent trials (default: 1)
        #[arg(long, default_value_t = 1)]
        n_concurrent: usize,
        /// Force rebuild Docker images from Dockerfile
        #[arg(long)]
        force_build: bool,
        /// Remove containers after each trial
        #[arg(long)]
        cleanup: bool,
        /// Job name (default: bench-YYYYMMDD-HHMMSS)
        #[arg(long)]
        job_name: Option<String>,
        /// Directory for job output (default: ./jobs)
        #[arg(long)]
        jobs_dir: Option<std::path::PathBuf>,
        /// Max tool-use turns for code agent (default: 30)
        #[arg(long, default_value_t = 30)]
        max_turns: usize,
        /// Max tokens per LLM response for code agent (default: 16384)
        #[arg(long, default_value_t = 16_384)]
        max_tokens: u32,
        /// Command timeout in seconds for code agent (default: 300)
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Show results from a completed or interrupted benchmark run.
    #[command(about = "Show results from a benchmark run")]
    Results {
        /// Path to the job directory
        job_dir: std::path::PathBuf,
    },
    /// List available datasets in Harbor cache.
    #[command(about = "List available datasets")]
    ListDatasets,
}
