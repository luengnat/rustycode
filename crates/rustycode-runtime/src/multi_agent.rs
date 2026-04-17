//! Multi-Agent Orchestration System for RustyCode
//!
//! This module provides enhanced multi-agent orchestration with communication:
//! - Parallel agent spawning with specialized roles
//! - Agent-to-agent communication protocol
//! - Shared working memory and consensus building
//! - Response aggregation and coordination
//! - Integration with LLM provider infrastructure

use anyhow::{Context, Result};
use rustycode_llm::provider_v2::{ChatMessage, CompletionRequest, LLMProvider};
use rustycode_llm::{caching::CachingStrategy, get_metadata};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

// Re-export shared memory types
pub use crate::shared_memory::{
    AccessLevel, MemoryConflict, MemoryData, MemoryEntry, MemoryStats, MemoryType,
    SharedWorkingMemory,
};

// Re-export hierarchical coordination types
pub use crate::hierarchical::{
    CommunicationChannel, CrossEnsembleRelation, DecisionStrategy, Ensemble, EnsembleCoordinator,
    EnsembleMember, EnsembleSpecialization, EnsembleStatus, HierarchyStructure, Permission,
    RelationType,
};

// Re-export enhanced orchestrator types
pub use crate::enhanced_orchestrator::{
    AgentAnalysisResult, EnhancedOrchestrator, OrchestratedAnalysis, OrchestratorConfig,
    SessionStats,
};

/// Agent role with specialized perspective
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AgentRole {
    /// Reviews code for factual accuracy and correctness
    FactualReviewer,
    /// Provides senior engineering perspective on architecture and design
    SeniorEngineer,
    /// Analyzes security vulnerabilities and best practices
    SecurityExpert,
    /// Checks for consistency across codebase and documentation
    ConsistencyReviewer,
    /// Verifies redundancy and checks for duplicate logic
    RedundancyChecker,
    /// Analyzes performance characteristics and optimization opportunities
    PerformanceAnalyst,
    /// Reviews test coverage and quality
    TestCoverageAnalyst,
    /// Checks documentation quality and completeness
    DocumentationReviewer,
}

impl AgentRole {
    /// Build layered system prompt combining provider metadata + provider-specific agent role + task context
    fn build_layered_prompt(&self, provider_id: &str, task_context: &str) -> String {
        // Layer 1: Provider base prompt (capabilities, tools, security, format)
        let provider_base = if let Some(metadata) = get_metadata(provider_id) {
            metadata.generate_system_prompt("") // Empty context, we'll add agent role next
        } else {
            // Fallback if provider metadata not found
            format!(
                "You are an AI assistant using the {} provider.",
                provider_id
            )
        };

        // Layer 2: Provider-specific agent role and specialized perspective
        let agent_role = self.system_prompt_for_provider(provider_id);

        // Layer 3: Task-specific context
        let task_section = if task_context.is_empty() {
            String::new()
        } else {
            format!("\n\n=== CURRENT TASK ===\n{}", task_context)
        };

        // Combine all layers
        format!(
            "{}\n\n=== SPECIALIZED AGENT ROLE ===\n{}{}",
            provider_base, agent_role, task_section
        )
    }

    /// Get provider-specific system prompt for this role
    fn system_prompt_for_provider(&self, provider_id: &str) -> String {
        let base_prompt = self.system_prompt();

        // Add provider-specific instructions
        let provider_instructions = match provider_id.to_lowercase().as_str() {
            "anthropic" | "claude" => {
                "\n\n=== CLAUDE-SPECIFIC INSTRUCTIONS ===\n\
                - Use XML-style tags (<thinking>, <analysis>, <finding>) to structure your analysis\n\
                - Leverage Claude's strong reasoning capabilities for deep analysis\n\
                - Think through edge cases and potential issues systematically\n\
                - Provide clear, structured output with labeled sections"
            }
            "openai" | "gpt" => {
                "\n\n=== GPT-SPECIFIC INSTRUCTIONS ===\n\
                - Be direct and concise in your analysis\n\
                - Use bullet points for clarity and readability\n\
                - Focus on practical, actionable insights\n\
                - Prioritize efficiency in your responses"
            }
            "gemini" | "google" => {
                "\n\n=== GEMINI-SPECIFIC INSTRUCTIONS ===\n\
                - Consider multiple perspectives in your analysis\n\
                - Be thorough but concise in explanations\n\
                - Leverage large context capabilities for comprehensive analysis\n\
                - Provide well-reasoned, balanced insights"
            }
            _ => {
                "\n\n=== GENERAL INSTRUCTIONS ===\n\
                - Adapt your analysis style to the provider's capabilities\n\
                - Provide clear, actionable feedback"
            }
        };

        format!("{}{}", base_prompt, provider_instructions)
    }
    /// Get all available agent roles
    pub fn all() -> Vec<AgentRole> {
        vec![
            AgentRole::FactualReviewer,
            AgentRole::SeniorEngineer,
            AgentRole::SecurityExpert,
            AgentRole::ConsistencyReviewer,
            AgentRole::RedundancyChecker,
            AgentRole::PerformanceAnalyst,
            AgentRole::TestCoverageAnalyst,
            AgentRole::DocumentationReviewer,
        ]
    }

    /// Get system prompt for this role
    pub fn system_prompt(&self) -> String {
        match self {
            AgentRole::FactualReviewer => {
                "You are a Factual Reviewer. Your role is to analyze code for factual accuracy, \
                correctness, and logic errors. Focus on:\n\
                - Identifying bugs and logic errors\n\
                - Verifying algorithm correctness\n\
                - Checking data flow and state management\n\
                - Validating edge cases and error handling\n\
                Provide specific, actionable feedback with line numbers when applicable."
                    .to_string()
            }
            AgentRole::SeniorEngineer => {
                "You are a Senior Engineer. Your role is to provide architectural and design guidance. \
                Focus on:\n\
                - Code architecture and design patterns\n\
                - Maintainability and extensibility\n\
                - Separation of concerns and modularity\n\
                - API design and interface clarity\n\
                - Best practices and idiomatic code\n\
                Provide constructive feedback on how to improve the overall code quality."
                    .to_string()
            }
            AgentRole::SecurityExpert => {
                "You are a Security Expert. Your role is to identify security vulnerabilities and risks. \
                Focus on:\n\
                - Input validation and sanitization\n\
                - Authentication and authorization issues\n\
                - Data exposure and sensitive information handling\n\
                - Injection vulnerabilities (SQL, XSS, command injection)\n\
                - Cryptographic errors and insecure randomness\n\
                - Dependency vulnerabilities\n\
                Flag any security concerns with severity ratings (Critical/High/Medium/Low)."
                    .to_string()
            }
            AgentRole::ConsistencyReviewer => {
                "You are a Consistency Reviewer. Your role is to check for consistency across the codebase. \
                Focus on:\n\
                - Naming conventions and terminology\n\
                - Code style and formatting consistency\n\
                - Error handling patterns\n\
                - API consistency and symmetry\n\
                - Documentation alignment with implementation\n\
                Identify any inconsistencies that could confuse users or maintainers."
                    .to_string()
            }
            AgentRole::RedundancyChecker => {
                "You are a Redundancy Checker. Your role is to identify redundant code and opportunities for reuse. \
                Focus on:\n\
                - Duplicate code blocks or logic\n\
                - Similar functions that could be unified\n\
                - Opportunities for abstraction\n\
                - Unnecessary complexity\n\
                - Dead code or unused variables\n\
                Suggest consolidations to reduce code duplication."
                    .to_string()
            }
            AgentRole::PerformanceAnalyst => {
                "You are a Performance Analyst. Your role is to analyze performance characteristics. \
                Focus on:\n\
                - Algorithm complexity and efficiency\n\
                - Memory usage and allocation patterns\n\
                - I/O operations and database queries\n\
                - Caching opportunities\n\
                - Concurrency and parallelism potential\n\
                - Hot paths and bottlenecks\n\
                Provide specific optimization suggestions when applicable."
                    .to_string()
            }
            AgentRole::TestCoverageAnalyst => {
                "You are a Test Coverage Analyst. Your role is to evaluate testing quality. \
                Focus on:\n\
                - Test coverage gaps and missing test cases\n\
                - Edge cases and error scenarios not tested\n\
                - Test quality and meaningful assertions\n\
                - Integration vs unit test balance\n\
                - Mock and fixture quality\n\
                Suggest specific tests that should be added."
                    .to_string()
            }
            AgentRole::DocumentationReviewer => {
                "You are a Documentation Reviewer. Your role is to assess documentation quality. \
                Focus on:\n\
                - Code comments and docstring completeness\n\
                - Function and parameter documentation\n\
                - README and project-level docs\n\
                - Example code and usage documentation\n\
                - API documentation clarity\n\
                Identify missing or unclear documentation."
                    .to_string()
            }
        }
    }

    /// Get role name for display
    pub fn name(&self) -> &str {
        match self {
            AgentRole::FactualReviewer => "Factual Reviewer",
            AgentRole::SeniorEngineer => "Senior Engineer",
            AgentRole::SecurityExpert => "Security Expert",
            AgentRole::ConsistencyReviewer => "Consistency Reviewer",
            AgentRole::RedundancyChecker => "Redundancy Checker",
            AgentRole::PerformanceAnalyst => "Performance Analyst",
            AgentRole::TestCoverageAnalyst => "Test Coverage Analyst",
            AgentRole::DocumentationReviewer => "Documentation Reviewer",
        }
    }

    /// Parse a role from a short name string (case-insensitive)
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "factual" | "factual_reviewer" => Some(AgentRole::FactualReviewer),
            "senior" | "senior_engineer" => Some(AgentRole::SeniorEngineer),
            "security" | "security_expert" => Some(AgentRole::SecurityExpert),
            "consistency" | "consistency_reviewer" => Some(AgentRole::ConsistencyReviewer),
            "redundancy" | "redundancy_checker" => Some(AgentRole::RedundancyChecker),
            "performance" | "performance_analyst" => Some(AgentRole::PerformanceAnalyst),
            "test" | "test_coverage" => Some(AgentRole::TestCoverageAnalyst),
            "docs" | "documentation" | "documentation_reviewer" => {
                Some(AgentRole::DocumentationReviewer)
            }
            _ => None,
        }
    }
}

/// Messages that agents can send to each other
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AgentMessage {
    /// Request for information or clarification from another agent
    Request {
        /// Agent making the request
        from: AgentRole,
        /// Agent being asked
        to: AgentRole,
        /// The question or request
        query: String,
        /// Context for the request (why this is being asked)
        context: String,
        /// Unique message ID for tracking
        message_id: String,
    },

    /// Response to a request from another agent
    Response {
        /// Agent providing the response
        from: AgentRole,
        /// Agent being responded to
        to: AgentRole,
        /// The answer or information
        answer: String,
        /// How confident the agent is in this answer (0.0 to 1.0)
        confidence: f64,
        /// Reference to the original request message_id
        request_id: String,
    },

    /// Broadcast announcement to all agents
    Broadcast {
        /// Agent making the announcement
        from: AgentRole,
        /// The announcement content
        announcement: String,
        /// Priority level of this announcement
        priority: BroadcastPriority,
        /// Unique message ID for tracking
        message_id: String,
    },
}

/// Priority level for broadcast messages
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd)]
#[non_exhaustive]
pub enum BroadcastPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Communication hub for routing messages between agents
#[derive(Debug, Clone)]
pub struct AgentCommunicationHub {
    /// Pending requests that haven't been answered yet
    pending_requests: Vec<AgentMessage>,
    /// Completed request/response pairs
    completed_conversations: Vec<(AgentMessage, AgentMessage)>,
    /// Broadcast announcements
    broadcasts: Vec<AgentMessage>,
    /// Message queue for processing
    message_queue: Vec<AgentMessage>,
    /// Timeout for responses
    #[allow(dead_code)] // Kept for future use
    response_timeout: std::time::Duration,
}

impl AgentCommunicationHub {
    /// Create a new communication hub
    pub fn new() -> Self {
        Self {
            pending_requests: Vec::new(),
            completed_conversations: Vec::new(),
            broadcasts: Vec::new(),
            message_queue: Vec::new(),
            response_timeout: std::time::Duration::from_secs(30),
        }
    }

    /// Send a request from one agent to another
    pub fn send_request(
        &mut self,
        from: AgentRole,
        to: AgentRole,
        query: String,
        context: String,
    ) -> String {
        let message_id = uuid::Uuid::new_v4().to_string();
        let message = AgentMessage::Request {
            from,
            to,
            query,
            context,
            message_id: message_id.clone(),
        };

        self.pending_requests.push(message.clone());
        self.message_queue.push(message);
        message_id
    }

    /// Send a response to a previous request
    pub fn send_response(
        &mut self,
        from: AgentRole,
        to: AgentRole,
        answer: String,
        confidence: f64,
        request_id: String,
    ) {
        let message = AgentMessage::Response {
            from,
            to,
            answer,
            confidence,
            request_id,
        };

        self.message_queue.push(message);
    }

    /// Broadcast a message to all agents
    pub fn broadcast(
        &mut self,
        from: AgentRole,
        announcement: String,
        priority: BroadcastPriority,
    ) -> String {
        let message_id = uuid::Uuid::new_v4().to_string();
        let message = AgentMessage::Broadcast {
            from,
            announcement,
            priority,
            message_id: message_id.clone(),
        };

        self.broadcasts.push(message.clone());
        self.message_queue.push(message);
        message_id
    }

    /// Get messages pending for a specific agent
    pub fn get_pending_for_agent(&self, agent: &AgentRole) -> Vec<&AgentMessage> {
        self.pending_requests
            .iter()
            .filter(|msg| match msg {
                AgentMessage::Request { to, .. } => to == agent,
                AgentMessage::Broadcast { .. } => true,
                _ => false,
            })
            .collect()
    }

    /// Get conversation history between two agents
    pub fn get_conversation_history(
        &self,
        agent1: &AgentRole,
        agent2: &AgentRole,
    ) -> Vec<&AgentMessage> {
        self.completed_conversations
            .iter()
            .filter(|(req, _resp)| {
                matches!(req, AgentMessage::Request { from, to, .. } if from == agent1 && to == agent2)
                    || matches!(req, AgentMessage::Request { from, to, .. } if from == agent2 && to == agent1)
            })
            .flat_map(|(req, resp)| vec![req as &AgentMessage, resp as &AgentMessage])
            .collect()
    }

    /// Process messages and return responses for a specific agent
    pub fn process_messages_for(&mut self, agent: &AgentRole) -> Vec<&AgentMessage> {
        let messages_for_agent: Vec<_> = self
            .message_queue
            .iter()
            .filter(|msg| match msg {
                AgentMessage::Request { to, .. } => to == agent,
                AgentMessage::Response { to, .. } => to == agent,
                AgentMessage::Broadcast { .. } => true,
            })
            .collect();

        messages_for_agent
    }

    /// Check if a request has been answered
    pub fn is_request_answered(&self, request_id: &str) -> bool {
        self.completed_conversations.iter().any(|(req, resp)| {
            matches!(req, AgentMessage::Request { message_id, .. } if message_id == request_id)
                && matches!(resp, AgentMessage::Response { .. })
        })
    }

    /// Get the number of pending requests
    pub fn pending_count(&self) -> usize {
        self.pending_requests.len()
    }

    /// Get all broadcasts sorted by priority
    pub fn get_broadcasts_by_priority(&self) -> Vec<AgentMessage> {
        let mut broadcasts: Vec<AgentMessage> = self.broadcasts.clone();
        broadcasts.sort_by(|a, b| {
            let priority_a = match a {
                AgentMessage::Broadcast { priority, .. } => priority,
                _ => &BroadcastPriority::Low,
            };
            let priority_b = match b {
                AgentMessage::Broadcast { priority, .. } => priority,
                _ => &BroadcastPriority::Low,
            };
            priority_b
                .partial_cmp(priority_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        broadcasts
    }
}

impl Default for AgentCommunicationHub {
    fn default() -> Self {
        Self::new()
    }
}

/// Individual agent response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// The role that generated this response
    pub role: AgentRole,
    /// The analysis content
    pub analysis: String,
    /// Issues found (if any)
    pub issues: Vec<String>,
    /// Suggestions for improvement
    pub suggestions: Vec<String>,
    /// Severity of issues found (None if no issues)
    pub severity: Option<IssueSeverity>,
}

/// Severity level for issues
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Aggregated analysis from multiple agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAgentAnalysis {
    /// All individual agent responses
    pub agent_responses: Vec<AgentResponse>,
    /// Consensus issues (mentioned by multiple agents)
    pub consensus_issues: Vec<String>,
    /// Critical findings requiring immediate attention
    pub critical_findings: Vec<String>,
    /// Overall summary
    pub summary: String,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
}

/// Configuration for multi-agent analysis
#[derive(Debug, Clone)]
pub struct MultiAgentConfig {
    /// Which agent roles to use (default: all)
    pub roles: Vec<AgentRole>,
    /// Maximum number of agents to run in parallel (default: 5)
    pub max_parallelism: usize,
    /// Context to include in the analysis prompt
    pub context: String,
    /// Code or content to analyze
    pub content: String,
    /// File path (if applicable)
    pub file_path: Option<String>,
    /// Additional instructions for agents
    pub instructions: Option<String>,
}

impl Default for MultiAgentConfig {
    fn default() -> Self {
        Self {
            roles: AgentRole::all(),
            max_parallelism: 5,
            context: String::new(),
            content: String::new(),
            file_path: None,
            instructions: None,
        }
    }
}

/// Multi-agent orchestrator
pub struct MultiAgentOrchestrator {
    /// LLM provider for running agents
    provider: Arc<dyn LLMProvider>,
    /// Configuration
    config: MultiAgentConfig,
}

impl MultiAgentOrchestrator {
    /// Create a new multi-agent orchestrator
    pub fn new(provider: Box<dyn LLMProvider>, config: MultiAgentConfig) -> Self {
        Self {
            provider: Arc::from(provider),
            config,
        }
    }

    /// Create from default provider config
    pub fn from_config(config: MultiAgentConfig) -> Result<Self> {
        let provider = super::agent::create_provider_from_config()
            .context("Failed to create LLM provider for multi-agent analysis")?;
        Ok(Self::new(provider, config))
    }

    /// Run multi-agent analysis
    pub async fn analyze(&self) -> Result<MultiAgentAnalysis> {
        let semaphore = Arc::new(Semaphore::new(self.config.max_parallelism));
        let mut tasks = Vec::new();

        // Spawn tasks for each agent role
        for role in &self.config.roles {
            let permit = semaphore.clone();
            let provider = self.provider.clone();
            let role = *role;
            let prompt = self.build_prompt(&role);

            let task = tokio::spawn(async move {
                // Acquire permit to limit parallelism
                let _permit = permit.acquire().await.map_err(|e| anyhow::anyhow!("semaphore closed: {}", e))?;

                Self::run_single_agent(&*provider, role, prompt).await
            });

            tasks.push(task);
        }

        // Collect all responses
        let mut agent_responses = Vec::new();
        for task in tasks {
            let response = task.await??;
            agent_responses.push(response);
        }

        // Aggregate responses
        Ok(self.aggregate_responses(agent_responses))
    }

    /// Build the analysis prompt for a specific agent role
    fn build_prompt(&self, _role: &AgentRole) -> String {
        let mut prompt = String::new();

        if let Some(ref file_path) = self.config.file_path {
            prompt.push_str(&format!("File: {}\n\n", file_path));
        }

        if !self.config.context.is_empty() {
            prompt.push_str(&format!("Context:\n{}\n\n", self.config.context));
        }

        if let Some(ref instructions) = self.config.instructions {
            prompt.push_str(&format!("Additional Instructions:\n{}\n\n", instructions));
        }

        prompt.push_str(&format!(
            "Code/Content to Analyze:\n{}\n\n",
            self.config.content
        ));

        prompt.push_str(
            "Please provide your analysis. Include:\n\
            1. Specific issues you found (if any)\n\
            2. Suggestions for improvement\n\
            3. Any other relevant feedback\n\n\
            Format your response clearly with bullet points or numbered lists.",
        );

        prompt
    }

    /// Run a single agent analysis
    async fn run_single_agent(
        provider: &dyn LLMProvider,
        role: AgentRole,
        prompt: String,
    ) -> Result<AgentResponse> {
        // Build layered system prompt: provider capabilities + agent role + task context
        let provider_id = provider.name();
        let system_prompt = role.build_layered_prompt(provider_id, &prompt);

        let messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(prompt), // Task-specific user message
        ];

        // Apply prompt caching for cost optimization
        // Cache system prompts (agent role definitions) since they're reused across requests
        let cached_messages = CachingStrategy::SystemPrompts.apply_to_messages(messages);

        let request = CompletionRequest::new(
            "claude-sonnet-4-6".to_string(), // Default model
            cached_messages,
        )
        .with_max_tokens(4096)
        .with_temperature(0.7);

        let completion = provider
            .complete(request)
            .await
            .context("Agent request failed")?;

        let response = completion.content;

        // Parse the response to extract issues and suggestions
        let (issues, suggestions, severity) = Self::parse_agent_response(&role, &response);

        Ok(AgentResponse {
            role,
            analysis: response,
            issues,
            suggestions,
            severity,
        })
    }

    /// Parse agent response to extract structured information
    fn parse_agent_response(
        role: &AgentRole,
        response: &str,
    ) -> (Vec<String>, Vec<String>, Option<IssueSeverity>) {
        let mut issues = Vec::new();
        let mut suggestions = Vec::new();
        let mut severity = None;

        let lines: Vec<&str> = response.lines().collect();
        let mut current_section = None;

        for line in lines {
            let line_lower = line.to_lowercase();

            // Detect section headers
            if line_lower.contains("issue")
                || line_lower.contains("problem")
                || line_lower.contains("bug")
            {
                current_section = Some("issues");
            } else if line_lower.contains("suggest")
                || line_lower.contains("recommend")
                || line_lower.contains("improve")
            {
                current_section = Some("suggestions");
            } else if line_lower.contains("critical") {
                severity = Some(IssueSeverity::Critical);
            } else if line_lower.contains("high") && severity.is_none() {
                severity = Some(IssueSeverity::High);
            } else if line_lower.contains("medium") && severity.is_none() {
                severity = Some(IssueSeverity::Medium);
            }

            // Extract bullet points
            if line.trim().starts_with('-')
                || line.trim().starts_with('*')
                || line.trim().starts_with("•")
            {
                let content = line
                    .trim()
                    .trim_start_matches('-')
                    .trim_start_matches('*')
                    .trim_start_matches('•')
                    .trim();
                if !content.is_empty() {
                    match current_section {
                        Some("issues") => issues.push(content.to_string()),
                        Some("suggestions") => suggestions.push(content.to_string()),
                        _ => {
                            // Default role-based classification
                            if matches!(
                                role,
                                AgentRole::SecurityExpert | AgentRole::FactualReviewer
                            ) {
                                issues.push(content.to_string());
                            } else {
                                suggestions.push(content.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Default severity based on role if none detected
        if severity.is_none() {
            severity = match role {
                AgentRole::SecurityExpert => Some(IssueSeverity::High),
                AgentRole::FactualReviewer => Some(IssueSeverity::Medium),
                _ => None,
            };
        }

        (issues, suggestions, severity)
    }

    /// Aggregate responses from multiple agents
    fn aggregate_responses(&self, responses: Vec<AgentResponse>) -> MultiAgentAnalysis {
        // Find consensus issues (mentioned by multiple agents)
        let mut issue_counts: HashMap<String, usize> = HashMap::new();
        for response in &responses {
            for issue in &response.issues {
                *issue_counts.entry(issue.clone()).or_insert(0) += 1;
            }
        }

        let consensus_issues: Vec<String> = issue_counts
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .map(|(issue, count)| format!("{} (mentioned by {} agents)", issue, count))
            .collect();

        // Find critical findings
        let critical_findings: Vec<String> = responses
            .iter()
            .filter(|r| r.severity == Some(IssueSeverity::Critical))
            .flat_map(|r| r.issues.clone())
            .collect();

        // Calculate confidence based on agreement
        let total_agents = responses.len() as f64;
        let agreement_score = if total_agents > 0.0 {
            consensus_issues.len() as f64 / total_agents
        } else {
            0.0
        };
        let confidence = (agreement_score + 0.5).min(1.0); // Base confidence of 0.5, increased by consensus

        // Generate summary
        let summary = self.generate_summary(&responses, &consensus_issues, &critical_findings);

        MultiAgentAnalysis {
            agent_responses: responses,
            consensus_issues,
            critical_findings,
            summary,
            confidence,
        }
    }

    /// Generate a summary of the multi-agent analysis
    fn generate_summary(
        &self,
        responses: &[AgentResponse],
        consensus_issues: &[String],
        critical_findings: &[String],
    ) -> String {
        let mut summary = String::new();

        summary.push_str("Multi-Agent Analysis Summary\n");
        summary.push_str("=============================\n\n");
        summary.push_str(&format!("Agents Consulted: {}\n", responses.len()));
        summary.push_str(&format!("Consensus Issues: {}\n", consensus_issues.len()));
        summary.push_str(&format!("Critical Findings: {}\n", critical_findings.len()));
        summary.push('\n');

        if !critical_findings.is_empty() {
            summary.push_str("🚨 CRITICAL FINDINGS:\n");
            for (i, finding) in critical_findings.iter().enumerate() {
                summary.push_str(&format!("  {}. {}\n", i + 1, finding));
            }
            summary.push('\n');
        }

        if !consensus_issues.is_empty() {
            summary.push_str("📊 CONSENSUS ISSUES:\n");
            for (i, issue) in consensus_issues.iter().enumerate() {
                summary.push_str(&format!("  {}. {}\n", i + 1, issue));
            }
            summary.push('\n');
        }

        summary.push_str("📋 AGENT PERSPECTIVES:\n");
        for response in responses {
            summary.push_str(&format!("\n{}:\n", response.role.name()));
            // Show first 200 chars of analysis
            let preview = if response.analysis.chars().count() > 200 {
                format!(
                    "{}...",
                    response.analysis.chars().take(197).collect::<String>()
                )
            } else {
                response.analysis.clone()
            };
            summary.push_str(&format!("  {}\n", preview.replace('\n', " ")));
        }

        summary
    }

    /// Format the analysis for display
    pub fn format_analysis(analysis: &MultiAgentAnalysis) -> String {
        let mut output = String::new();

        output.push_str(&format!("{}\n\n", analysis.summary));
        output.push_str(&format!(
            "Confidence Score: {:.1}%\n\n",
            analysis.confidence * 100.0
        ));

        output.push_str("DETAILED ANALYSIS:\n");
        output.push_str("==================\n\n");

        for response in &analysis.agent_responses {
            output.push_str(&format!("## {}\n\n", response.role.name()));
            output.push_str(&format!("{}\n\n", response.analysis));

            if !response.issues.is_empty() {
                output.push_str("Issues:\n");
                for issue in &response.issues {
                    output.push_str(&format!("  • {}\n", issue));
                }
                output.push('\n');
            }

            if !response.suggestions.is_empty() {
                output.push_str("Suggestions:\n");
                for suggestion in &response.suggestions {
                    output.push_str(&format!("  • {}\n", suggestion));
                }
                output.push('\n');
            }

            if let Some(sev) = response.severity {
                output.push_str(&format!("Severity: {:?}\n\n", sev));
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_role_names() {
        assert_eq!(AgentRole::FactualReviewer.name(), "Factual Reviewer");
        assert_eq!(AgentRole::SecurityExpert.name(), "Security Expert");
    }

    #[test]
    fn test_agent_role_system_prompts() {
        let prompt = AgentRole::FactualReviewer.system_prompt();
        assert!(prompt.contains("Factual Reviewer"));
        assert!(prompt.contains("factual accuracy"));
    }

    #[test]
    fn test_multi_agent_config_default() {
        let config = MultiAgentConfig::default();
        assert_eq!(config.roles, AgentRole::all());
        assert_eq!(config.max_parallelism, 5);
    }

    #[test]
    fn test_issue_severity_ordering() {
        assert!(IssueSeverity::Critical > IssueSeverity::High);
        assert!(IssueSeverity::High > IssueSeverity::Medium);
        assert!(IssueSeverity::Medium > IssueSeverity::Low);
    }

    // ========== LAYERED PROMPT ARCHITECTURE TESTS ==========

    #[test]
    fn test_build_layered_prompt_with_anthropic() {
        let role = AgentRole::SecurityExpert;
        let task_context = "Analyze this function for SQL injection vulnerabilities";

        let layered_prompt = role.build_layered_prompt("anthropic", task_context);

        // Verify all three layers are present
        assert!(
            layered_prompt.contains("SPECIALIZED AGENT ROLE"),
            "Layer 2: Agent role section missing"
        );
        assert!(
            layered_prompt.contains("CURRENT TASK"),
            "Layer 3: Task context section missing"
        );

        // Verify provider-specific instructions for Anthropic
        assert!(
            layered_prompt.contains("CLAUDE-SPECIFIC INSTRUCTIONS"),
            "Anthropic-specific instructions missing"
        );
        assert!(
            layered_prompt.contains("<thinking>"),
            "Anthropic XML-style tag guidance missing"
        );
        assert!(
            layered_prompt.contains("Security Expert"),
            "Base role prompt missing"
        );

        // Verify task context is included
        assert!(
            layered_prompt.contains("SQL injection"),
            "Task context not included in layered prompt"
        );
    }

    #[test]
    fn test_build_layered_prompt_with_openai() {
        let role = AgentRole::PerformanceAnalyst;
        let task_context = "Optimize this loop for better performance";

        let layered_prompt = role.build_layered_prompt("openai", task_context);

        // Verify provider-specific instructions for OpenAI
        assert!(
            layered_prompt.contains("GPT-SPECIFIC INSTRUCTIONS"),
            "OpenAI-specific instructions missing"
        );
        assert!(
            layered_prompt.contains("direct and concise"),
            "OpenAI style guidance missing"
        );
        assert!(
            layered_prompt.contains("Performance Analyst"),
            "Base role prompt missing"
        );

        // Verify task context is included
        assert!(
            layered_prompt.contains("Optimize this loop"),
            "Task context not included"
        );
    }

    #[test]
    fn test_build_layered_prompt_with_gemini() {
        let role = AgentRole::SeniorEngineer;
        let task_context = "Review the architecture of this module";

        let layered_prompt = role.build_layered_prompt("gemini", task_context);

        // Verify provider-specific instructions for Gemini
        assert!(
            layered_prompt.contains("GEMINI-SPECIFIC INSTRUCTIONS"),
            "Gemini-specific instructions missing"
        );
        assert!(
            layered_prompt.contains("multiple perspectives"),
            "Gemini guidance missing"
        );
        assert!(
            layered_prompt.contains("Senior Engineer"),
            "Base role prompt missing"
        );

        // Verify task context is included
        assert!(
            layered_prompt.contains("Review the architecture"),
            "Task context not included"
        );
    }

    #[test]
    fn test_build_layered_prompt_with_unknown_provider() {
        let role = AgentRole::TestCoverageAnalyst;
        let task_context = "Check test coverage for this module";

        let layered_prompt = role.build_layered_prompt("unknown_provider", task_context);

        // Verify fallback to general instructions
        assert!(
            layered_prompt.contains("GENERAL INSTRUCTIONS"),
            "General instructions missing for unknown provider"
        );
        assert!(
            layered_prompt.contains("Test Coverage Analyst"),
            "Base role prompt missing"
        );

        // Verify task context is included
        assert!(
            layered_prompt.contains("Check test coverage"),
            "Task context not included"
        );
    }

    #[test]
    fn test_build_layered_prompt_empty_task_context() {
        let role = AgentRole::DocumentationReviewer;
        let layered_prompt = role.build_layered_prompt("anthropic", "");

        // Should not include CURRENT TASK section when context is empty
        assert!(
            !layered_prompt.contains("CURRENT TASK"),
            "Empty task context should not create section"
        );

        // Should still have provider and role layers
        assert!(
            layered_prompt.contains("SPECIALIZED AGENT ROLE"),
            "Agent role section should be present"
        );
        assert!(
            layered_prompt.contains("Documentation Reviewer"),
            "Base role prompt missing"
        );
    }

    #[test]
    fn test_system_prompt_for_provider_anthropic() {
        let role = AgentRole::FactualReviewer;
        let provider_prompt = role.system_prompt_for_provider("anthropic");

        // Verify base prompt
        assert!(provider_prompt.contains("Factual Reviewer"));

        // Verify Anthropic-specific additions
        assert!(provider_prompt.contains("CLAUDE-SPECIFIC INSTRUCTIONS"));
        assert!(provider_prompt.contains("<thinking>"));
        assert!(provider_prompt.contains("<analysis>"));
        assert!(provider_prompt.contains("<finding>"));
        assert!(provider_prompt.contains("XML-style tags"));
    }

    #[test]
    fn test_system_prompt_for_provider_openai() {
        let role = AgentRole::RedundancyChecker;
        let provider_prompt = role.system_prompt_for_provider("openai");

        // Verify base prompt
        assert!(provider_prompt.contains("Redundancy Checker"));

        // Verify OpenAI-specific additions
        assert!(provider_prompt.contains("GPT-SPECIFIC INSTRUCTIONS"));
        assert!(provider_prompt.contains("direct and concise"));
        assert!(provider_prompt.contains("bullet points"));
        assert!(provider_prompt.contains("actionable insights"));
    }

    #[test]
    fn test_system_prompt_for_provider_gemini() {
        let role = AgentRole::ConsistencyReviewer;
        let provider_prompt = role.system_prompt_for_provider("gemini");

        // Verify base prompt
        assert!(provider_prompt.contains("Consistency Reviewer"));

        // Verify Gemini-specific additions
        assert!(provider_prompt.contains("GEMINI-SPECIFIC INSTRUCTIONS"));
        assert!(provider_prompt.contains("multiple perspectives"));
        assert!(provider_prompt.contains("large context"));
        assert!(provider_prompt.contains("well-reasoned"));
    }

    #[test]
    fn test_system_prompt_for_provider_case_insensitive() {
        let role = AgentRole::PerformanceAnalyst;

        // Test various case insensitivities
        let uppercase = role.system_prompt_for_provider("ANTHROPIC");
        let lowercase = role.system_prompt_for_provider("anthropic");
        let mixed_case = role.system_prompt_for_provider("OpenAI");

        // All should contain provider-specific instructions
        assert!(uppercase.contains("CLAUDE-SPECIFIC INSTRUCTIONS"));
        assert!(lowercase.contains("CLAUDE-SPECIFIC INSTRUCTIONS"));
        assert!(mixed_case.contains("GPT-SPECIFIC INSTRUCTIONS"));
    }

    #[test]
    fn test_all_agent_roles_have_base_prompts() {
        let roles = AgentRole::all();

        for role in roles {
            let base_prompt = role.system_prompt();

            // Verify each role has a meaningful base prompt
            assert!(
                !base_prompt.is_empty(),
                "{:?} should have a non-empty base prompt",
                role
            );
            assert!(
                base_prompt.len() > 100,
                "{:?} base prompt seems too short",
                role
            );
            assert!(
                base_prompt.contains("You are"),
                "{:?} base prompt should introduce the role",
                role
            );
        }
    }

    #[test]
    fn test_layered_prompt_has_all_three_layers() {
        let role = AgentRole::SecurityExpert;
        let task_context = "Review this authentication code";

        // Test with Anthropic provider
        let anthropic_prompt = role.build_layered_prompt("anthropic", task_context);

        // Layer 1: Provider base (from metadata) - should contain provider info
        assert!(
            anthropic_prompt.len() > 200,
            "Layered prompt should be substantial"
        );

        // Layer 2: Provider-specific agent role
        assert!(anthropic_prompt.contains("SPECIALIZED AGENT ROLE"));
        assert!(anthropic_prompt.contains("CLAUDE-SPECIFIC INSTRUCTIONS"));

        // Layer 3: Task context
        assert!(anthropic_prompt.contains("CURRENT TASK"));
        assert!(anthropic_prompt.contains("Review this authentication code"));

        // Verify order: provider -> role -> task
        let role_pos = anthropic_prompt.find("SPECIALIZED AGENT ROLE").unwrap();
        let task_pos = anthropic_prompt.find("CURRENT TASK").unwrap();
        assert!(
            role_pos < task_pos,
            "Agent role should come before task context"
        );
    }

    #[test]
    fn test_different_providers_get_different_specialized_instructions() {
        let role = AgentRole::SeniorEngineer;
        let task_context = "Review this API design";

        let anthropic_prompt = role.build_layered_prompt("anthropic", task_context);
        let openai_prompt = role.build_layered_prompt("openai", task_context);
        let gemini_prompt = role.build_layered_prompt("gemini", task_context);

        // Each provider should have unique specialized instructions
        assert!(anthropic_prompt.contains("CLAUDE-SPECIFIC INSTRUCTIONS"));
        assert!(openai_prompt.contains("GPT-SPECIFIC INSTRUCTIONS"));
        assert!(gemini_prompt.contains("GEMINI-SPECIFIC INSTRUCTIONS"));

        // Verify the instructions are actually different
        assert!(anthropic_prompt.contains("<thinking>"));
        assert!(openai_prompt.contains("direct and concise"));
        assert!(gemini_prompt.contains("multiple perspectives"));
    }

    #[test]
    fn test_provider_aliases_work_correctly() {
        let role = AgentRole::FactualReviewer;

        // Test provider aliases
        let claude_prompt = role.system_prompt_for_provider("claude");
        let anthropic_prompt = role.system_prompt_for_provider("anthropic");
        let gpt_prompt = role.system_prompt_for_provider("gpt");
        let openai_prompt = role.system_prompt_for_provider("openai");

        // Aliases should map to same provider instructions
        assert!(claude_prompt.contains("CLAUDE-SPECIFIC INSTRUCTIONS"));
        assert!(anthropic_prompt.contains("CLAUDE-SPECIFIC INSTRUCTIONS"));
        assert!(gpt_prompt.contains("GPT-SPECIFIC INSTRUCTIONS"));
        assert!(openai_prompt.contains("GPT-SPECIFIC INSTRUCTIONS"));
    }

    #[test]
    fn test_layered_prompt_structure_consistency() {
        let roles = vec![
            AgentRole::FactualReviewer,
            AgentRole::SeniorEngineer,
            AgentRole::SecurityExpert,
            AgentRole::ConsistencyReviewer,
            AgentRole::RedundancyChecker,
            AgentRole::PerformanceAnalyst,
            AgentRole::TestCoverageAnalyst,
            AgentRole::DocumentationReviewer,
        ];

        let providers = vec!["anthropic", "openai", "gemini"];
        let task_context = "Analyze this code snippet";

        for role in roles {
            for provider in &providers {
                let layered_prompt = role.build_layered_prompt(provider, task_context);

                // All layered prompts should have consistent structure
                assert!(
                    layered_prompt.contains("SPECIALIZED AGENT ROLE"),
                    "Role {:?} with provider {} missing agent role section",
                    role,
                    provider
                );
                assert!(
                    layered_prompt.contains("CURRENT TASK"),
                    "Role {:?} with provider {} missing task context",
                    role,
                    provider
                );
                assert!(
                    layered_prompt.contains(task_context),
                    "Role {:?} with provider {} missing actual task text",
                    role,
                    provider
                );

                // Verify provider-specific section exists
                match *provider {
                    "anthropic" => assert!(layered_prompt.contains("CLAUDE-SPECIFIC INSTRUCTIONS")),
                    "openai" => assert!(layered_prompt.contains("GPT-SPECIFIC INSTRUCTIONS")),
                    "gemini" => assert!(layered_prompt.contains("GEMINI-SPECIFIC INSTRUCTIONS")),
                    _ => assert!(layered_prompt.contains("GENERAL INSTRUCTIONS")),
                }
            }
        }
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for multi_agent
    // =========================================================================

    // 1. AgentRole serde roundtrip for all variants
    #[test]
    fn agent_role_serde_roundtrip() {
        let roles = AgentRole::all();
        for role in &roles {
            let json = serde_json::to_string(role).unwrap();
            let decoded: AgentRole = serde_json::from_str(&json).unwrap();
            assert_eq!(*role, decoded);
        }
    }

    // 2. AgentRole::all() returns 8 roles
    #[test]
    fn agent_role_all_returns_eight() {
        assert_eq!(AgentRole::all().len(), 8);
    }

    // 3. AgentRole::from_name with various inputs
    #[test]
    fn agent_role_from_name_valid_inputs() {
        assert_eq!(
            AgentRole::from_name("factual"),
            Some(AgentRole::FactualReviewer)
        );
        assert_eq!(
            AgentRole::from_name("senior"),
            Some(AgentRole::SeniorEngineer)
        );
        assert_eq!(
            AgentRole::from_name("security"),
            Some(AgentRole::SecurityExpert)
        );
        assert_eq!(
            AgentRole::from_name("consistency"),
            Some(AgentRole::ConsistencyReviewer)
        );
        assert_eq!(
            AgentRole::from_name("redundancy"),
            Some(AgentRole::RedundancyChecker)
        );
        assert_eq!(
            AgentRole::from_name("performance"),
            Some(AgentRole::PerformanceAnalyst)
        );
        assert_eq!(
            AgentRole::from_name("test"),
            Some(AgentRole::TestCoverageAnalyst)
        );
        assert_eq!(
            AgentRole::from_name("docs"),
            Some(AgentRole::DocumentationReviewer)
        );
    }

    // 4. AgentRole::from_name with invalid input returns None
    #[test]
    fn agent_role_from_name_invalid_returns_none() {
        assert!(AgentRole::from_name("unknown").is_none());
        assert!(AgentRole::from_name("").is_none());
        assert!(AgentRole::from_name("ADMIN").is_none());
    }

    // 5. BroadcastPriority serde roundtrip for all variants
    #[test]
    fn broadcast_priority_serde_roundtrip() {
        let priorities = [
            BroadcastPriority::Low,
            BroadcastPriority::Medium,
            BroadcastPriority::High,
            BroadcastPriority::Critical,
        ];
        for p in &priorities {
            let json = serde_json::to_string(p).unwrap();
            let decoded: BroadcastPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, decoded);
        }
    }

    // 6. AgentMessage::Request serde roundtrip
    #[test]
    fn agent_message_request_serde() {
        let msg = AgentMessage::Request {
            from: AgentRole::SecurityExpert,
            to: AgentRole::SeniorEngineer,
            query: "Is this safe?".into(),
            context: "Reviewing auth module".into(),
            message_id: "msg-1".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, json2);
    }

    // 7. AgentMessage::Response serde roundtrip
    #[test]
    fn agent_message_response_serde() {
        let msg = AgentMessage::Response {
            from: AgentRole::SeniorEngineer,
            to: AgentRole::SecurityExpert,
            answer: "Yes, it's safe".into(),
            confidence: 0.85,
            request_id: "msg-1".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, json2);
    }

    // 8. AgentMessage::Broadcast serde roundtrip
    #[test]
    fn agent_message_broadcast_serde() {
        let msg = AgentMessage::Broadcast {
            from: AgentRole::FactualReviewer,
            announcement: "Found critical bug".into(),
            priority: BroadcastPriority::Critical,
            message_id: "msg-bc-1".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&decoded).unwrap();
        assert_eq!(json, json2);
    }

    // 9. AgentCommunicationHub::new creates empty hub
    #[test]
    fn communication_hub_new_is_empty() {
        let hub = AgentCommunicationHub::new();
        assert_eq!(hub.pending_count(), 0);
    }

    // 10. AgentCommunicationHub::default creates empty hub
    #[test]
    fn communication_hub_default_is_empty() {
        let hub = AgentCommunicationHub::default();
        assert_eq!(hub.pending_count(), 0);
    }

    // 11. send_request adds to pending
    #[test]
    fn send_request_adds_to_pending() {
        let mut hub = AgentCommunicationHub::new();
        hub.send_request(
            AgentRole::FactualReviewer,
            AgentRole::SecurityExpert,
            "Is this safe?".into(),
            "Auth review".into(),
        );
        assert_eq!(hub.pending_count(), 1);
    }

    // 12. broadcast adds to broadcasts list
    #[test]
    fn broadcast_adds_to_broadcasts() {
        let mut hub = AgentCommunicationHub::new();
        hub.broadcast(
            AgentRole::SeniorEngineer,
            "Deploy ready".into(),
            BroadcastPriority::High,
        );
        let by_priority = hub.get_broadcasts_by_priority();
        assert_eq!(by_priority.len(), 1);
    }

    // 13. AgentResponse serde roundtrip
    #[test]
    fn agent_response_serde_roundtrip() {
        let resp = AgentResponse {
            role: AgentRole::SecurityExpert,
            analysis: "Found SQL injection".into(),
            issues: vec!["SQL injection in login".into()],
            suggestions: vec!["Use parameterized queries".into()],
            severity: Some(IssueSeverity::Critical),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: AgentResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.role, AgentRole::SecurityExpert);
        assert_eq!(decoded.issues.len(), 1);
        assert_eq!(decoded.severity, Some(IssueSeverity::Critical));
    }

    // 14. IssueSeverity serde roundtrip for all variants
    #[test]
    fn issue_severity_serde_roundtrip() {
        let severities = [
            IssueSeverity::Low,
            IssueSeverity::Medium,
            IssueSeverity::High,
            IssueSeverity::Critical,
        ];
        for s in &severities {
            let json = serde_json::to_string(s).unwrap();
            let decoded: IssueSeverity = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    // 15. MultiAgentAnalysis serde roundtrip
    #[test]
    fn multi_agent_analysis_serde_roundtrip() {
        let analysis = MultiAgentAnalysis {
            agent_responses: vec![AgentResponse {
                role: AgentRole::FactualReviewer,
                analysis: "Code is correct".into(),
                issues: vec![],
                suggestions: vec!["Add more tests".into()],
                severity: None,
            }],
            consensus_issues: vec!["Missing tests".into()],
            critical_findings: vec![],
            summary: "Overall good quality".into(),
            confidence: 0.75,
        };
        let json = serde_json::to_string(&analysis).unwrap();
        let decoded: MultiAgentAnalysis = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_responses.len(), 1);
        assert_eq!(decoded.consensus_issues.len(), 1);
        assert!((decoded.confidence - 0.75).abs() < f64::EPSILON);
    }
}
