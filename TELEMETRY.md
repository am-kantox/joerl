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

### Distributed System Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_remote_messages_sent_total` | Counter | Remote messages sent successfully | `target_node` |
| `joerl_remote_messages_failed_total` | Counter | Failed remote message sends | `target_node`, `reason` |
| `joerl_node_connections_active` | Gauge | Current active node connections | - |
| `joerl_node_connection_established_total` | Counter | Node connections established | `node` |
| `joerl_node_connection_lost_total` | Counter | Node connections lost | `node` |
| `joerl_network_latency_seconds` | Histogram | Network operation latency | `node` |
| `joerl_serialization_errors_total` | Counter | Message serialization errors | - |

**Node labels**: Remote node names (e.g., `"worker1"`, `"node_b"`)

**Failure reason labels**: `reconnect_failed`, `write_failed`, `flush_failed`, `connection_failed`, `serialization_error`

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

# Remote message throughput by node
rate(joerl_remote_messages_sent_total[1m]) by (target_node)

# Remote message failure rate by node and reason
rate(joerl_remote_messages_failed_total[1m]) by (target_node, reason)

# Network latency percentiles by node
histogram_quantile(0.50, rate(joerl_network_latency_seconds_bucket[5m])) by (node)
histogram_quantile(0.95, rate(joerl_network_latency_seconds_bucket[5m])) by (node)
histogram_quantile(0.99, rate(joerl_network_latency_seconds_bucket[5m])) by (node)

# Current active node connections
joerl_node_connections_active

# Node connection churn (establishes + losses)
rate(joerl_node_connection_established_total[5m]) + 
rate(joerl_node_connection_lost_total[5m])

# Nodes with most connection issues
topk(5, rate(joerl_node_connection_lost_total[5m]) by (node))

# Message serialization error rate
rate(joerl_serialization_errors_total[1m])

# Remote message success rate
(rate(joerl_remote_messages_sent_total[1m]) / 
 (rate(joerl_remote_messages_sent_total[1m]) + 
  rate(joerl_remote_messages_failed_total[1m]) + 0.001)) * 100

# Average network latency across all nodes
avg(rate(joerl_network_latency_seconds_sum[1m]) / 
    rate(joerl_network_latency_seconds_count[1m]))

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
- **Remote message failures**: `rate(joerl_remote_messages_failed_total[1m]) > 50`
- **Node connection losses**: `rate(joerl_node_connection_lost_total[5m]) > 1`
- **High network latency**: `histogram_quantile(0.95, rate(joerl_network_latency_seconds_bucket[5m])) > 0.5`
- **Serialization errors**: `rate(joerl_serialization_errors_total[1m]) > 0`
- **Node disconnected**: `joerl_node_connections_active < expected_count`

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

## Distributed Tracing

joerl supports distributed tracing using OpenTelemetry spans. When the `telemetry` feature is enabled, you can trace message flow across actor boundaries and visualize the complete request lifecycle.

### Overview

Distributed tracing provides:
- **Request Flow Visualization**: See the complete path of a request through multiple actors
- **Latency Analysis**: Identify bottlenecks and slow operations
- **Causality Tracking**: Understand parent-child relationships between operations
- **Error Attribution**: Quickly find the source of failures

### Built-in Span Methods

joerl provides span creation methods for key operations:

```rust
use joerl::telemetry::{ActorMetrics, MessageMetrics};
use tracing::Instrument;

// Actor lifecycle spans
let span = ActorMetrics::actor_spawn_span("MyActor", &pid.to_string());
let _guard = span.enter();

// Actor lifecycle events
let span = ActorMetrics::actor_lifecycle_span("started", "MyActor", &pid.to_string());

// Message send/receive spans
let span = MessageMetrics::message_send_span(&from_pid, &to_pid);
let span = MessageMetrics::message_receive_span(&to_pid, parent_span_id);
```

### Instrumenting Your Actors

Wrap actor methods with spans to trace execution:

```rust
use joerl::{Actor, ActorContext, Message};
use async_trait::async_trait;
use tracing::{debug_span, Instrument};

struct MyActor;

#[async_trait]
impl Actor for MyActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        let span = debug_span!(
            "actor.started",
            actor.type = "MyActor",
            actor.pid = %ctx.pid()
        );
        
        async {
            // Your initialization code
            println!("Actor started");
        }.instrument(span).await;
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        let span = debug_span!(
            "actor.handle_message",
            actor.type = "MyActor",
            actor.pid = %ctx.pid()
        );
        
        async {
            if let Some(data) = msg.downcast_ref::<String>() {
                // Process message
                println!("Received: {}", data);
            }
        }.instrument(span).await;
    }
}
```

### Span Context Propagation

joerl automatically captures and propagates span context through messages:

1. **Message Send**: Current span ID is captured when a message is created
2. **Message Receive**: Span ID is available in the envelope for linking spans
3. **Span Linking**: Child spans can reference parent span IDs for causality

This creates a distributed trace showing message flow:

```
actor.spawn (ActorA) -> message.send -> message.receive -> actor.handle_message (ActorB)
```

### Setting Up Tracing Backend

#### Console Output (Development)

```rust
use tracing_subscriber;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    joerl::telemetry::init();
    // Your code
}
```

#### Jaeger (Production)

```rust
use opentelemetry::global;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up OpenTelemetry tracer
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317"),
        )
        .install_batch(Tokio)?;
    
    // Create tracing subscriber with OpenTelemetry layer
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = Registry::default().with(telemetry);
    tracing::subscriber::set_global_default(subscriber)?;
    
    joerl::telemetry::init();
    
    // Your actor system code
    let system = joerl::ActorSystem::new();
    // ...
    
    // Shutdown tracer on exit
    global::shutdown_tracer_provider();
    Ok(())
}
```

#### Running Jaeger Locally

```bash
# Start Jaeger all-in-one
docker run -d --name jaeger \
  -e COLLECTOR_OTLP_ENABLED=true \
  -p 16686:16686 \
  -p 4317:4317 \
  jaegertracing/all-in-one:latest

# View traces at http://localhost:16686
```

### Span Attributes

joerl spans include standard OpenTelemetry semantic conventions:

**Actor Spans**:
- `actor.type`: Actor struct name
- `actor.pid`: Process ID
- `actor.event`: Lifecycle event (spawn, started, stopped)
- `otel.kind`: "internal"

**Messaging Spans**:
- `messaging.operation`: "send" or "receive"
- `messaging.system`: "joerl"
- `actor.from_pid`: Sender PID
- `actor.to_pid`: Receiver PID
- `parent.span.id`: Parent span ID for linking
- `otel.kind`: "producer" or "consumer"

### Best Practices

1. **Sampling**: Use sampling for high-throughput systems to reduce overhead
2. **Span Granularity**: Balance detail with performance - trace critical paths
3. **Error Recording**: Use `span.record_error()` to capture failures
4. **Context Propagation**: Let joerl handle span context automatically
5. **Backend Configuration**: Configure appropriate retention and sampling in your tracing backend

### Performance Impact

OpenTelemetry span creation has minimal overhead (~100-500ns per span) when:
- Using batch exporters (not synchronous)
- Applying appropriate sampling rates  
- Avoiding excessive span attributes

For systems processing >100K messages/sec, consider:
- Sampling spans at 1-10%
- Using tail-based sampling in your collector
- Reducing span attribute cardinality

## Memory Tracking

joerl provides limited memory tracking capabilities. Rust does not offer built-in per-actor memory profiling, so the available metrics focus on system-level and estimated measurements.

### Available Memory Metrics

| Metric | Type | Description | Platform |
|--------|------|-------------|----------|
| `joerl_system_memory_bytes` | Gauge | Process memory usage (RSS) | Linux only |
| `joerl_mailbox_memory_bytes{type}` | Gauge | Estimated mailbox memory | All |
| `joerl_mailbox_memory_total_bytes` | Gauge | Total estimated mailbox memory | All |

### System Memory Tracking

On Linux, joerl can track process-level memory usage:

```rust
use joerl::telemetry::MemoryMetrics;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() {
    joerl::telemetry::init();
    
    // Update system memory metrics periodically
    tokio::spawn(async {
        let mut ticker = interval(Duration::from_secs(5));
        loop {
            ticker.tick().await;
            MemoryMetrics::update_system_memory();
        }
    });
    
    // Your actor system
}
```

### Mailbox Memory Estimation

Estimate mailbox memory based on queue depth:

```rust
use joerl::telemetry::MemoryMetrics;

// Estimate mailbox memory for a specific actor type
let actor_type = "Worker";
let mailbox_depth = 1000;
let avg_message_size = 128; // bytes (rough estimate)

MemoryMetrics::mailbox_memory_typed(actor_type, mailbox_depth, avg_message_size);
```

### Limitations

**Why is per-actor memory tracking limited?**

1. **No Built-in Support**: Rust doesn't provide APIs to measure per-object memory
2. **Heap Allocations**: Messages use `Box<dyn Any>` with varying sizes
3. **Stack vs Heap**: Actor state can be on stack or heap, hard to track
4. **Shared References**: `Arc` and shared data make attribution complex

### Recommended Tools

For detailed memory profiling, use external tools:

**Development**:
- **dhat-rs**: Heap profiling for Rust ([dhat-rs](https://github.com/nnethercote/dhat-rs))
- **Valgrind**: Memory debugging and profiling
- **heaptrack**: Heap memory profiler

**Production**:
- **jemalloc** with profiling enabled
- Prometheus `process_resident_memory_bytes` (from process exporters)
- Operating system monitoring (htop, ps, /proc)

### Example: Using dhat-rs

```rust
use dhat::{Dhat, DhatAlloc};

#[global_allocator]
static ALLOCATOR: DhatAlloc = DhatAlloc;

fn main() {
    let _dhat = Dhat::start_heap_profiling();
    
    // Your actor system code
    let system = joerl::ActorSystem::new();
    // ...
    
    // Profiling output written on drop
}
```

## Custom Telemetry Providers

joerl allows you to integrate custom telemetry backends by implementing the `TelemetryProvider` trait.

### TelemetryProvider Trait

Implement this trait to receive callbacks at key actor system events:

```rust
use joerl::telemetry::TelemetryProvider;

struct MyTelemetryProvider {
    // Your custom state
}

impl TelemetryProvider for MyTelemetryProvider {
    fn on_actor_spawned(&self, actor_type: &str, pid: &str) {
        // Called when an actor is spawned
        println!("[TELEMETRY] Spawned {} with PID {}", actor_type, pid);
    }
    
    fn on_actor_stopped(&self, actor_type: &str, pid: &str, reason: &str) {
        // Called when an actor stops
        println!("[TELEMETRY] Stopped {} ({}): {}", actor_type, pid, reason);
    }
    
    fn on_actor_panicked(&self, actor_type: &str, pid: &str, error: &str) {
        // Called when an actor panics
        eprintln!("[TELEMETRY] PANIC in {} ({}): {}", actor_type, pid, error);
    }
    
    fn on_message_sent(&self, from_pid: &str, to_pid: &str) {
        // Called when a message is sent
    }
    
    fn on_message_received(&self, pid: &str) {
        // Called when a message is received
    }
    
    fn on_supervisor_restart(&self, child_type: &str, strategy: &str) {
        // Called when a supervisor restarts a child
        println!("[TELEMETRY] Restarted {} ({})", child_type, strategy);
    }
}
```

### Registering a Provider

Register your provider before creating the actor system:

```rust
use joerl::telemetry::set_telemetry_provider;

fn main() {
    // Register custom provider
    set_telemetry_provider(Box::new(MyTelemetryProvider {}));
    
    joerl::telemetry::init();
    
    // Create actor system
    let system = joerl::ActorSystem::new();
    // Provider hooks will be called automatically
}
```

### Use Cases

**Custom Metrics Backend**:
```rust
impl TelemetryProvider for StatsdProvider {
    fn on_actor_spawned(&self, actor_type: &str, _pid: &str) {
        self.client.incr(&format!("actors.spawned.{}", actor_type));
    }
}
```

**Application Logging**:
```rust
impl TelemetryProvider for LoggingProvider {
    fn on_actor_panicked(&self, actor_type: &str, pid: &str, error: &str) {
        log::error!(
            target: "actor_panic",
            "Actor panic: type={}, pid={}, error={}",
            actor_type, pid, error
        );
    }
}
```

**Debugging and Auditing**:
```rust
impl TelemetryProvider for AuditProvider {
    fn on_actor_spawned(&self, actor_type: &str, pid: &str) {
        self.audit_log.record(AuditEvent::ActorSpawned {
            timestamp: Utc::now(),
            actor_type: actor_type.to_string(),
            pid: pid.to_string(),
        });
    }
}
```

### Notes

- Provider can only be set once (returns warning if called again)
- All trait methods have default no-op implementations
- Provider must be `Send + Sync` for thread safety
- Hooks are called synchronously - keep them fast to avoid blocking
- For async operations, spawn tasks rather than awaiting in hooks

## Further Reading

- [Prometheus Best Practices](https://prometheus.io/docs/practices/)
- [OpenTelemetry Documentation](https://opentelemetry.io/docs/)
- [Grafana Dashboard Examples](https://grafana.com/grafana/dashboards/)
- [Metrics Crate Documentation](https://docs.rs/metrics/)
