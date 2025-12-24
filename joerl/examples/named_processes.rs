//! Named processes example.
//!
//! Demonstrates Erlang-style process registration using names.
//! Shows how to register actors with names, look them up, and use
//! them in a practical worker pool scenario.

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, Message};
use tokio::time::{Duration, sleep};

/// A worker actor that processes tasks.
#[derive(Clone)]
struct Worker {
    id: usize,
}

#[async_trait]
impl Actor for Worker {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[Worker {}] Started with Pid: {}", self.id, ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(task) = msg.downcast_ref::<String>() {
            println!("[Worker {}] Processing task: {}", self.id, task);
            sleep(Duration::from_millis(100)).await;
            println!("[Worker {}] Task completed: {}", self.id, task);

            // Notify manager (if registered)
            if let Some(manager_pid) = ctx.whereis("manager") {
                let result = format!("Worker {} completed: {}", self.id, task);
                ctx.send(manager_pid, Box::new(result)).await.ok();
            }
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, _ctx: &mut ActorContext) {
        println!("[Worker {}] Stopped: {}", self.id, reason);
    }
}

/// A manager actor that coordinates workers.
struct Manager {
    tasks_completed: usize,
}

#[async_trait]
impl Actor for Manager {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[Manager] Started with Pid: {}", ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(result) = msg.downcast_ref::<String>() {
            self.tasks_completed += 1;
            println!("[Manager] Received result: {}", result);
            println!("[Manager] Total tasks completed: {}", self.tasks_completed);
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, _ctx: &mut ActorContext) {
        println!(
            "[Manager] Stopped: {} (Total tasks: {})",
            reason, self.tasks_completed
        );
    }
}

#[tokio::main]
async fn main() {
    println!("=== Named Processes Example ===\n");

    let system = ActorSystem::new();

    // Spawn and register a manager
    let manager = system.spawn(Manager { tasks_completed: 0 });
    system.register("manager", manager.pid()).unwrap();
    println!("✓ Registered manager with name 'manager'");

    // Spawn and register workers with names
    let mut workers = Vec::new();
    for i in 1..=3 {
        let worker = system.spawn(Worker { id: i });
        let name = format!("worker{}", i);
        system.register(&name, worker.pid()).unwrap();
        println!("✓ Registered {} with name '{}'", worker.pid(), name);
        workers.push(worker);
    }

    println!("\n--- Registered Names ---");
    let registered = system.registered();
    println!("All registered names: {:?}\n", registered);

    // Send tasks to workers by name
    println!("--- Sending Tasks ---");
    for i in 1..=3 {
        let worker_name = format!("worker{}", i);
        if let Some(worker_pid) = system.whereis(&worker_name) {
            let task = format!("Task-{}", i);
            println!("Sending {} to {}", task, worker_name);
            system.send(worker_pid, Box::new(task)).await.unwrap();
        }
    }

    // Wait for tasks to complete
    sleep(Duration::from_millis(500)).await;

    // Demonstrate looking up by name
    println!("\n--- Name Lookups ---");
    if let Some(pid) = system.whereis("manager") {
        println!("Manager Pid: {}", pid);
    }

    if let Some(pid) = system.whereis("worker1") {
        println!("Worker1 Pid: {}", pid);
    }

    // Demonstrate unregistering
    println!("\n--- Unregistering ---");
    system.unregister("worker3").unwrap();
    println!("✓ Unregistered 'worker3'");

    // Verify it's gone
    match system.whereis("worker3") {
        Some(_) => println!("✗ worker3 still registered (unexpected)"),
        None => println!("✓ worker3 no longer registered"),
    }

    println!("\n--- Final Registered Names ---");
    let final_registered = system.registered();
    println!("Remaining registered names: {:?}", final_registered);

    // Send one more task using the manager directly
    println!("\n--- Direct Manager Communication ---");
    if let Some(manager_pid) = system.whereis("manager") {
        system
            .send(manager_pid, Box::new("Direct message".to_string()))
            .await
            .unwrap();
    }

    // Clean shutdown
    sleep(Duration::from_millis(200)).await;
    println!("\n=== Example Complete ===");
}
