# Agent System Guide

## Agent Architecture

RustyCode's agent system provides a powerful framework for building specialized AI agents that can handle specific tasks, collaborate with each other, and deliver comprehensive analysis through orchestrated workflows.

### Core Components

```
┌─────────────────────────────────────────────────────────────┐
│                    Agent System                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐     ┌──────────────┐     ┌────────────┐ │
│  │    Agent     │     │ Orchestrator │     │  Registry  │ │
│  │   Trait      │────▶│  (Chief of   │────▶│            │ │
│  │              │     │   Staff)     │     │            │ │
│  └──────────────┘     └──────────────┘     └────────────┘ │
│         │                                          │        │
│         ▼                                          ▼        │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Built-in Agents                        │   │
│  │  ┌────────────┐ ┌────────────┐ ┌────────────┐     │   │
│  │  │ Code Review│ │  Security  │ │   Testing  │     │   │
│  │  │   Agent    │ │   Expert   │ │  Analyst   │     │   │
│  │  └────────────┘ └────────────┘ └────────────┘     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Agent Trait Interface

All agents implement the `Agent` trait:

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    /// Get agent identifier
    fn id(&self) -> &str;

    /// Get agent display name
    fn name(&self) -> &str;

    /// Get agent description
    fn description(&self) -> &str;

    /// Get agent system prompt
    fn system_prompt(&self) -> String;

    /// Execute agent with input
    async fn execute(&self, input: &str) -> Result<AgentResult>;
}
```

### Agent Registry

The `SubagentRegistry` manages available agents:

```rust
use rustycode_core::agents::SubagentRegistry;

let registry = SubagentRegistry::with_defaults();

// List available agents
for agent_id in registry.list_ids() {
    if let Some(agent) = registry.get(&agent_id) {
        println!("{}: {}", agent.name(), agent.description());
    }
}

// Register custom agent
registry.register(custom_agent)?;

// Load agents from directory
registry.load_from_directory(Path::new("/path/to/agents"))?;
```

## Built-in Agents

### Code Reviewer Agent

**Purpose**: Review code for quality, maintainability, and best practices

**Use Cases**:
- Pull request reviews
- Code quality assessments
- Refactoring suggestions
- Best practice validation

**Example**:
```rust
use rustycode_runtime::multi_agent::{AgentRole, MultiAgentOrchestrator, MultiAgentConfig};

let config = MultiAgentConfig {
    roles: vec![AgentRole::SeniorEngineer],
    content: r#"
fn calculate(a: i32, b: i32) -> i32 {
    a + b
}
"#.to_string(),
    ..MultiAgentConfig::default()
};

let orchestrator = MultiAgentOrchestrator::from_config(config)?;
let analysis = orchestrator.analyze().await?;

println!("{}", MultiAgentOrchestrator::format_analysis(&analysis));
```

### Security Expert Agent

**Purpose**: Identify security vulnerabilities and risks

**Use Cases**:
- Security audits
- Vulnerability scanning
- Dependency analysis
- Authentication/authorization review

**Findings**:
- SQL injection vulnerabilities
- XSS vulnerabilities
- Authentication flaws
- Data exposure issues
- Cryptographic errors

### Test Coverage Analyst

**Purpose**: Evaluate test coverage and quality

**Use Cases**:
- Test gap analysis
- Coverage reports
- Test quality assessment
- Edge case identification

**Output**:
- Missing test cases
- Untested code paths
- Test quality issues
- Coverage metrics

### Performance Analyst

**Purpose**: Analyze performance characteristics

**Use Cases**:
- Performance optimization
- Bottleneck identification
- Algorithm analysis
- Resource usage review

**Findings**:
- Inefficient algorithms
- Memory leaks
- I/O bottlenecks
- Caching opportunities

## Using Agents

### Single Agent Execution

Execute a single agent for a specific task:

```rust
use rustycode_core::agents::SubagentRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    let registry = SubagentRegistry::with_defaults();
    let agent = registry.get("code-reviewer")
        .ok_or("Agent not found")?;

    let code = r#"
fn process_data(data: Vec<i32>) -> Vec<i32> {
    data.iter().map(|x| x * 2).collect()
}
"#;

    let result = agent.execute(code).await?;

    if result.success {
        println!("Review: {}", result.content);
    }

    Ok(())
}
```

### Parallel Agent Execution

Run multiple agents in parallel for comprehensive analysis:

```rust
use rustycode_runtime::multi_agent::{AgentRole, MultiAgentOrchestrator, MultiAgentConfig};
use futures::future::join_all;

#[tokio::main]
async fn main() -> Result<()> {
    let config = MultiAgentConfig {
        roles: vec![
            AgentRole::FactualReviewer,
            AgentRole::SecurityExpert,
            AgentRole::PerformanceAnalyst,
        ],
        content: "your code here".to_string(),
        max_parallelism: 3,
        ..MultiAgentConfig::default()
    };

    let orchestrator = MultiAgentOrchestrator::from_config(config)?;
    let analysis = orchestrator.analyze().await?;

    println!("=== Multi-Agent Analysis ===");
    println!("Consensus Issues: {}", analysis.consensus_issues.len());
    println!("Critical Findings: {}", analysis.critical_findings.len());
    println!("Confidence: {:.1}%", analysis.confidence * 100.0);

    for response in analysis.agent_responses {
        println!("\n=== {} ===", response.role.name());
        println!("{}", response.analysis);
    }

    Ok(())
}
```

### Multi-Agent Workflows

Create complex workflows with multiple agents:

```rust
use rustycode_runtime::multi_agent::{AgentRole, MultiAgentOrchestrator, MultiAgentConfig};

#[tokio::main]
async fn main() -> Result<()> {
    let code = std::fs::read_to_string("src/main.rs")?;

    // Stage 1: Code review
    let review_config = MultiAgentConfig {
        roles: vec![AgentRole::SeniorEngineer],
        content: code.clone(),
        ..MultiAgentConfig::default()
    };

    let orchestrator = MultiAgentOrchestrator::from_config(review_config)?;
    let review = orchestrator.analyze().await?;

    // Stage 2: Security analysis (if review passed)
    if review.confidence > 0.7 {
        let security_config = MultiAgentConfig {
            roles: vec![AgentRole::SecurityExpert],
            content: code.clone(),
            context: review.summary,
            ..MultiAgentConfig::default()
        };

        let security_orchestrator = MultiAgentOrchestrator::from_config(security_config)?;
        let security = security_orchestrator.analyze().await?;

        println!("Review: {}", review.summary);
        println!("Security: {}", security.summary);
    }

    Ok(())
}
```

## Creating Custom Agents

### Implementing the Agent Trait

```rust
use rustycode_core::agents::{Agent, AgentResult};
use async_trait::async_trait;

pub struct MyCustomAgent {
    id: String,
    name: String,
    description: String,
}

#[async_trait]
impl Agent for MyCustomAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn system_prompt(&self) -> String {
        "You are a custom agent specializing in...".to_string()
    }

    async fn execute(&self, input: &str) -> Result<AgentResult> {
        // Implement agent logic
        Ok(AgentResult::success("Analysis complete".to_string()))
    }
}
```

### Registering Custom Agents

```rust
use rustycode_core::agents::SubagentRegistry;

let custom_agent = MyCustomAgent {
    id: "my-agent".to_string(),
    name: "My Custom Agent".to_string(),
    description: "Does something specific".to_string(),
};

let registry = SubagentRegistry::with_defaults();
registry.register(Box::new(custom_agent))?;

// Use the agent
let agent = registry.get("my-agent").unwrap();
let result = agent.execute("input").await?;
```

### Agent from File System

Load agent definitions from files:

```rust
use rustycode_core::agents::SubagentRegistry;

let registry = SubagentRegistry::new();

// Load agents from directory
registry.load_from_directory(Path::new("/path/to/agents"))?;

// Agent file format (JSON):
/*
{
  "id": "custom-agent",
  "name": "Custom Agent",
  "description": "A custom agent",
  "system_prompt": "You are a specialist in...",
  "provider": "anthropic",
  "model": "claude-3-5-sonnet-latest"
}
*/
```

## Agent Patterns

### Orchestrator Pattern (Chief of Staff)

The orchestrator pattern coordinates multiple specialized agents:

```rust
use rustycode_core::agents::Orchestrator;

let orchestrator = Orchestrator::with_defaults();

// Process request through orchestrator
let result = orchestrator.process("Review this code").await?;

// Orchestrator automatically:
// 1. Analyzes request
// 2. Routes to appropriate agent
// 3. Integrates results
// 4. Returns unified response
```

### Split-Role Pattern

Use multiple agents with different perspectives:

```rust
use rustycode_runtime::multi_agent::{AgentRole, MultiAgentConfig};

let config = MultiAgentConfig {
    roles: vec![
        AgentRole::FactualReviewer,      // Check correctness
        AgentRole::SeniorEngineer,       // Assess architecture
        AgentRole::SecurityExpert,       // Find vulnerabilities
        AgentRole::ConsistencyReviewer,  // Check consistency
    ],
    content: code.to_string(),
    ..MultiAgentConfig::default()
};

let orchestrator = MultiAgentOrchestrator::from_config(config)?;
let analysis = orchestrator.analyze().await?;

// Each agent provides unique perspective
// Results are aggregated for comprehensive view
```

### Sequential Refinement Pattern

Chain agents to progressively refine output:

```rust
// Stage 1: Draft
let draft_agent = registry.get("coder").unwrap();
let draft = draft_agent.execute("Implement feature X").await?;

// Stage 2: Review
let review_agent = registry.get("reviewer").unwrap();
let review = review_agent.execute(&draft.content).await?;

// Stage 3: Refine
let refine_agent = registry.get("refiner").unwrap();
let refined = refine_agent.execute(&review.content).await?;

// Final output is progressively refined
```

### Parallel Execution Pattern

Run multiple agents simultaneously for speed:

```rust
use futures::future::join_all;

let agents = vec![
    registry.get("security-expert").unwrap(),
    registry.get("performance-analyst").unwrap(),
    registry.get("test-coverage-analyst").unwrap(),
];

let tasks: Vec<_> = agents.iter()
    .map(|agent| agent.execute(code))
    .collect();

let results = join_all(tasks).await?;

// All agents ran in parallel
// Results are ready simultaneously
```

## Best Practices

### Agent Design

1. **Single Responsibility**: Each agent should have one clear purpose
2. **Clear Interface**: Well-defined inputs and outputs
3. **Idempotent**: Same input should produce same output
4. **Error Handling**: Graceful degradation on failures

### Agent Composition

1. **Start Simple**: Begin with single agent execution
2. **Add Parallelism**: Add parallel agents for comprehensive analysis
3. **Orchestrate**: Use orchestrator for complex workflows
4. **Iterate**: Refine agent prompts and configurations

### Prompt Engineering

1. **Layered Prompts**: Provider capabilities + agent role + task context
2. **Clear Instructions**: Explicit expectations and output format
3. **Examples**: Provide examples for complex tasks
4. **Validation**: Check agent outputs against expectations

### Performance

1. **Limit Parallelism**: Control concurrent agent execution
2. **Cache Results**: Cache agent outputs for repeated queries
3. **Use Streaming**: Stream responses for long-running agents
4. **Monitor Costs**: Track token usage and costs

## Troubleshooting

### Agent Not Found

**Problem**: Agent ID not recognized

**Solutions**:
```rust
// List available agents
let registry = SubagentRegistry::with_defaults();
for agent_id in registry.list_ids() {
    println!("{}", agent_id);
}

// Check specific agent
if let Some(agent) = registry.get("my-agent") {
    println!("Found: {}", agent.name());
} else {
    println!("Agent not found");
}
```

### Agent Execution Failed

**Problem**: Agent execution returns error

**Solutions**:
```rust
match agent.execute(input).await {
    Ok(result) => {
        if !result.success {
            eprintln!("Agent failed: {}", result.error);
        } else {
            println!("Success: {}", result.content);
        }
    }
    Err(e) => {
        eprintln!("Execution error: {}", e);
    }
}
```

### Poor Agent Output

**Problem**: Agent output is not useful

**Solutions**:
1. **Refine system prompt**:
```rust
impl Agent for MyAgent {
    fn system_prompt(&self) -> String {
        r#"
You are a specialist agent. Your role is to:
1. Analyze the input carefully
2. Identify specific issues
3. Provide actionable recommendations
4. Format output as bullet points

Output Format:
- Issue 1
  - Severity: [High/Medium/Low]
  - Recommendation: [actionable advice]
"#.to_string()
    }
}
```

2. **Provide context**:
```rust
let config = MultiAgentConfig {
    roles: vec![AgentRole::SecurityExpert],
    content: code.to_string(),
    context: "This is a payment processing system".to_string(),
    instructions: Some("Focus on PCI compliance".to_string()),
    ..MultiAgentConfig::default()
};
```

3. **Use appropriate model**:
```rust
// Use more capable model for complex tasks
let config = MultiAgentConfig {
    roles: vec![AgentRole::SecurityExpert],
    model: "claude-opus-4-6".to_string(),  // Most capable
    ..MultiAgentConfig::default()
};
```

## Conclusion

The RustyCode agent system is designed to be:
- **Modular**: Easy to create and combine agents
- **Flexible**: Support for various execution patterns
- **Extensible**: Simple to add custom agents
- **Production-Ready**: Robust error handling and monitoring

For more information, see:
- [Architecture Overview](ARCHITECTURE.md)
- [Provider Guide](PROVIDERS.md)
- [MCP Integration Guide](MCP.md)
