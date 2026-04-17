# Agent System & Orchestration - Detailed Design

## Overview

Agent system with 16 specialized agents, orchestration, and parallel execution.

## Architecture

```
Agent System Architecture:
┌─────────────────────────────────────────────────────────────┐
│ AgentContext                                              │
│ ├─ task: String                                           │
│ ├─ codebase: PathBuf                                      │
│ ├─ session: Session                                       │
│ └─ tools: Vec<Tool>                                       │
└───────────────────┬─────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│ AgentOrchestrator                                          │
│ ├─ Agent Selection                                         │
│ ├─ Parallel Execution                                      │
│ ├─ Result Aggregation                                      │
│ └─ Error Handling                                         │
└───────────────────┬─────────────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────────────┐
│ Specialized Agents                                         │
│ ├─ PlannerAgent                                           │
│ ├─ ArchitectAgent                                          │
│ ├─ TddGuideAgent                                          │
│ ├─ CodeReviewerAgent                                       │
│ ├─ SecurityReviewerAgent                                   │
│ ├─ DebuggerAgent                                           │
│ └─ ... (16 total)                                          │
└─────────────────────────────────────────────────────────────┘
```

## Agent Implementation

### Agent Trait

```rust
// crates/rustycode-core/src/agents/mod.rs

#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;
    fn can_handle(&self, context: &AgentContext) -> bool;
    async fn execute(&self, context: AgentContext) -> Result<AgentResult>;
}

pub struct AgentContext {
    pub task: String,
    pub codebase: PathBuf,
    pub session: Session,
    pub tools: Vec<Box<dyn Tool>>,
    pub capabilities: AgentCapabilities,
}

pub struct AgentResult {
    pub output: String,
    pub artifacts: Vec<Artifact>,
    pub next_actions: Vec<String>,
    pub confidence: f64,
}

pub struct Artifact {
    pub path: PathBuf,
    pub content: String,
    pub artifact_type: ArtifactType,
}

#[derive(Debug, Clone)]
pub enum ArtifactType {
    Code,
    Documentation,
    Test,
    Config,
}
```

### Orchestrator

```rust
// crates/rustycode-core/src/agents/orchestrator.rs

pub struct AgentOrchestrator {
    agents: Vec<Box<dyn Agent>>,
}

impl AgentOrchestrator {
    pub fn new() -> Self {
        Self {
            agents: vec![
                Box::new(PlannerAgent::new()),
                Box::new(ArchitectAgent::new()),
                Box::new(TddGuideAgent::new()),
                Box::new(CodeReviewerAgent::new()),
                Box::new(SecurityReviewerAgent::new()),
                Box::new(DebuggerAgent::new()),
            ],
        }
    }

    pub async fn execute(&self, context: AgentContext) -> Result<Vec<AgentResult>> {
        // Select agents that can handle the task
        let capable_agents: Vec<_> = self.agents
            .iter()
            .filter(|agent| agent.can_handle(&context))
            .collect();

        if capable_agents.is_empty() {
            return Err(AgentError::NoCapableAgent);
        }

        // Execute in parallel
        let futures: Vec<_> = capable_agents
            .into_iter()
            .map(|agent| {
                let context = context.clone();
                async move {
                    agent.execute(context).await
                }
            })
            .collect();

        let results: Result<Vec<_>> = futures::future::join_all(futures).await
            .into_iter()
            .collect();

        results
    }
}
```

### Specific Agents

```rust
// crates/rustycode-core/src/agents/planner.rs

pub struct PlannerAgent {
    llm_provider: Arc<dyn LLMProvider>,
}

impl Agent for PlannerAgent {
    fn name(&self) -> &str {
        "planner"
    }

    fn can_handle(&self, context: &AgentContext) -> bool {
        // Handle planning tasks
        context.task.contains("plan")
            || context.task.contains("design")
            || context.task.contains("architecture")
    }

    async fn execute(&self, context: AgentContext) -> Result<AgentResult> {
        let prompt = format!(
            "Create a step-by-step implementation plan for:\n\n{}",
            context.task
        );

        let request = CompletionRequest::new(
            "claude-3-5-sonnet-20250514".into(),
            vec![ChatMessage::user(prompt)],
        );

        let response = self.llm_provider.complete(request).await?;

        Ok(AgentResult {
            output: response.content,
            artifacts: vec![],
            next_actions: vec!["implement plan".into()],
            confidence: 0.9,
        })
    }
}
```

## Usage

```rust
let orchestrator = AgentOrchestrator::new();

let context = AgentContext {
    task: "Plan a REST API in Rust".into(),
    codebase: PathBuf::from("."),
    session: session.clone(),
    tools: vec![],
    capabilities: AgentCapabilities::default(),
};

let results = orchestrator.execute(context).await?;

for result in results {
    println!("{}: {}", result.agent_name, result.output);
}
```

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
```
