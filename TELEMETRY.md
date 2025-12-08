# Telemetry and Observability

joerl provides comprehensive telemetry support for monitoring and observability of actor systems. This document describes the telemetry features, available metrics, and how to integrate with various observability backends.

## Table of Contents

- [Overview](#overview)
- [Enabling Telemetry](#enabling-telemetry)
- [Available Metrics](#available-metrics)
- [Integration Examples](#integration-examples)
- [Best Practices](#best-practices)

## Overview

joerl's telemetry system is built on industry-standard crates:

- **`metrics`**: For counters, gauges, and histograms
- **`tracing`**: For structured logging and distributed tracing
- **`tracing-opentelemetry`**: For OpenTelemetry integration (optional)

Telemetry is **optional** and zero-cost when disabled. When the `telemetry` feature is not enabled, all telemetry operations compile to no-ops with zero runtime overhead.

## Enabling Telemetry

### Feature Flag

Add the `telemetry` feature to your `Cargo.toml`:

```toml
[dependencies]
joerl = { version = "0.4", features = ["telemetry"] }
```

### Initialization

```rust
use joerl::telemetry;

fn main() {
    // Initialize telemetry
    telemetry::init();
    
    // Your actor system code here
}
```

### Sampling Configuration

For high-throughput systems, sampling reduces telemetry overhead by recording only a percentage of metrics:

```rust
use joerl::telemetry::{TelemetryConfig, set_config};

fn main() {
    // Configure 10% sampling for high-frequency operations
    let config = TelemetryConfig {
        message_sampling_rate: 10,      // Sample 10% of messages
        queue_wait_sampling_rate: 10,   // Sample 10% of queue wait times
        signal_sampling_rate: 100,       // Keep signals at 100% (low frequency)
    };
    
    set_config(config);
    
    // Initialize after configuration
    joerl::telemetry::init();
    
    // Your actor system code
}
```

**Sampling rates** (0-100%):
- `100` = Record all events (default, no sampling)
- `10` = Record 10% of events (10x reduction in overhead)
- `1` = Record 1% of events (100x reduction)
- `0` = Disable metric (no recording)

**When to use sampling:**
- Message rates > 10,000/sec per actor
- Systems with hundreds of actors
- Production deployments where overhead matters
- Keep signal sampling at 100% (typically low frequency)

## Available Metrics

joerl automatically tracks the following metrics when telemetry is enabled:

### Actor Lifecycle

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_actors_spawned_total` | Counter | Total actors spawned | `type` |
| `joerl_actors_stopped_total` | Counter | Total actors stopped | `type`, `reason` |
| `joerl_actors_active` | Gauge | Current number of active actors | `type` |
| `joerl_actors_panicked_total` | Counter | Total actors that panicked | `type` |
| `joerl_actor_lifetime_seconds` | Histogram | Actor lifetime duration | `type` |
| `joerl_short_lived_actors_total` | Counter | Actors that lived < 1 second | `type` |

**Type labels**: Actor struct name (e.g., `"Worker"`, `"Supervisor"`, `"boxed"` for dynamically spawned actors)

**Reason labels**: `normal`, `shutdown`, `killed`, `panic`, `custom`

### Message Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_messages_sent_total` | Counter | Total messages sent successfully | - |
| `joerl_messages_sent_failed_total` | Counter | Failed message send attempts | `reason` |
| `joerl_messages_processed_total` | Counter | Total messages processed | - |
| `joerl_message_processing_duration_seconds` | Histogram | Message processing latency | - |
| `joerl_message_queue_wait_seconds` | Histogram | Time messages spend in queue | - |

**Failure reason labels**: `mailbox_full`, `actor_not_found`

### Mailbox Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_mailbox_depth` | Gauge | Current mailbox queue depth | `type` |
| `joerl_mailbox_utilization_percent` | Gauge | Mailbox utilization percentage (0-100) | `type` |
| `joerl_mailbox_full_total` | Counter | Times mailbox was full | `type` |

**Type labels**: Actor struct name (same as actor lifecycle metrics)

### Links and Monitors

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_links_created_total` | Counter | Total links created | - |
| `joerl_monitors_created_total` | Counter | Total monitors created | - |

### Supervisor Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_supervisor_restarts_total` | Counter | Total child restarts | `strategy` |
| `joerl_supervisor_restart_intensity_exceeded_total` | Counter | Times restart intensity was exceeded | - |
| `joerl_supervisor_restart_duration_seconds` | Histogram | Time taken to restart a child | - |

**Strategy labels**: `one_for_one`, `one_for_all`, `rest_for_one`

### GenServer Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_gen_server_call_duration_seconds` | Histogram | Synchronous call latency | `type` |
| `joerl_gen_server_casts_total` | Counter | Total casts sent | `type` |
| `joerl_gen_server_call_timeouts_total` | Counter | Total call timeouts | `type` |
| `joerl_gen_server_calls_in_flight` | Gauge | Current calls in progress | `type` |

**Type labels**: GenServer struct name (e.g., `"Counter"`, `"WorkerServer"`)

### GenStatem Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_gen_statem_transitions_total` | Counter | Total state transitions | `type`, `from`, `to` |
| `joerl_gen_statem_invalid_transitions_total` | Counter | Invalid transition attempts | `type`, `state`, `event` |
| `joerl_gen_statem_state_duration_seconds` | Histogram | Time spent in each state | `type`, `state` |
| `joerl_gen_statem_current_state` | Gauge | Current state of FSMs | `type`, `state` |

**Type labels**: GenStatem struct name (e.g., `"Door"`, `"Connection"`)

**State/Event labels**: Debug representation of state/event enums (e.g., `"Locked"`, `"Open"`)

### Signal Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_signals_sent_total` | Counter | Total signals sent | `type` |
| `joerl_signals_received_total` | Counter | Total signals received | `type` |
| `joerl_signals_ignored_total` | Counter | Signals ignored (trapped) | `type` |
| `joerl_exit_signals_by_reason_total` | Counter | Exit signals by reason | `reason` |

**Signal type labels**: `exit`, `down`, `stop`, `kill`

**Reason labels**: `normal`, `shutdown`, `killed`, `panic`, `custom`

## Integration Examples

### Prometheus

Export metrics in Prometheus format:

```rust
use joerl::telemetry;
use metrics_exporter_prometheus::PrometheusBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up Prometheus exporter
    PrometheusBuilder::new()
        .with_http_listener(([127, 0, 0, 1], 9090))
        .install()?;
    
    telemetry::init();
    
    // Your actor system code
    let system = joerl::ActorSystem::new();
    
    // Metrics now available at http://127.0.0.1:9090/metrics
    
    Ok(())
}
```

### OpenTelemetry (OTLP)

Export to OpenTelemetry-compatible backends:

```rust
use joerl::telemetry;
use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up OpenTelemetry tracing
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317")
        )
        .install_batch(opentelemetry::runtime::Tokio)?;
    
    let telemetry_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer);
    
    tracing_subscriber::registry()
        .with(telemetry_layer)
        .init();
    
    telemetry::init();
    
    // Your actor system code
    
    Ok(())
}
```

### Grafana

Use Prometheus as a data source in Grafana:

1. Run the Prometheus exporter (see example above)
2. Configure Prometheus to scrape `http://localhost:9090/metrics`
3. Add Prometheus as a data source in Grafana
4. Create dashboards with PromQL queries

Example PromQL queries:

```promql
# Actor throughput (actors spawned per second)
rate(joerl_actors_spawned_total[1m])

# Actor throughput by type
rate(joerl_actors_spawned_total[1m]) by (type)

# Average message processing latency
rate(joerl_message_processing_duration_seconds_sum[1m]) / 
rate(joerl_message_processing_duration_seconds_count[1m])

# Current active actors
joerl_actors_active

# Active actors by type
joerl_actors_active by (type)

# Actor stop rate by type and reason
rate(joerl_actors_stopped_total[5m]) by (type, reason)

# Message send failure rate
rate(joerl_messages_sent_failed_total[1m])

# Supervisor restart rate by strategy
rate(joerl_supervisor_restarts_total[5m])

# Find actors with high panic rates
topk(5, rate(joerl_actors_panicked_total[5m]) by (type))

# Mailbox depth by actor type
joerl_mailbox_depth by (type)

# Actors with highest mailbox utilization
topk(5, joerl_mailbox_utilization_percent by (type))

# Mailbox backpressure events by actor type
rate(joerl_mailbox_full_total[5m]) by (type)

# GenServer average call latency by type
rate(joerl_gen_server_call_duration_seconds_sum[1m]) /
rate(joerl_gen_server_call_duration_seconds_count[1m]) by (type)

# GenServer call latency percentiles (p50, p95, p99)
histogram_quantile(0.50, rate(joerl_gen_server_call_duration_seconds_bucket[5m])) by (type)
histogram_quantile(0.95, rate(joerl_gen_server_call_duration_seconds_bucket[5m])) by (type)
histogram_quantile(0.99, rate(joerl_gen_server_call_duration_seconds_bucket[5m])) by (type)

# GenServer calls in progress
joerl_gen_server_calls_in_flight by (type)

# GenServer cast throughput
rate(joerl_gen_server_casts_total[1m]) by (type)

# GenStatem state transition rate
rate(joerl_gen_statem_transitions_total[1m]) by (type, from, to)

# GenStatem most common state transitions
topk(10, rate(joerl_gen_statem_transitions_total[5m]) by (type, from, to))

# GenStatem average time in each state
rate(joerl_gen_statem_state_duration_seconds_sum[5m]) /
rate(joerl_gen_statem_state_duration_seconds_count[5m]) by (type, state)

# GenStatem current state distribution
joerl_gen_statem_current_state by (type, state)

# GenStatem invalid transitions (potential bugs)
rate(joerl_gen_statem_invalid_transitions_total[5m]) by (type, state, event)

# Actor average lifetime by type
rate(joerl_actor_lifetime_seconds_sum[5m]) /
rate(joerl_actor_lifetime_seconds_count[5m]) by (type)

# Actor lifetime percentiles (p50, p95, p99)
histogram_quantile(0.50, rate(joerl_actor_lifetime_seconds_bucket[5m])) by (type)
histogram_quantile(0.95, rate(joerl_actor_lifetime_seconds_bucket[5m])) by (type)
histogram_quantile(0.99, rate(joerl_actor_lifetime_seconds_bucket[5m])) by (type)

# Short-lived actors (potential restart loops)
rate(joerl_short_lived_actors_total[5m]) by (type)

# Actors with highest short-lived rate (restart issues)
topk(5, rate(joerl_short_lived_actors_total[5m]) by (type))

# Signal sending rate by type
rate(joerl_signals_sent_total[1m]) by (type)

# Signal receiving rate by type
rate(joerl_signals_received_total[1m]) by (type)

# Ignored signal rate (trapped exits)
rate(joerl_signals_ignored_total[1m]) by (type)

# Exit signal breakdown by reason
rate(joerl_exit_signals_by_reason_total[5m]) by (reason)

# Most common exit reasons
topk(5, rate(joerl_exit_signals_by_reason_total[5m]) by (reason))

# Signal ignore ratio (trapped vs total)
rate(joerl_signals_ignored_total[5m]) /
(rate(joerl_signals_received_total[5m]) + 0.001)

# Message queue wait time (backlog)
rate(joerl_message_queue_wait_seconds_sum[1m]) /
rate(joerl_message_queue_wait_seconds_count[1m])

# Processing time vs queue wait time
rate(joerl_message_processing_duration_seconds_sum[1m]) /
rate(joerl_message_processing_duration_seconds_count[1m])

# Queue wait time percentiles
histogram_quantile(0.50, rate(joerl_message_queue_wait_seconds_bucket[5m]))
histogram_quantile(0.95, rate(joerl_message_queue_wait_seconds_bucket[5m]))
histogram_quantile(0.99, rate(joerl_message_queue_wait_seconds_bucket[5m]))

# Identify actors with high queue backlog (wait > processing)
# Compare queue wait vs processing duration to find backlog hotspots
```

### Datadog

Export to Datadog using the Datadog agent:

```rust
use metrics_exporter_statsd::StatsdBuilder;

fn main() {
    // Configure StatsD exporter for Datadog
    StatsdBuilder::from("127.0.0.1", 8125)
        .with_queue_size(1024)
        .install()
        .expect("Failed to install StatsD exporter");
    
    joerl::telemetry::init();
    
    // Your actor system code
}
```

## Best Practices

### 1. Enable Telemetry in Development and Production

```toml
[features]
default = ["telemetry"]
telemetry = ["joerl/telemetry"]
```

### 2. Set Up Alerts

Monitor critical metrics:

- **High restart rate**: `rate(joerl_supervisor_restarts_total[5m]) > 10`
- **Mailbox backpressure**: `joerl_mailbox_full_total` increasing
- **High panic rate**: `rate(joerl_actors_panicked_total[5m]) > 0`
- **Message send failures**: `rate(joerl_messages_sent_failed_total[1m]) > 100`
- **Restart loops**: `rate(joerl_short_lived_actors_total[5m]) > 5`
- **Abnormal exit signals**: `rate(joerl_exit_signals_by_reason_total{reason!="normal"}[5m]) > 10`

### 3. Use Dashboards

Create dashboards that show:

- Active actors over time
- Message throughput and latency
- Supervisor restart events
- Error rates and failure reasons

### 4. Distributed Tracing

For distributed actor systems, use OpenTelemetry to trace message flows across nodes:

```rust
use tracing::{info_span, Instrument};

async fn handle_distributed_message(&self, msg: Message) {
    let span = info_span!("handle_message", actor_id = ?self.id);
    
    async {
        // Message handling logic
    }
    .instrument(span)
    .await
}
```

### 5. Performance Considerations

- Telemetry has minimal overhead (~5-10ns per metric update)
- Use sampling for high-frequency operations if needed
- Metrics are lock-free and designed for concurrent access
- Consider aggregating metrics locally before export in high-throughput systems

#### Sampling Guidelines

**Measure first**: Profile your system to determine if sampling is needed. Most systems can handle 100% sampling.

**Sampling recommendations by throughput**:
- < 1,000 msgs/sec: No sampling needed (100%)
- 1,000-10,000 msgs/sec: Consider 50% sampling
- 10,000-100,000 msgs/sec: Use 10% sampling
- > 100,000 msgs/sec: Use 1-5% sampling

**What to sample**:
- ✅ High-frequency: message processing, queue wait times
- ❌ Low-frequency: actor lifecycle, supervisor restarts, signals
- ❌ Never sample: panic counters, error metrics

**Interpreting sampled metrics**:
- Counters: Multiply by (100 / sample_rate) for estimates
- Histograms: Percentiles remain accurate, counts are sampled
- Gauges: Not affected by sampling

**Example**: With 10% sampling and 1,000 sampled messages, estimate ~10,000 total messages

### 6. Custom Metrics

You can add your own application-specific metrics:

```rust
use metrics::{counter, histogram};

#[async_trait]
impl Actor for MyActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        counter!("my_app_custom_messages_total").increment(1);
        
        let start = std::time::Instant::now();
        // Handle message
        let duration = start.elapsed();
        
        histogram!("my_app_business_logic_duration_seconds")
            .record(duration.as_secs_f64());
    }
}
```

## Example Output

Here's what Prometheus metrics output looks like:

```
# HELP joerl_actors_spawned_total Total actors spawned
# TYPE joerl_actors_spawned_total counter
joerl_actors_spawned_total 150

# HELP joerl_actors_active Current number of active actors
# TYPE joerl_actors_active gauge
joerl_actors_active 47

# HELP joerl_messages_sent_total Total messages sent
# TYPE joerl_messages_sent_total counter
joerl_messages_sent_total 10543

# HELP joerl_message_processing_duration_seconds Message processing latency
# TYPE joerl_message_processing_duration_seconds histogram
joerl_message_processing_duration_seconds_bucket{le="0.001"} 8234
joerl_message_processing_duration_seconds_bucket{le="0.01"} 10123
joerl_message_processing_duration_seconds_bucket{le="0.1"} 10543
joerl_message_processing_duration_seconds_sum 45.234
joerl_message_processing_duration_seconds_count 10543

# HELP joerl_supervisor_restarts_total Total child restarts
# TYPE joerl_supervisor_restarts_total counter
joerl_supervisor_restarts_total{strategy="one_for_one"} 5
```

## Running the Example

Try the included telemetry demo:

```bash
cargo run --example telemetry_demo --features telemetry
```

Then visit `http://127.0.0.1:9090/metrics` to see live metrics.

## Troubleshooting

### Metrics Not Showing Up

1. Verify the `telemetry` feature is enabled
2. Ensure `telemetry::init()` is called
3. Check that your exporter is properly configured
4. Verify actors are actually spawning and processing messages

### High Overhead

- Reduce histogram bucket count in exporter configuration
- Use sampling for high-frequency operations
- Consider aggregating before export

### Missing Labels

Labels are automatically applied based on the operation context. If you need custom labels, you'll need to create your own metrics using the `metrics` crate directly.

## Health Checks

joerl provides a built-in health checking system that monitors the overall health of your actor system based on telemetry metrics.

### Basic Usage

```rust
use joerl::{ActorSystem, SystemHealth, HealthConfig};

let system = ActorSystem::new();
let health = SystemHealth::new(system.clone(), HealthConfig::default());

// Simple health check
if health.is_healthy() {
    println!("System is healthy");
} else {
    println!("System has issues: {}", health.get_report());
}
```

### Health Status Levels

The system reports one of three health states:

- **Healthy**: All metrics within acceptable thresholds
- **Degraded**: Some issues detected (warnings) but system is operational
- **Unhealthy**: Critical issues detected requiring immediate attention

### Configurable Thresholds

```rust
use joerl::{SystemHealth, HealthConfig};

let config = HealthConfig {
    max_actors: 10000,          // Alert if actor count exceeds this
    min_actors: 0,              // Alert if actor count below this
    max_panic_rate: 0.01,       // Alert if >1% of actors panic
    max_restart_rate: 0.05,     // Alert if >5% restart rate
    max_backpressure_rate: 0.1, // Alert if >10% backpressure
};

let health = SystemHealth::new(system.clone(), config);
```

### Health Check Methods

```rust
// Simple boolean check
let healthy: bool = health.is_healthy();

// Readiness probe (for Kubernetes)
let ready: bool = health.is_ready();

// Get detailed status
use joerl::HealthStatus;
let status: HealthStatus = health.get_status();
match status {
    HealthStatus::Healthy => println!("✓ All systems operational"),
    HealthStatus::Degraded => println!("⚠ Some issues detected"),
    HealthStatus::Unhealthy => println!("✗ Critical issues"),
}

// Get structured issue list
let issues = health.get_issues();
for issue in issues {
    println!("[{:?}] {}: {}", issue.severity, issue.title, issue.message);
}

// Get formatted report
let report = health.get_report();
println!("{}", report);
```

### Kubernetes Integration

Use health checks for liveness and readiness probes:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: joerl-app
spec:
  containers:
  - name: app
    image: my-joerl-app:latest
    livenessProbe:
      httpGet:
        path: /health
        port: 8080
      initialDelaySeconds: 10
      periodSeconds: 30
    readinessProbe:
      httpGet:
        path: /ready
        port: 8080
      initialDelaySeconds: 5
      periodSeconds: 10
```

**Note**: HTTP endpoint implementation is left as a future enhancement. For now, you can implement health check endpoints in your application using the `is_healthy()` and `is_ready()` methods with frameworks like `axum` or `warp`.

### Health Check Metrics

The health checker evaluates:

- **Actor Count**: Too many or too few actors
- **Panic Rate**: `actors_panicked / actors_spawned`
- **Restart Rate**: `supervisor_restarts / actors_stopped`
- **Backpressure Rate**: `mailbox_full / messages_sent`

### Example Health Report

```
Status: Degraded
Issues:
  [Warning] High actor count: 8523 active actors (threshold: 5000)
  [Warning] High backpressure: 15.3% of messages encountering full mailboxes
```

## Further Reading

- [Prometheus Best Practices](https://prometheus.io/docs/practices/)
- [OpenTelemetry Documentation](https://opentelemetry.io/docs/)
- [Grafana Dashboard Examples](https://grafana.com/grafana/dashboards/)
- [Metrics Crate Documentation](https://docs.rs/metrics/)
