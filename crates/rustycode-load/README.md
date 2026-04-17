# rustycode-load

A comprehensive, high-performance load testing framework for Rust.

## Features

- **Flexible Scenario Definition**: Define custom load test scenarios with configurable parameters
- **Concurrent Request Generation**: Generate load with thousands of concurrent requests using tokio
- **Ramp-Up Strategies**: Linear, stepped, and custom ramp-up patterns
- **Response Time Tracking**: High-precision timing with microsecond granularity
- **Percentile Calculation**: Accurate p50, p90, p95, p99, and p999 calculations
- **Report Generation**: JSON, terminal, HTML, and Markdown output formats
- **Real-Time Monitoring**: Track progress during test execution
- **Error Classification**: Categorize failures by type (Network, HTTP, Timeout, etc.)
- **SLA Validation**: Define response time thresholds and error rate limits

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
rustycode-load = { path = "../rustycode-load" }
```

## Quick Start

```rust
use rustycode_load::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Define a load test scenario
    let scenario = LoadScenario::builder()
        .name("API Load Test")
        .concurrent_users(100)
        .duration(Duration::from_secs(60))
        .ramp_up(RampUpStrategy::Linear {
            duration: Duration::from_secs(10),
        })
        .request_generator(|user_id| {
            LoadRequest::http(
                format!("https://api.example.com/data/{}", user_id),
                reqwest::Method::GET,
            )
        })
        .build()?;

    // Run the load test
    let runner = LoadTestRunner::new();
    let results = runner.run(scenario).await?;

    // Print results
    results.print_summary();

    Ok(())
}
```

## Architecture

The framework is organized into several modules:

- **scenario**: Define load test scenarios with configurable parameters
- **executor**: Execute load tests with concurrent request generation
- **metrics**: Collect and analyze performance metrics
- **report**: Generate reports in multiple formats
- **ramp_up**: Configure ramp-up strategies
- **request**: Define custom request types
- **error**: Error handling and classification

## Core Concepts

### Scenarios

A `LoadScenario` defines the parameters of your load test:

```rust
let scenario = LoadScenario::builder()
    .name("My Load Test")
    .description("Testing API endpoint")
    .concurrent_users(50)
    .duration(Duration::from_secs(60))
    .ramp_up(RampUpStrategy::Linear {
        duration: Duration::from_secs(10),
    })
    .think_time(Duration::from_millis(100))
    .request_generator(|user_id| {
        // Generate requests for each user
        LoadRequest::http_get("https://api.example.com/data".to_string())
    })
    .response_time_threshold(ResponseTimeThreshold {
        p50: Duration::from_millis(100),
        p90: Duration::from_millis(200),
        p95: Duration::from_millis(300),
        p99: Duration::from_millis(500),
    })
    .max_error_rate(0.01) // 1%
    .build()?;
```

### Request Types

The framework supports multiple request types:

#### HTTP Requests

```rust
let request = LoadRequest::http_get("https://api.example.com/data".to_string());

let request = LoadRequest::http_post("https://api.example.com/data".to_string())
    .with_header("Authorization".to_string(), "Bearer token".to_string())
    .with_body(r#"{"key": "value"}"#.to_string());
```

#### Custom Async Requests

```rust
let request = LoadRequest::custom(|ctx| {
    Box::pin(async move {
        // Execute custom logic
        let start = std::time::Instant::now();

        // ... perform operation ...

        let duration = start.elapsed();
        Ok(LoadResult::success(duration))
    })
});
```

### Ramp-Up Strategies

Control how users are ramped up:

```rust
// Immediate: Start all users at once
let immediate = RampUpStrategy::Immediate;

// Linear: Gradually increase users over time
let linear = RampUpStrategy::Linear {
    duration: Duration::from_secs(30),
};

// Stepped: Add users in discrete steps
let stepped = RampUpStrategy::Stepped {
    steps: 5,
    step_duration: Duration::from_secs(10),
};
```

### Metrics and Results

Access detailed metrics:

```rust
let results = runner.run(scenario).await?;

// Response times
println!("Median: {:?}", results.response_times.p50);
println!("p90: {:?}", results.response_times.p90);
println!("p99: {:?}", results.response_times.p99);

// Throughput
println!("Total requests: {}", results.throughput.total_requests);
println!("Error rate: {:.2}%", results.throughput.error_rate * 100.0);
println!("Throughput: {:.2} req/s", results.throughput.throughput_per_second);

// Errors
for (category, count) in &results.errors.by_category {
    println!("{}: {}", category.name(), count);
}

// Per-user metrics
for (user_id, metrics) in &results.per_user_metrics {
    println!("User {}: {} requests, avg {:?}",
        user_id,
        metrics.total_requests,
        metrics.avg_response_time
    );
}
```

### Report Generation

Generate reports in multiple formats:

```rust
// Terminal report
let term_gen = ReportGenerator::new(ReportFormat::Terminal);
let term_report = term_gen.generate(&results)?;
println!("{}", term_report);

// JSON report
let json_gen = ReportGenerator::new(ReportFormat::Json);
let json_report = json_gen.generate(&results)?;
std::fs::write("results.json", json_report)?;

// HTML report
let html_gen = ReportGenerator::new(ReportFormat::Html);
let html_report = html_gen.generate(&results)?;
std::fs::write("results.html", html_report)?;

// Markdown report
let md_gen = ReportGenerator::new(ReportFormat::Markdown);
let md_report = md_gen.generate(&results)?;
std::fs::write("results.md", md_report)?;
```

## Advanced Usage

### Progress Monitoring

Monitor test execution in real-time:

```rust
let results = runner
    .run_with_progress(scenario, |progress| {
        println!(
            "Progress: {:.1}% | Active: {} | Requests: {} | RPS: {:.2}",
            progress.percent_complete(),
            progress.active_users,
            progress.completed_requests,
            progress.current_rps
        );
    })
    .await?;
```

### Custom Error Handling

```rust
let scenario = LoadScenario::builder()
    .name("Error Handling Test")
    .stop_on_error(false) // Continue on errors
    .max_error_rate(0.05) // Allow 5% error rate
    .request_generator(|user_id| {
        LoadRequest::custom(move |_| {
            Box::pin(async move {
                // Custom error handling logic
                match perform_operation().await {
                    Ok(result) => Ok(LoadResult::success(result.duration)),
                    Err(e) => Ok(LoadResult::error(
                        result.duration,
                        format!("Operation failed: {}", e),
                    )),
                }
            })
        })
    })
    .build()?;
```

### SLA Validation

```rust
let threshold = ResponseTimeThreshold {
    p50: Duration::from_millis(100),
    p90: Duration::from_millis(200),
    p95: Duration::from_millis(300),
    p99: Duration::from_millis(500),
};

let scenario = LoadScenario::builder()
    .response_time_threshold(threshold)
    .max_error_rate(0.01)
    // ... other configuration
    .build()?;

// After running the test
let results = runner.run(scenario).await?;

if let Some(threshold) = &scenario.response_time_threshold {
    let p50_ok = results.response_times.p50 <= threshold.p50;
    let p90_ok = results.response_times.p90 <= threshold.p90;
    // ... check other percentiles

    println!("SLA: {}", if p50_ok && p90_ok { "PASS" } else { "FAIL" });
}
```

## Configuration

### Default Values

```rust
pub mod defaults {
    pub const DEFAULT_CONCURRENT_USERS: usize = 10;
    pub const DEFAULT_DURATION: Duration = Duration::from_secs(60);
    pub const DEFAULT_RAMP_UP_DURATION: Duration = Duration::from_secs(10);
    pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
    pub const DEFAULT_THINK_TIME: Duration = Duration::from_millis(100);
    pub const DEFAULT_METRICS_INTERVAL: Duration = Duration::from_secs(1);
    pub const DEFAULT_MAX_ERROR_RATE: f64 = 0.01; // 1%
}
```

### Custom Configuration

```rust
let config = LoadTestConfig {
    http_client: Some(Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap()),
    metrics_interval: Duration::from_millis(500),
    collect_time_series: true,
    max_concurrent_per_user: 5,
};

let runner = LoadTestRunner::with_config(config);
```

## Examples

See the `examples/load-testing/` directory for complete examples:

- `basic_load_test.rs`: Simple load test with custom requests
- `http_load_test.rs`: HTTP endpoint load testing with SLA validation
- `custom_scenario.rs`: Advanced scenarios with error injection and progress monitoring

Run examples:

```bash
cargo run --example basic_load_test
cargo run --example http_load_test
cargo run --example custom_scenario
```

## Testing

Run the test suite:

```bash
# Run all tests
cargo test --package rustycode-load

# Run with output
cargo test --package rustycode-load -- --nocapture

# Run integration tests
cargo test --package rustycode-load --test integration_test
```

## Performance

The framework is designed for high-performance load testing:

- **Async/Await**: Built on tokio for efficient concurrent execution
- **Zero-Copy Metrics**: Channels for efficient metrics collection
- **Memory Efficient**: Streams metrics instead of buffering all results
- **Scalable**: Tested with 10,000+ concurrent users

## Best Practices

1. **Start Small**: Begin with low concurrent users and gradually increase
2. **Use Ramp-Up**: Avoid sudden load spikes with gradual ramp-up
3. **Monitor Progress**: Use progress callbacks for long-running tests
4. **Set SLAs**: Define response time thresholds and error rate limits
5. **Analyze Errors**: Review error categories to identify bottlenecks
6. **Save Reports**: Generate HTML/JSON reports for historical analysis
7. **Test Realistically**: Simulate actual user behavior with think time

## Error Categories

The framework categorizes errors automatically:

- **Network**: DNS failures, connection refused, timeouts
- **HTTP**: 4xx and 5xx status codes
- **Timeout**: Request timeout errors
- **Application**: Business logic errors
- **Serialization**: Parse/encode errors
- **Other**: Uncategorized errors

## License

MIT

## Contributing

Contributions are welcome! Please read the contributing guidelines before submitting PRs.
