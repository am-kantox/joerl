//! Comprehensive supervision tree example.
//!
//! Demonstrates all three supervisor restart strategies:
//! - OneForOne: Only restart the failed child
//! - OneForAll: Restart all children when one fails
//! - RestForOne: Restart the failed child and all started after it
//!
//! This example creates a supervision tree with worker actors that can
//! be instructed to fail, demonstrating how supervisors handle restarts.

use async_trait::async_trait;
use joerl::{
    Actor, ActorContext, ActorSystem, ExitReason, Message,
    supervisor::{ChildSpec, RestartIntensity, RestartStrategy, SupervisorSpec, spawn_supervisor},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{Duration, sleep};

/// A worker actor that can be instructed to fail.
struct Worker {
    name: String,
    restart_count: Arc<AtomicUsize>,
}

impl Worker {
    fn new(name: impl Into<String>, restart_count: Arc<AtomicUsize>) -> Self {
        Self {
            name: name.into(),
            restart_count,
        }
    }
}

#[async_trait]
impl Actor for Worker {
    async fn started(&mut self, ctx: &mut ActorContext) {
        let count = self.restart_count.fetch_add(1, Ordering::SeqCst);
        println!(
            "✓ Worker '{}' [{}] started (restart #{})",
            self.name,
            ctx.pid(),
            count
        );
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "fail" => {
                    println!("✗ Worker '{}' [{}] failing...", self.name, ctx.pid());
                    ctx.stop(ExitReason::Panic("intentional failure".to_string()));
                }
                "ping" => {
                    println!("→ Worker '{}' [{}] received ping", self.name, ctx.pid());
                }
                _ => {}
            }
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, ctx: &mut ActorContext) {
        println!(
            "✗ Worker '{}' [{}] stopped: {}",
            self.name,
            ctx.pid(),
            reason
        );
    }
}

/// Demonstrates OneForOne strategy: only failed child restarts.
async fn demo_one_for_one(system: Arc<ActorSystem>) {
    println!("\n=== OneForOne Strategy ===");
    println!("Only the failed child will be restarted.\n");

    let worker_a_count = Arc::new(AtomicUsize::new(0));
    let worker_b_count = Arc::new(AtomicUsize::new(0));
    let worker_c_count = Arc::new(AtomicUsize::new(0));

    // Create supervisor spec with OneForOne strategy
    let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
        .child(ChildSpec::new("worker_a", {
            let count = Arc::clone(&worker_a_count);
            move || Box::new(Worker::new("A", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("worker_b", {
            let count = Arc::clone(&worker_b_count);
            move || Box::new(Worker::new("B", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("worker_c", {
            let count = Arc::clone(&worker_c_count);
            move || Box::new(Worker::new("C", Arc::clone(&count)))
        }));

    let _supervisor = spawn_supervisor(&system, spec);
    sleep(Duration::from_millis(100)).await;

    println!("\nInitial state:");
    println!(
        "  Worker A restarts: {}",
        worker_a_count.load(Ordering::SeqCst)
    );
    println!(
        "  Worker B restarts: {}",
        worker_b_count.load(Ordering::SeqCst)
    );
    println!(
        "  Worker C restarts: {}",
        worker_c_count.load(Ordering::SeqCst)
    );

    // Note: Current implementation doesn't expose children PIDs, so we can't send messages
    // This is a known limitation that would need API extension
    println!("\n(Note: OneForOne restart would only affect Worker B if it failed)");

    sleep(Duration::from_millis(200)).await;
}

/// Demonstrates OneForAll strategy: all children restart when one fails.
async fn demo_one_for_all(system: Arc<ActorSystem>) {
    println!("\n\n=== OneForAll Strategy ===");
    println!("All children will be restarted when one fails.\n");

    let worker_a_count = Arc::new(AtomicUsize::new(0));
    let worker_b_count = Arc::new(AtomicUsize::new(0));
    let worker_c_count = Arc::new(AtomicUsize::new(0));

    // Create supervisor spec with OneForAll strategy
    let spec = SupervisorSpec::new(RestartStrategy::OneForAll)
        .child(ChildSpec::new("worker_a", {
            let count = Arc::clone(&worker_a_count);
            move || Box::new(Worker::new("A", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("worker_b", {
            let count = Arc::clone(&worker_b_count);
            move || Box::new(Worker::new("B", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("worker_c", {
            let count = Arc::clone(&worker_c_count);
            move || Box::new(Worker::new("C", Arc::clone(&count)))
        }));

    let _supervisor = spawn_supervisor(&system, spec);
    sleep(Duration::from_millis(100)).await;

    println!("\nInitial state:");
    println!(
        "  Worker A restarts: {}",
        worker_a_count.load(Ordering::SeqCst)
    );
    println!(
        "  Worker B restarts: {}",
        worker_b_count.load(Ordering::SeqCst)
    );
    println!(
        "  Worker C restarts: {}",
        worker_c_count.load(Ordering::SeqCst)
    );

    println!("\n(Note: OneForAll restart would affect ALL workers if any failed)");

    sleep(Duration::from_millis(200)).await;
}

/// Demonstrates RestForOne strategy: failed child and all started after restart.
async fn demo_rest_for_one(system: Arc<ActorSystem>) {
    println!("\n\n=== RestForOne Strategy ===");
    println!("Failed child and all children started after it will be restarted.\n");

    let worker_a_count = Arc::new(AtomicUsize::new(0));
    let worker_b_count = Arc::new(AtomicUsize::new(0));
    let worker_c_count = Arc::new(AtomicUsize::new(0));

    // Create supervisor spec with RestForOne strategy
    let spec = SupervisorSpec::new(RestartStrategy::RestForOne)
        .child(ChildSpec::new("worker_a", {
            let count = Arc::clone(&worker_a_count);
            move || Box::new(Worker::new("A", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("worker_b", {
            let count = Arc::clone(&worker_b_count);
            move || Box::new(Worker::new("B", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("worker_c", {
            let count = Arc::clone(&worker_c_count);
            move || Box::new(Worker::new("C", Arc::clone(&count)))
        }));

    let _supervisor = spawn_supervisor(&system, spec);
    sleep(Duration::from_millis(100)).await;

    println!("\nInitial state:");
    println!(
        "  Worker A restarts: {}",
        worker_a_count.load(Ordering::SeqCst)
    );
    println!(
        "  Worker B restarts: {}",
        worker_b_count.load(Ordering::SeqCst)
    );
    println!(
        "  Worker C restarts: {}",
        worker_c_count.load(Ordering::SeqCst)
    );

    println!("\n(Note: If Worker B failed, both B and C would restart, but not A)");

    sleep(Duration::from_millis(200)).await;
}

/// Demonstrates restart intensity limits.
async fn demo_restart_intensity(system: Arc<ActorSystem>) {
    println!("\n\n=== Restart Intensity Limits ===");
    println!("Supervisor stops if restart intensity is exceeded.\n");

    let worker_count = Arc::new(AtomicUsize::new(0));

    // Create supervisor with strict restart intensity
    let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
        .intensity(RestartIntensity {
            max_restarts: 2,
            within_seconds: 5,
        })
        .child(ChildSpec::new("flaky_worker", {
            let count = Arc::clone(&worker_count);
            move || Box::new(Worker::new("Flaky", Arc::clone(&count)))
        }));

    let _supervisor = spawn_supervisor(&system, spec);
    sleep(Duration::from_millis(100)).await;

    println!("Restart intensity: max 2 restarts within 5 seconds");
    println!("Worker starts: {}", worker_count.load(Ordering::SeqCst));
    println!("\n(Note: If worker fails 3 times quickly, supervisor will stop)");

    sleep(Duration::from_millis(200)).await;
}

/// Demonstrates nested supervision trees.
async fn demo_nested_supervisors(system: Arc<ActorSystem>) {
    println!("\n\n=== Nested Supervision Trees ===");
    println!("Supervisors can supervise other supervisors.\n");

    // Create a child supervisor
    let child_spec = SupervisorSpec::new(RestartStrategy::OneForOne)
        .child(ChildSpec::new("worker_1", || {
            Box::new(Worker::new("Worker-1", Arc::new(AtomicUsize::new(0))))
        }))
        .child(ChildSpec::new("worker_2", || {
            Box::new(Worker::new("Worker-2", Arc::new(AtomicUsize::new(0))))
        }));

    let _child_supervisor = spawn_supervisor(&system, child_spec);

    println!("Created nested supervisor with 2 workers");
    println!("(In production, top-level supervisor would supervise child supervisors)");

    sleep(Duration::from_millis(100)).await;
}

/// Demonstrates a realistic application supervision tree.
async fn demo_application_tree(system: Arc<ActorSystem>) {
    println!("\n\n=== Application Supervision Tree ===");
    println!("Realistic example: Database pool + API workers + Background jobs\n");

    // Simulated application components
    let db_restarts = Arc::new(AtomicUsize::new(0));
    let api_restarts = Arc::new(AtomicUsize::new(0));
    let job_restarts = Arc::new(AtomicUsize::new(0));

    let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
        .intensity(RestartIntensity {
            max_restarts: 5,
            within_seconds: 10,
        })
        .child(ChildSpec::new("database_pool", {
            let count = Arc::clone(&db_restarts);
            move || Box::new(Worker::new("DatabasePool", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("api_worker_1", {
            let count = Arc::clone(&api_restarts);
            move || Box::new(Worker::new("APIWorker-1", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("api_worker_2", {
            let count = Arc::clone(&api_restarts);
            move || Box::new(Worker::new("APIWorker-2", Arc::clone(&count)))
        }))
        .child(ChildSpec::new("background_job", {
            let count = Arc::clone(&job_restarts);
            move || Box::new(Worker::new("BackgroundJob", Arc::clone(&count)))
        }));

    let _supervisor = spawn_supervisor(&system, spec);
    sleep(Duration::from_millis(100)).await;

    println!("Application components started:");
    println!("  ✓ Database pool");
    println!("  ✓ API Worker 1");
    println!("  ✓ API Worker 2");
    println!("  ✓ Background job processor");
    println!("\nWith OneForOne strategy, each component can fail and restart independently.");

    sleep(Duration::from_millis(200)).await;
}

#[tokio::main]
async fn main() {
    // Initialize tracing for better visibility
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║        Supervision Tree Examples - joerl                ║");
    println!("╚══════════════════════════════════════════════════════════╝");

    let system = Arc::new(ActorSystem::new());

    // Run all demonstrations
    demo_one_for_one(Arc::clone(&system)).await;
    demo_one_for_all(Arc::clone(&system)).await;
    demo_rest_for_one(Arc::clone(&system)).await;
    demo_restart_intensity(Arc::clone(&system)).await;
    demo_nested_supervisors(Arc::clone(&system)).await;
    demo_application_tree(Arc::clone(&system)).await;

    println!("\n\n╔══════════════════════════════════════════════════════════╗");
    println!("║                 Examples Completed                       ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");
}
