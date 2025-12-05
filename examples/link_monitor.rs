//! Links and monitors demonstration.
//!
//! This example demonstrates the difference between links and monitors:
//! - **Links** are bidirectional: if one actor dies, the linked actor dies too
//!   (unless it's trapping exits)
//! - **Monitors** are unidirectional: the monitoring actor receives a DOWN signal
//!   when the monitored actor dies, but doesn't die itself

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, ExitReason, Message, MonitorRef, Signal};
use std::time::Duration;
use tokio::time::sleep;

/// A worker actor that can be told to crash.
struct Worker {
    name: String,
}

#[async_trait]
impl Actor for Worker {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[{}] Worker started with pid {}", self.name, ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "crash" => {
                    println!("[{}] Crashing intentionally!", self.name);
                    ctx.stop(ExitReason::Panic("intentional crash".to_string()));
                }
                "work" => {
                    println!("[{}] Doing some work...", self.name);
                }
                _ => {
                    println!("[{}] Unknown command: {}", self.name, cmd);
                }
            }
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, ctx: &mut ActorContext) {
        println!("[{}] Worker {} stopped: {}", self.name, ctx.pid(), reason);
    }
}

/// Message to set up monitoring
struct MonitorMessage {
    monitor_ref: MonitorRef,
}

/// An observer that monitors other actors without being affected by their failures.
struct Observer {
    name: String,
    monitored_ref: Option<MonitorRef>,
}

#[async_trait]
impl Actor for Observer {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[{}] Observer started with pid {}", self.name, ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(monitor_msg) = msg.downcast_ref::<MonitorMessage>() {
            self.monitored_ref = Some(monitor_msg.monitor_ref);
            println!(
                "[{}] Now monitoring with ref: {}",
                self.name, monitor_msg.monitor_ref
            );
        }
    }

    async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
        match signal {
            Signal::Down {
                reference,
                pid,
                reason,
            } => {
                if Some(reference) == self.monitored_ref {
                    println!(
                        "[{}] DOWN signal received! Monitored actor {} terminated: {}",
                        self.name, pid, reason
                    );
                    println!("[{}] I'm still alive and can continue working!", self.name);
                }
            }
            Signal::Exit { from, reason } => {
                println!(
                    "[{}] EXIT signal received from {}: {}",
                    self.name, from, reason
                );
            }
            _ => {}
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, ctx: &mut ActorContext) {
        println!("[{}] Observer {} stopped: {}", self.name, ctx.pid(), reason);
    }
}

/// A supervisor that traps exits and can handle linked actor failures.
struct Supervisor {
    name: String,
    exit_count: usize,
}

#[async_trait]
impl Actor for Supervisor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        // Trap exits so we receive EXIT signals instead of dying
        ctx.trap_exit(true);
        println!(
            "[{}] Supervisor started with pid {} (trapping exits)",
            self.name,
            ctx.pid()
        );
    }

    async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}

    async fn handle_signal(&mut self, signal: Signal, ctx: &mut ActorContext) {
        match signal {
            Signal::Exit { from, reason } => {
                self.exit_count += 1;
                println!(
                    "[{}] EXIT signal #{} received from {}: {}",
                    self.name, self.exit_count, from, reason
                );

                if reason.is_normal() {
                    println!("[{}] Worker exited normally, no action needed", self.name);
                } else {
                    println!("[{}] Worker crashed! I could restart it here...", self.name);
                }

                // Stop after handling 2 exits
                if self.exit_count >= 2 {
                    println!("[{}] Handled enough exits, shutting down", self.name);
                    ctx.stop(ExitReason::Normal);
                }
            }
            Signal::Down {
                reference,
                pid,
                reason,
            } => {
                println!(
                    "[{}] DOWN signal received for {} (ref: {}): {}",
                    self.name, pid, reference, reason
                );
            }
            _ => {}
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, ctx: &mut ActorContext) {
        println!(
            "[{}] Supervisor {} stopped: {}",
            self.name,
            ctx.pid(),
            reason
        );
    }
}

#[tokio::main]
async fn main() {
    println!("=== Links and Monitors Example ===\n");

    let system = ActorSystem::new();

    // Scenario 1: Monitor (unidirectional)
    println!("--- Scenario 1: Monitor ---");
    println!("Observer monitors Worker1. When Worker1 crashes, Observer receives");
    println!("a DOWN signal but continues running.\n");

    let worker1 = system.spawn(Worker {
        name: "Worker1".to_string(),
    });

    let observer = system.spawn(Observer {
        name: "Observer".to_string(),
        monitored_ref: None,
    });

    // Set up the monitor relationship: observer monitors worker1
    let monitor_ref = worker1.monitor(observer.pid()).expect("Failed to monitor");

    // Send the monitor ref to the observer
    observer
        .send(Box::new(MonitorMessage { monitor_ref }))
        .await
        .unwrap();

    sleep(Duration::from_millis(100)).await;

    println!("\nCrashing Worker1...\n");
    worker1.send(Box::new("crash")).await.unwrap();

    sleep(Duration::from_millis(200)).await;

    println!("\nChecking if Observer is still alive...");
    if system.is_alive(observer.pid()) {
        println!("✓ Observer is still alive!");
    }

    sleep(Duration::from_millis(100)).await;

    // Scenario 2: Link with trap_exit (bidirectional but handled)
    println!("\n--- Scenario 2: Link with trap_exit ---");
    println!("Supervisor links to Worker2 and Worker3, but traps exits.");
    println!("When workers crash, Supervisor receives EXIT signals but doesn't die.\n");

    let supervisor = system.spawn(Supervisor {
        name: "Supervisor".to_string(),
        exit_count: 0,
    });

    let worker2 = system.spawn(Worker {
        name: "Worker2".to_string(),
    });

    let worker3 = system.spawn(Worker {
        name: "Worker3".to_string(),
    });

    // Link supervisor to workers
    system
        .link(supervisor.pid(), worker2.pid())
        .expect("Failed to link supervisor and worker2");
    system
        .link(supervisor.pid(), worker3.pid())
        .expect("Failed to link supervisor and worker3");

    sleep(Duration::from_millis(100)).await;

    println!("\nCrashing Worker2...\n");
    worker2.send(Box::new("crash")).await.unwrap();

    sleep(Duration::from_millis(200)).await;

    println!("\nCrashing Worker3...\n");
    worker3.send(Box::new("crash")).await.unwrap();

    sleep(Duration::from_millis(300)).await;

    // Scenario 3: Link without trap_exit (bidirectional, both die)
    println!("\n--- Scenario 3: Link without trap_exit ---");
    println!("Worker4 and Worker5 are linked. When Worker4 crashes,");
    println!("Worker5 also dies because it's not trapping exits.\n");

    let worker4 = system.spawn(Worker {
        name: "Worker4".to_string(),
    });

    let worker5 = system.spawn(Worker {
        name: "Worker5".to_string(),
    });

    // Link workers together
    system
        .link(worker4.pid(), worker5.pid())
        .expect("Failed to link worker4 and worker5");

    sleep(Duration::from_millis(100)).await;

    println!("Before crash:");
    println!("  Worker4 alive: {}", system.is_alive(worker4.pid()));
    println!("  Worker5 alive: {}", system.is_alive(worker5.pid()));

    println!("\nCrashing Worker4...\n");
    worker4.send(Box::new("crash")).await.unwrap();

    sleep(Duration::from_millis(300)).await;

    println!("After crash:");
    println!("  Worker4 alive: {}", system.is_alive(worker4.pid()));
    println!("  Worker5 alive: {}", system.is_alive(worker5.pid()));
    println!("✓ Both workers died due to link!");

    sleep(Duration::from_millis(100)).await;

    println!("\n=== Summary ===");
    println!("• Monitors: Unidirectional, observer receives DOWN signal");
    println!("• Links: Bidirectional, both actors fail together");
    println!("• Links + trap_exit: Bidirectional, but handled gracefully");
    println!("\nExample completed!");
}
