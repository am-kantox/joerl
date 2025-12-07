//! Telemetry demonstration with Prometheus metrics export.
//!
//! This example demonstrates how to:
//! - Enable telemetry with the `telemetry` feature
//! - Export metrics to Prometheus
//! - Observe actor lifecycle, messages, and supervisor events
//!
//! ## Running
//!
//! ```bash
//! cargo run --example telemetry_demo --features telemetry
//! ```
//!
//! Then open http://localhost:9090/metrics in your browser to see the metrics.

use async_trait::async_trait;
use joerl::{
    Actor, ActorContext, ActorSystem, ExitReason, Message,
    supervisor::{ChildSpec, RestartStrategy, SupervisorSpec, spawn_supervisor},
};
use std::sync::Arc;
use std::time::Duration;

/// A worker that may fail periodically.
struct Worker {
    id: String,
    fail_count: usize,
}

#[async_trait]
impl Actor for Worker {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("Worker {} started with PID {}", self.id, ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "work" => {
                    println!("Worker {} processing work", self.id);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                "fail" => {
                    self.fail_count += 1;
                    println!("Worker {} failing (count: {})", self.id, self.fail_count);
                    ctx.stop(ExitReason::Custom("intentional failure".to_string()));
                }
                _ => {}
            }
        }
    }

    async fn stopped(&mut self, _reason: &ExitReason, _ctx: &mut ActorContext) {
        println!("Worker {} stopped: {}", self.id, _reason);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== joerl Telemetry Demo ===\n");

    #[cfg(feature = "telemetry")]
    {
        // Install Prometheus exporter
        println!("Starting Prometheus metrics exporter on 127.0.0.1:9090");
        let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
        builder
            .with_http_listener(([127, 0, 0, 1], 9090))
            .install()
            .expect("Failed to install Prometheus exporter");

        println!("Metrics available at: http://127.0.0.1:9090/metrics\n");

        // Initialize telemetry
        joerl::telemetry::init();
    }

    #[cfg(not(feature = "telemetry"))]
    {
        println!("Telemetry feature is not enabled!");
        println!("Run with: cargo run --example telemetry_demo --features telemetry\n");
    }

    let system = Arc::new(ActorSystem::new());

    println!("Creating supervision tree with 3 workers...");

    // Create a supervisor with three workers
    let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
        .child(ChildSpec::new("worker_1", || {
            Box::new(Worker {
                id: "worker_1".to_string(),
                fail_count: 0,
            })
        }))
        .child(ChildSpec::new("worker_2", || {
            Box::new(Worker {
                id: "worker_2".to_string(),
                fail_count: 0,
            })
        }))
        .child(ChildSpec::new("worker_3", || {
            Box::new(Worker {
                id: "worker_3".to_string(),
                fail_count: 0,
            })
        }));

    let supervisor = spawn_supervisor(&system, spec);
    println!("Supervisor started with PID: {}\n", supervisor.pid());

    // Give supervisors time to start children
    tokio::time::sleep(Duration::from_millis(100)).await;

    println!("Spawning additional actors to demonstrate metrics...");

    // Spawn some standalone workers
    let mut workers = Vec::new();
    for i in 1..=5 {
        let worker = system.spawn(Worker {
            id: format!("standalone_{}", i),
            fail_count: 0,
        });
        workers.push(worker);
    }

    println!("Sending messages to generate telemetry data...");

    // Send some work messages
    for i in 0..10 {
        for worker in &workers {
            let _ = worker.send(Box::new("work")).await;
        }

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Trigger some failures to demonstrate supervisor restart metrics
        if i % 3 == 0 && !workers.is_empty() {
            println!("Triggering worker failure...");
            let _ = workers[0].send(Box::new("fail")).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    println!("\nDemo statistics:");
    println!(
        "- Actors spawned and stopped: check joerl_actors_spawned_total and joerl_actors_stopped_total"
    );
    println!("- Messages sent: check joerl_messages_sent_total");
    println!("- Messages processed: check joerl_messages_processed_total");
    println!("- Supervisor restarts: check joerl_supervisor_restarts_total");

    #[cfg(feature = "telemetry")]
    {
        println!("\nKeeping server alive for 30 seconds...");
        println!("Visit http://127.0.0.1:9090/metrics to see metrics");
        tokio::time::sleep(Duration::from_secs(30)).await;
    }

    #[cfg(not(feature = "telemetry"))]
    {
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    println!("\nShutting down...");
    Ok(())
}
