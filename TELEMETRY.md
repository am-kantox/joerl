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

## Available Metrics

joerl automatically tracks the following metrics when telemetry is enabled:

### Actor Lifecycle

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_actors_spawned_total` | Counter | Total actors spawned | `type` |
| `joerl_actors_stopped_total` | Counter | Total actors stopped | `type`, `reason` |
| `joerl_actors_active` | Gauge | Current number of active actors | `type` |
| `joerl_actors_panicked_total` | Counter | Total actors that panicked | `type` |

**Type labels**: Actor struct name (e.g., `"Worker"`, `"Supervisor"`, `"boxed"` for dynamically spawned actors)

**Reason labels**: `normal`, `shutdown`, `killed`, `panic`, `custom`

### Message Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_messages_sent_total` | Counter | Total messages sent successfully | - |
| `joerl_messages_sent_failed_total` | Counter | Failed message send attempts | `reason` |
| `joerl_messages_processed_total` | Counter | Total messages processed | - |
| `joerl_message_processing_duration_seconds` | Histogram | Message processing latency | - |

**Failure reason labels**: `mailbox_full`, `actor_not_found`

### Mailbox Operations

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `joerl_mailbox_depth` | Gauge | Current mailbox queue depth | - |
| `joerl_mailbox_full_total` | Counter | Times mailbox was full | - |

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

## Further Reading

- [Prometheus Best Practices](https://prometheus.io/docs/practices/)
- [OpenTelemetry Documentation](https://opentelemetry.io/docs/)
- [Grafana Dashboard Examples](https://grafana.com/grafana/dashboards/)
- [Metrics Crate Documentation](https://docs.rs/metrics/)
