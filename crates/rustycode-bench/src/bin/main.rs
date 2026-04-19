use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use rustycode_bench::{
    agent::{BenchAgent, CodeAgent, CodeAgentConfig},
    dataset::DatasetRegistry,
    job::{Job, JobConfig},
    task::ResolvedTask,
    verifier::{ScriptVerifier, Verifier},
};

#[derive(Parser)]
#[command(name = "rtk-bench")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        dataset_dir: PathBuf,
        job_name: Option<String>,
        concurrent: Option<usize>,
    },
    List {
        dataset_dir: Option<PathBuf>,
        #[arg(short, long)]
        verbose: bool,
    },
    Verify {
        task_dir: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .init();

    match cli.command {
        Commands::Run {
            dataset_dir,
            job_name,
            concurrent,
        } => run_benchmark(dataset_dir, job_name, concurrent).await,

        Commands::List {
            dataset_dir,
            verbose,
        } => list_tasks(dataset_dir, verbose),

        Commands::Verify { task_dir } => verify_task(task_dir),
    }
}

async fn run_benchmark(
    dataset_dir: PathBuf,
    job_name: Option<String>,
    concurrent: Option<usize>,
) -> Result<()> {
    let has_key = std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("GOOGLE_API_KEY").is_ok();

    if !has_key {
        bail!("No API key. Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GOOGLE_API_KEY");
    }

    let job_name = job_name.unwrap_or_else(|| {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        format!("bench_{}", timestamp)
    });

    let n_concurrent = concurrent.unwrap_or(1);

    tracing::info!(
        "Starting benchmark: {} (concurrent: {})",
        job_name,
        n_concurrent
    );

    let registry = DatasetRegistry::new();
    let dataset_path = if dataset_dir.exists() {
        dataset_dir.clone()
    } else if let Ok(p) = registry.resolve(dataset_dir.to_str().unwrap_or_default()) {
        p
    } else {
        bail!("Dataset not found: {:?}", dataset_dir);
    };

    let tasks = ResolvedTask::discover(&dataset_path)
        .with_context(|| format!("Failed to discover tasks in {}", dataset_path.display()))?;

    if tasks.is_empty() {
        bail!("No tasks found in {}", dataset_path.display());
    }

    tracing::info!("Found {} tasks", tasks.len());

    let jobs_dir = dataset_dir.join("_jobs");
    std::fs::create_dir_all(&jobs_dir)?;

    let job_config = JobConfig {
        job_name: job_name.clone(),
        jobs_dir,
        n_concurrent,
        force_build: false,
        cleanup: true,
    };

    let provider = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        "anthropic"
    } else if std::env::var("OPENAI_API_KEY").is_ok() {
        "openai"
    } else {
        bail!("Unsupported provider");
    };

    tracing::info!("Using provider: {}", provider);

    let agent_factory = move |_solution_dir: PathBuf| -> Box<dyn BenchAgent> {
        let cfg = CodeAgentConfig {
            provider: provider.to_string(),
            ..Default::default()
        };
        let agent = CodeAgent::auto(cfg).expect("Failed to create CodeAgent");
        Box::new(agent) as Box<dyn BenchAgent>
    };

    let verifier_factory = move |tests_dir: PathBuf, timeout_secs: u64| -> Box<dyn Verifier> {
        Box::new(ScriptVerifier::new(tests_dir, timeout_secs)) as Box<dyn Verifier>
    };

    tracing::info!("Running benchmarks...");
    let results = Job::new(job_config)
        .run(&dataset_path, &agent_factory, &verifier_factory)
        .await
        .with_context(|| "Failed to run benchmark")?;

    println!("\n=== Benchmark Results ===");
    println!("Total: {}", results.total);
    println!("Passed: {}", results.passed);
    println!("Failed: {}", results.failed);
    println!("Pass rate: {:.1}%", results.accuracy * 100.0);
    println!("Mean reward: {:.3}", results.mean_reward);

    if !results.task_results.is_empty() {
        println!("\n=== Task Results ===");
        for task_result in &results.task_results {
            let status = if task_result.passed { "PASS" } else { "FAIL" };
            println!(
                "- {}: {} (reward: {:.2})",
                task_result.task_name, status, task_result.reward
            );
        }
    }

    Ok(())
}

fn list_tasks(dataset_dir: Option<PathBuf>, verbose: bool) -> Result<()> {
    let registry = DatasetRegistry::new();

    if let Some(dir) = dataset_dir {
        let path = if dir.exists() {
            dir.clone()
        } else if let Ok(p) = registry.resolve(dir.to_str().unwrap_or_default()) {
            p
        } else {
            bail!("Directory not found: {:?}", dir);
        };

        let tasks = ResolvedTask::discover(&path)?;

        println!("\n=== Tasks in {} ===", path.display());
        println!("Total: {}\n", tasks.len());

        for task in &tasks {
            if verbose {
                println!("- {}", task.name);
                println!("  Category: {}", task.config.metadata.category);
                println!("  Difficulty: {}", task.config.metadata.difficulty);
                println!();
            } else {
                println!("- {}", task.name);
            }
        }
    } else {
        let datasets = registry.list_datasets();

        if datasets.is_empty() {
            println!("No datasets found.");
            println!("\nSearch paths checked:");
            if let Ok(home) = std::env::var("HOME") {
                println!("  - {}/.cache/harbor/tasks", home);
            }
            println!("  - ./");
            return Ok(());
        }

        println!("\n=== Available Datasets ===\n");
        for ds in &datasets {
            println!("- {} ({} tasks)", ds.name, ds.task_count);
            if verbose {
                println!("  Path: {:?}", ds.path);
                println!();
            }
        }
    }

    Ok(())
}

fn verify_task(task_dir: PathBuf) -> Result<()> {
    let task = ResolvedTask::from_dir(&task_dir)
        .with_context(|| format!("Failed to load task from {}", task_dir.display()))?;

    println!("\n=== Task: {} ===", task.name);
    println!("Category: {}", task.config.metadata.category);
    println!("Difficulty: {}", task.config.metadata.difficulty);

    let has_instructions = !task.instruction.is_empty();
    println!(
        "\nInstructions: {}",
        if has_instructions { "OK" } else { "MISSING" }
    );

    let has_dockerfile = task.environment_dir.join("Dockerfile").exists();
    println!(
        "Dockerfile: {}",
        if has_dockerfile { "OK" } else { "MISSING" }
    );

    let has_tests = task.tests_dir.join("test.sh").exists();
    println!("Tests: {}", if has_tests { "OK" } else { "MISSING" });

    let has_solution = task.solution_dir.exists();
    println!(
        "Solution: {}",
        if has_solution { "OK" } else { "NOT PROVIDED" }
    );

    let valid = has_instructions && has_dockerfile && has_tests;
    println!("\nStatus: {}", if valid { "VALID" } else { "INCOMPLETE" });

    if !valid {
        bail!("Task is incomplete");
    }

    Ok(())
}