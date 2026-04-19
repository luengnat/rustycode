// rustycode-orchestra/src/lib.rs
//! Orchestra v2 - Get Stuff Done Methodology Framework
//!
//! Complete rewrite with improved workflows, better LLM integration,
//! and support for both interactive and autonomous execution modes.

#![warn(clippy::all)]

/// Crate-level test serialization lock.
/// Many tests share global singleton state (activity log, tool tracking).
/// This lock prevents race conditions when `cargo test` runs them in parallel.
///
/// Uses `parking_lot::Mutex` which does not poison on panic — avoids cascading
/// failures where one panicked test poisons the lock for all subsequent tests.
#[cfg(test)]
pub(crate) static CRATE_TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub mod auto;
pub mod auto_mode;
pub mod auto_runtime;
pub mod cli;
pub mod complexity;
pub mod conversation_runtime;
pub mod debug;
pub mod engine;
pub mod llm;
pub mod model_router;
pub mod orchestra_config;
pub mod orchestra_executor;
pub mod orchestra_service;
pub mod progress;
pub mod state;
pub mod task_execution_runtime;
pub mod tools;
pub mod workflow;
pub mod worktree;

// New phase-based execution system
pub mod crash_recovery;
pub mod phases;
pub mod request_dedup;

// Critical production features
pub mod activity_log;
pub mod atomic_write;
pub mod auto_budget;
pub mod auto_idempotency;
pub mod auto_model_selection;
pub mod auto_observability;
pub mod auto_post_unit;
pub mod auto_prompts;
pub mod auto_recovery;
pub mod auto_supervisor;
pub mod auto_timers;
pub mod auto_tool_tracking;
pub mod budget;
pub mod cache;
pub mod captures;
pub mod collision_diagnostics;
pub mod complexity_classifier;
pub mod constants;
pub mod context_budget;
pub mod git_constants;
pub mod input_classifier;
pub mod json_persistence;
pub mod jsonl_utils;
pub mod metrics;
pub mod migrate_external;
pub mod migrate_preview;
pub mod migrate_validator;
pub mod model_cost_table;
pub mod observability_validator;
pub mod orchestra_tools;
pub mod paths;
pub mod plan_mode;
pub mod tool_access_matrix;
pub mod prompt_compressor;
pub mod routing_history;
pub mod safe_fs;
pub mod semantic_chunker;
pub mod semver;
pub mod session_context;
pub mod session_forensics;
pub mod session_status_io;
pub mod skill_discovery;
pub mod state_derivation;
pub mod summary_distiller;
pub mod timeout;
pub mod token_counter;
pub mod tool_tracking;
pub mod unit_closeout;
pub mod unit_runtime;
pub mod verification;
pub mod verification_gate;

// Fixture-based testing system
pub mod app_paths;
pub mod auto_direct_dispatch;
pub mod auto_stuck_detection;
pub mod auto_worktree_sync;
pub mod bundled_extension_paths;
pub mod commands_config;
pub mod debug_logger;
pub mod detection;
pub mod diff_context;
pub mod dispatch_guard;
pub mod doctor_format;
pub mod doctor_types;
pub mod exit_command;
pub mod export;
pub mod extension_discovery;
pub mod extension_registry;
pub mod files;
pub mod fixture;
pub mod format_utils;
pub mod frontmatter;
pub mod git_self_heal;
pub mod gitignore;
pub mod headless_context;
pub mod headless_events;
pub mod help_text;
pub mod history;
pub mod logo;
pub mod lru_ttl_cache;
pub mod milestone_actions;
pub mod milestone_ids;
pub mod models_resolver;
pub mod namespaced_resolver;
pub mod notifications;
pub mod path_display;
pub mod pi_migration;
pub mod plan_slice_runtime;
pub mod post_unit_runtime;
pub mod project_bootstrap;
pub mod prompt_cache_optimizer;
pub mod prompt_loader;
pub mod prompt_ordering;
pub mod provider_error_pause;
pub mod queue_order;
pub mod quick;
pub mod remote_questions_config;
pub mod remote_questions_format;
pub mod repo_identity;
pub mod roadmap_slices;
pub mod sanitize;
pub mod skill_telemetry;
pub mod structured_data_formatter;
pub mod task_control_runtime;
pub mod task_verification_runtime;
pub mod terminal;
pub mod tool_bootstrap;
pub mod ttsr_rule_loader;
pub mod unit_lifecycle_runtime;
pub mod universal_config_tools;
pub mod universal_config_types;
pub mod url_utils;
pub mod validate_directory;
pub mod verification_evidence;
pub mod verification_retry_state;
pub mod wizard;
pub mod workspace_index;
pub mod worktree_name_gen;

// SWE-bench evaluation adapter
pub mod swebench;

// Gastown-inspired tracking systems
pub mod agent_identity;
pub mod convoy;
pub mod formulas;
pub mod seance;

pub use auto::{AutoConfig, AutoMode, AutoTaskResult};
pub use auto_mode::{
    AutoMode as AutoModeStateMachine, AutoModeConfig, AutoModeResult, AutoSession, UnitResult,
};
pub use complexity::{Complexity, ComplexityClassifier, ModelTier, RiskLevel, Unit, UnitType};
pub use debug::{DebugManager, DebugSession};
pub use engine::{Task, TaskStatus, Wave, WorkflowEngine};
pub use llm::{ChatMessage, LlmClient, LlmConfig, ModelProfile, TaskExecutionResult};
pub use model_router::{
    BudgetStatus, BudgetTracker, CostTable, ModelRouter, ModelSelection, RoutingHistory,
};
pub use progress::{ProgressReport, ProgressTracker};
pub use rustycode_protocol::{PermissionRole, ToolCall};
pub use state::{DebugState, ExecutionState, OrchestraState, ProjectState, StateManager};
pub use tools::{create_tool_executor, ToolExecutionResult, ToolExecutor};
pub use workflow::{
    PhaseResult, PlanningResult, SliceResult, Task as WorkflowTask, TaskResult,
    UnitExecutionResult as WorkflowUnitExecutionResult, WorkflowContext, WorkflowOrchestrator,
    WorkflowPhase, WorkflowResult,
};
pub use worktree::{Worktree, WorktreeLock, WorktreeManager};
// Canonical Orchestra v2 runtime
pub use activity_log::{
    clear_activity_log_state, prune_activity_logs, save_activity_log, SessionEntry,
};
pub use agent_identity::{AgentId, AgentIdentity, AgentIdentityManager};
pub use atomic_write::{atomic_write, atomic_write_async, atomic_write_bytes};
pub use auto_budget::{
    format_alert_level, format_enforcement_action, get_budget_alert_level,
    get_budget_enforcement_action, get_new_budget_alert_level, BudgetAlertLevel,
    BudgetEnforcementAction, BudgetEnforcementMode,
};
pub use auto_direct_dispatch::{dispatch_direct_phase, DirectDispatchResult};
pub use auto_idempotency::{
    add_completed_key, build_idempotency_key, check_idempotency, clear_recently_evicted,
    get_lifetime_dispatch_count, get_skip_count, is_completed_skip, is_loop_skip,
    mark_recently_evicted, remove_completed_key, IdempotencyContext, IdempotencyResult,
    IdempotencyState, SkipReason, MAX_CONSECUTIVE_SKIPS, MAX_LIFETIME_DISPATCHES,
};
pub use auto_model_selection::{
    select_model_for_unit, unit_phase_label, AvailableModel, DynamicRoutingConfig, ModelConfig,
    ModelLike, ModelSelectionResult, OrchestraPreferences, RoutingMetadata,
};
pub use auto_observability::{build_observability_repair_block, collect_observability_warnings};
pub use auto_post_unit::{
    post_unit_post_verification, post_unit_pre_verification, HookRetryTrigger, PostUnitContext,
    PostUnitResult, PostVerificationAction, QuickTask, TriageCapture,
};
pub use auto_prompts::{
    build_context_section, build_dependencies_section, build_task_instructions, build_task_prompt,
    inline_dependency_summaries, inline_file, inline_file_optional, inline_file_smart,
    inline_orchestra_root_file,
};
pub use auto_recovery::{
    build_loop_remediation_steps, completed_keys_path, diagnose_expected_artifact,
    load_persisted_keys, persist_completed_key, remove_persisted_key,
    resolve_expected_artifact_path, verify_expected_artifact, write_blocker_placeholder,
};
pub use auto_stuck_detection::{
    check_stuck_and_recover, StuckContext, StuckDetectionCurrentUnit, StuckDetectionSession,
    StuckResult, MAX_LIFETIME_DISPATCHES as STUCK_MAX_LIFETIME_DISPATCHES, MAX_UNIT_DISPATCHES,
    STUB_RECOVERY_THRESHOLD,
};
pub use auto_supervisor::{
    deregister_sigterm_handler, detect_working_tree_activity, get_git_status_summary,
    has_staged_changes, is_repo_clean, is_shutdown_requested, register_sigterm_handler,
    request_shutdown, SigtermGuard,
};
pub use auto_timers::{
    check_context_pressure, get_runtime, record_progress, register_callback,
    start_unit_supervision, stop_unit_supervision, track_tool_complete, track_tool_start,
    ProgressKind, RuntimePhase, TimerConfig, TimerEvent, TimerHandle,
};
pub use auto_tool_tracking::{
    clear_in_flight_tools as auto_clear_in_flight_tools,
    get_in_flight_tool_count as auto_get_in_flight_tool_count,
    get_oldest_in_flight_tool_age_ms as auto_get_oldest_in_flight_tool_age_ms,
    get_oldest_in_flight_tool_start as auto_get_oldest_in_flight_tool_start,
    mark_tool_end as auto_mark_tool_end, mark_tool_start as auto_mark_tool_start,
    InFlightToolTracker,
};
pub use auto_worktree_sync::{
    check_resources_stale, clean_stale_runtime_units, escape_stale_worktree, read_resource_version,
    sync_project_root_to_worktree, sync_state_to_project_root,
};
pub use cache::parse_cache as cache_parse_cache;
pub use cache::path_cache as cache_path_cache;
pub use cache::state_cache as cache_state_cache;
pub use cache::{
    cache_count, init_builtin_caches, invalidate_all_caches, invalidate_cache, is_cache_registered,
    register_cache, unregister_cache,
};
pub use captures::{
    append_capture, count_pending_captures, has_pending_captures, load_actionable_captures,
    load_all_captures, load_pending_captures, mark_capture_executed, mark_capture_resolved,
    parse_triage_output, resolve_captures_path, CaptureEntry, CaptureStatus, Classification,
    TriageResult,
};
pub use collision_diagnostics::{
    analyze_collisions, doctor_report, AliasConflictType, ClassifiedDiagnostic, CollisionClass,
    CollisionDoctorReport, CollisionDoctorSummary, DiagnosticSeverity, NamespacedComponent,
    RegistryCollision, RegistryDiagnostic, ResolutionResult,
};
pub use commands_config::{
    create_auth_storage, get_auth_entry, get_auth_path, get_config_auth_storage_path,
    get_config_status_text, get_tool_keys, is_tool_configured, load_tool_api_keys,
    remove_tool_api_key, set_tool_api_key, AuthEntry, AuthStorage, AuthStorageEntry, ToolKeyConfig,
};
pub use complexity_classifier::{
    classify_unit_complexity, tier_label, tier_ordinal, ClassificationResult, TaskMetadata,
};
pub use constants::{
    CACHE_MAX, DEFAULT_BASH_TIMEOUT_SECS, DEFAULT_COMMAND_TIMEOUT_MS, DIR_CACHE_MAX,
    STATE_REBUILD_MIN_INTERVAL_MS,
};
pub use context_budget::{
    budget_usage_percent, compute_budgets, content_fits_budget, remaining_budget,
    resolve_executor_context_window, truncate_at_section_boundary, BudgetAllocation, ModelInfo,
    TaskCountRange, TruncationResult,
};
pub use convoy::{
    Convoy, ConvoyId, ConvoyManager, ConvoyStatus, ConvoyTask, TaskCounts, TaskId as ConvoyTaskId,
    TaskStatus as ConvoyTaskStatus,
};
pub use rustycode_protocol::ConvoyPlan;
pub use debug_logger::{
    debug_count, debug_log, debug_peak, debug_time, disable_debug, enable_debug,
    get_debug_log_path, is_enabled, write_debug_summary, DebugCounter,
};
pub use detection::{
    detect_package_manager, detect_project_signals, detect_project_state, detect_v1_planning,
    has_global_setup, is_first_ever_launch, language_map, OrchestraProjectState, ProjectDetection,
    ProjectSignals, V1Detection, V2Detection, CI_MARKERS, MONOREPO_MARKERS, PROJECT_FILES,
    TEST_MARKERS,
};
pub use diff_context::{
    get_changed_files_with_context, get_recently_changed_files, rank_files_by_relevance,
    ChangeType, ChangedFileInfo, RecentFilesOptions,
};
pub use dispatch_guard::get_prior_slice_completion_blocker;
pub use doctor_format::{
    filter_doctor_issues, format_doctor_issues_for_prompt, format_doctor_report, matches_scope,
    summarize_doctor_issues,
};
pub use doctor_types::{
    DoctorIssue, DoctorIssueCode, DoctorIssueCodeCount, DoctorReport, DoctorScope, DoctorSeverity,
    DoctorSummary,
};
pub use exit_command::{execute_exit, register_exit_command, ExitDeps, StopAutoFn};
pub use export::{write_export_file, ExportReport, ModelBreakdown, SliceBreakdown};
pub use extension_discovery::{
    discover_extension_entry_paths, is_extension_file, resolve_extension_entries,
};
pub use extension_registry::{
    disable_extension, discover_all_manifests, enable_extension, ensure_registry_entries,
    get_registry_path, is_extension_enabled, load_registry, read_manifest,
    read_manifest_from_entry_path, save_registry, ExtensionDependencies, ExtensionManifest,
    ExtensionProvides, ExtensionRegistry, ExtensionRegistryEntry, ExtensionSource, ExtensionTier,
    PlatformRequires,
};
pub use files::{
    // Cache
    clear_parse_cache,
    count_must_haves_mentioned_in_summary,
    extract_all_sections,
    extract_bold_field,
    // Helpers
    extract_section,
    // UAT
    extract_uat_type,
    format_continue,
    format_overrides_section,
    format_secrets_manifest,
    // File I/O
    load_file,
    parse_bullets,
    // Context
    parse_context_depends_on,
    // Continue
    parse_continue,
    parse_frontmatter_map,
    // Overrides
    parse_overrides,
    // Slice Plan
    parse_plan,
    // Requirements
    parse_requirement_counts,
    // Roadmap
    parse_roadmap,
    // Secrets Manifest
    parse_secrets_manifest,
    // Summary
    parse_summary,
    // Task Plan Must-Haves
    parse_task_plan_must_haves,
    save_file,
    split_frontmatter,
    BoundaryMapEntry,
    Continue,
    ContinueFrontmatter,
    FileModified,
    MustHaveItem,
    Override,
    OverrideScope,
    RequirementCounts,
    RequiresEntry,
    Roadmap,
    RoadmapSlice,
    SecretsManifest,
    SecretsManifestEntry,
    SlicePlan,
    Summary,
    SummaryFrontmatter,
    TaskPlanEntry,
    UatType,
};
pub use formulas::{
    Formula, FormulaExecutionContext, FormulaManager, FormulaStep, FormulaValidation, StepResult,
};
pub use git_constants::git_no_prompt_env;
pub use git_self_heal::{
    abort_and_reset, format_git_error, AbortAndResetResult, MergeConflictError,
};
pub use gitignore::{
    ensure_gitignore, ensure_preferences, untrack_runtime_files, GitignoreOptions,
    BASELINE_PATTERNS, ORCHESTRA_RUNTIME_PATTERNS,
};
pub use headless_context::{bootstrap_orchestra_project, load_context, read_stdin, ContextOptions};
pub use headless_events::{
    get_fire_and_forget_methods_set, get_quick_commands_set, idle_timeout, is_blocked_notification,
    is_fire_and_forget_method, is_milestone_ready_notification, is_quick_command,
    is_terminal_notification, new_milestone_idle_timeout, HeadlessEvent, FIRE_AND_FORGET_METHODS,
    IDLE_TIMEOUT_MS, NEW_MILESTONE_IDLE_TIMEOUT_MS, QUICK_COMMANDS, TERMINAL_PREFIXES,
};
pub use help_text::{format_main_help, format_subcommand_help, get_subcommand_help};
pub use input_classifier::{
    classify_input_fallback, estimate_input_complexity, parse_llm_response, ComplexitySignals,
    InputComplexityEstimate, InputTier, INPUT_CLASSIFIER_PROMPT,
};
pub use json_persistence::{
    load_json_file, load_json_file_or_null, save_json_file, write_json_file_atomic,
};
pub use jsonl_utils::{parse_jsonl, MAX_JSONL_BYTES};
pub use metrics::{
    classify_unit_phase, format_cost, format_duration, format_token_count, MetricsLedger,
    MetricsManager, MetricsPhase, MetricsTotals, PhaseBreakdown, TokenCounts, UnitMetrics,
};
pub use migrate_external::{
    migrate_to_external_state, recover_failed_migration, ExternalMigrationResult,
};
pub use migrate_preview::{
    generate_preview, MigrationPreview, MilestoneData, OrchestraProject, RequirementData,
    SliceData, TaskData,
};
pub use migrate_validator::{
    validate_planning_directory, MigrationValidationIssue, MigrationValidationResult,
    MigrationValidationSeverity,
};
pub use milestone_actions::{
    discard_milestone, get_parked_reason, is_parked, park_milestone, unpark_milestone,
};
pub use milestone_ids::{
    extract_milestone_seq, find_milestone_ids, generate_milestone_suffix, is_valid_milestone_id,
    max_milestone_num, milestone_id_sort, next_milestone_id, parse_milestone_id,
    sort_milestone_ids, ParsedMilestoneId, MILESTONE_ID_RE,
};
pub use model_cost_table::{
    calculate_cost, compare_model_cost, find_cheapest_model, get_all_cost_entries,
    get_bundled_cost_table, is_model_known, lookup_model_cost, ModelCostComparison, ModelCostEntry,
};
pub use namespaced_resolver::{
    AliasResolution, AmbiguousResolution, CanonicalResolution, ComponentType, LocalFirstResolution,
    NameResolutionResult, NamespacedRegistry, NamespacedResolver, NotFoundResolution,
    ResolutionContext, ResolutionType, ResolverComponent, ShorthandResolution,
};
pub use notifications::{
    send_desktop_notification, should_send_desktop_notification, NotificationKind,
    NotificationPreferences, NotifyLevel,
};
pub use observability_validator::{
    format_validation_issues, validate_complete_boundary, validate_execute_boundary,
    validate_plan_boundary, validate_slice_plan_content, validate_slice_summary_content,
    validate_task_plan_content, validate_task_summary_content, ValidationIssue, ValidationScope,
    ValidationSeverity,
};
pub use orchestra_config::OrchestraProjectConfig;
pub use orchestra_executor::Orchestra2Executor;
pub use orchestra_service::{OrchestraService, ProviderBundle};
pub use orchestra_tools::{
    detect_verification_commands, format_verification_result, run_verification,
    OrchestraToolResult, ToolConfig,
};
pub use paths::{
    build_dir_name, build_milestone_file_name, build_slice_file_name, build_task_file_name,
    clear_path_cache, milestones_dir, orchestra_root, rel_milestone_file, rel_milestone_path,
    rel_orchestra_root_file, rel_slice_file, rel_slice_path, rel_task_file, resolve_dir,
    resolve_file, resolve_milestone_file, resolve_milestone_path, resolve_orchestra_root_file,
    resolve_slice_file, resolve_slice_path, resolve_task_file, resolve_task_files,
    resolve_tasks_dir, OrchestraRootFile,
};
pub use phases::Phase;
pub use pi_migration::{
    get_pi_default_model_and_provider, is_llm_provider, migrate_pi_credentials, pi_auth_path,
    pi_settings_path, AuthCredential, MigrationResult, LLM_PROVIDER_IDS,
};
pub use project_bootstrap::{
    bootstrap_default_project, bootstrap_project, bootstrap_quick_task_project, BootstrapInfo,
};
pub use prompt_cache_optimizer::{
    classify_section, compute_cache_hit_rate, estimate_cache_savings, optimize_for_caching,
    section, CacheOptimizedPrompt, CacheUsage, ContentRole, PromptSection, Provider, SectionCounts,
};
pub use prompt_compressor::{
    compress_prompt, compress_to_target, CompressionLevel, CompressionOptions, CompressionResult,
};
pub use prompt_loader::{
    clear_cache, get_base_dir, inline_template, load_prompt, load_template, set_base_dir,
};
pub use prompt_ordering::{
    analyze_cache_efficiency, reorder_for_caching, CacheEfficiencyAnalysis, ExtractedSection,
    SectionRole,
};
pub use provider_error_pause::{
    classify_provider_error, pause_auto_for_provider_error, pause_auto_for_provider_error_async,
    NotificationLevel, PauseOptions, ProviderErrorClassification, ProviderErrorPauseUI,
};
pub use queue_order::{
    load_queue_order, prune_queue_order, save_queue_order, sort_by_queue_order,
    validate_queue_order, DependencyRedundancy, DependencyValidation, DependencyViolation,
    DependencyViolationType,
};
pub use quick::{
    ensure_quick_dir, get_next_task_num, get_quick_task_branch_name, get_quick_task_rel_path,
    slugify,
};
pub use remote_questions_format::{
    format_for_discord, format_for_slack, format_for_telegram, parse_discord_reaction_response,
    parse_discord_response, parse_slack_reaction_response, parse_slack_reply,
    parse_telegram_response, DiscordEmbed, DiscordField, DiscordFooter, DiscordFormattedResponse,
    DiscordReaction, RemoteAnswer, RemoteAnswerValue, RemoteContext, RemoteOption, RemotePrompt,
    RemoteQuestion, SlackBlock, SlackElement, SlackText, TelegramInlineButton,
    TelegramInlineKeyboardMarkup, TelegramMessage, DISCORD_NUMBER_EMOJIS,
    SLACK_NUMBER_REACTION_NAMES,
};
pub use repo_identity::{
    ensure_orchestra_symlink, external_orchestra_root, get_remote_url, get_repo_identity_info,
    is_inside_worktree, repo_identity, resolve_git_root, RepoIdentityInfo, DEFAULT_ORCHESTRA_DIR,
    ORCHESTRA_STATE_DIR_ENV, PROJECTS_SUBDIR,
};
pub use roadmap_slices::{expand_dependencies, parse_roadmap_slices, RoadmapSliceEntry};
pub use routing_history::{
    clear_routing_history, get_adaptive_tier_adjustment, get_routing_history, init_routing_history,
    record_feedback, record_outcome, reset_routing_history, ComplexityTier, FeedbackEntry,
    FeedbackRating, PatternHistory, RoutingHistoryData, TierOutcome, FAILURE_THRESHOLD,
    FEEDBACK_WEIGHT, ROLLING_WINDOW,
};
pub use safe_fs::{safe_copy, safe_copy_recursive, safe_mkdir};
pub use semantic_chunker::{
    chunk_by_relevance, format_chunks, score_chunks, split_into_chunks, Chunk, ChunkOptions,
    ChunkResult, ContentType, RelevanceOptions,
};
pub use semver::{compare_semver, is_newer, is_newer_or_equal};
pub use session_forensics::{
    extract_trace, extract_trace_from_session, format_trace_summary, get_git_changes,
    synthesize_crash_recovery, CommandRun, ExecutionTrace, ForensicToolCall, RecoveryBriefing,
};
pub use session_status_io::{
    cleanup_stale_sessions, consume_signal, is_session_stale, read_all_session_statuses,
    read_session_status, remove_session_status, send_signal, write_session_status, CurrentUnit,
    SessionSignal, SessionState, SessionStatus, SignalMessage,
};
pub use skill_discovery::{
    clear_skill_snapshot, detect_new_skills, format_skills_markdown, format_skills_xml,
    has_skill_snapshot, snapshot_skills, DiscoveredSkill,
};
pub use skill_telemetry::{
    capture_available_skills, detect_stale_skills, get_agent_dir, get_and_clear_skills,
    get_skill_last_used, record_skill_read, reset_skill_telemetry, set_agent_dir, SkillUsage,
};
pub use state_derivation::{
    MilestoneRef, OrchestraState as DerivedOrchestraState, SliceRef, StateDeriver, TaskRef,
};
pub use structured_data_formatter::{
    format_decision_compact, format_decisions_compact, format_requirement_compact,
    format_requirements_compact, format_task_plan_compact, measure_savings, DecisionInput,
    RequirementInput, TaskPlanInput,
};
pub use summary_distiller::{distill_single, distill_summaries, DistillationResult};
pub use token_counter::{
    count_tokens, count_tokens_sync, estimate_tokens_for_provider, get_chars_per_token,
    init_token_counter, is_accurate_counting_available, parse_token_provider, TokenProvider,
};
pub use tool_bootstrap::{
    ensure_managed_tools, get_candidate_names, get_tool_spec, is_regular_file, provision_tool,
    resolve_tool_from_path, split_path, ManagedTool, ToolSpec,
};
pub use tool_tracking::{
    clear_in_flight_tools, get_in_flight_tool_count, get_in_flight_tools,
    get_oldest_in_flight_tool_age_ms, get_oldest_in_flight_tool_start, mark_tool_end,
    mark_tool_start, ToolCallId, ToolState,
};
pub use unit_closeout::{closeout_unit, ActivityFilePath, CloseoutOptions};
pub use unit_runtime::{
    clear_unit_runtime_record, format_execute_task_recovery_status, list_unit_runtime_records,
    read_unit_runtime_record, write_unit_runtime_record, ExecuteTaskRecoveryStatus, RecoveryReason,
    UnitRuntimePhase, UnitRuntimeRecord,
};
pub use validate_directory::{
    assert_safe_directory, validate_directory, DirectoryValidationResult,
    DirectoryValidationSeverity,
};
pub use verification_evidence::{
    format_evidence_table,
    write_verification_json,
    AuditSeverity,
    AuditWarning,
    DiscoverySource as EvidenceDiscoverySource,
    RuntimeError,
    RuntimeErrorSeverity,
    RuntimeErrorSource,
    VerificationCheck as EvidenceVerificationCheck,
    // Re-export verification_evidence types (distinct from verification_gate types)
    // Note: Use verification_evidence::VerificationResult for full evidence with timestamp/errors
    VerificationResult as EvidenceVerificationResult,
};
pub use verification_gate::{
    discover_commands, format_failure_context, is_likely_command, run_verification_gate,
    sanitize_command, DiscoverCommandsOptions, DiscoverySource, VerificationCheck,
    VerificationResult,
};
pub mod error;
pub use error::{OrchestraV2Error, Result};

/// Orchestra v2 version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Orchestra v2 directory name
pub const ORCHESTRA_DIR: &str = ".orchestra";

/// Default configuration
impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_profile: ModelProfile::Balanced,
            planning_temperature: 0.1,
            execution_temperature: 0.7,
            verification_temperature: 0.3,
            research_temperature: 0.5,
            max_tokens: 8192,
            streaming: true,
        }
    }
}
