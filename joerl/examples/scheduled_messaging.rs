//! Scheduled messaging example.
//!
//! Demonstrates Erlang-style `send_after` for delayed message delivery.
//! Shows timer cancellation, scheduling to named processes, and
//! practical use cases like timeouts and reminders.

use async_trait::async_trait;
use joerl::scheduler::Destination;
use joerl::{Actor, ActorContext, ActorSystem, Message};
use tokio::time::{Duration, sleep};

/// A reminder actor that schedules follow-up messages.
struct ReminderActor {
    name: String,
}

#[async_trait]
impl Actor for ReminderActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[{}] Started with Pid: {}", self.name, ctx.pid());

        // Schedule a reminder to ourselves after 2 seconds
        ctx.send_after(
            Destination::Pid(ctx.pid()),
            Box::new("First reminder".to_string()),
            Duration::from_secs(2),
        );
        println!("[{}] Scheduled reminder in 2 seconds", self.name);
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(text) = msg.downcast_ref::<String>() {
            println!("[{}] Received: {}", self.name, text);

            // Schedule another reminder
            if text == "First reminder" {
                ctx.send_after(
                    Destination::Pid(ctx.pid()),
                    Box::new("Second reminder".to_string()),
                    Duration::from_secs(1),
                );
                println!("[{}] Scheduled second reminder in 1 second", self.name);
            }
        }
    }
}

/// A timeout actor that expects a response within a time limit.
struct TimeoutActor {
    waiting_for_response: bool,
}

#[async_trait]
impl Actor for TimeoutActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[TimeoutActor] Started with Pid: {}", ctx.pid());

        // Simulate waiting for a response with a timeout
        self.waiting_for_response = true;

        // Schedule a timeout message
        ctx.send_after(
            Destination::Pid(ctx.pid()),
            Box::new("timeout".to_string()),
            Duration::from_secs(3),
        );
        println!("[TimeoutActor] Waiting for response (3 second timeout)...");
    }

    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(text) = msg.downcast_ref::<String>() {
            if text == "response" && self.waiting_for_response {
                println!("[TimeoutActor] ✓ Received response in time!");
                self.waiting_for_response = false;
            } else if text == "timeout" && self.waiting_for_response {
                println!("[TimeoutActor] ✗ Timeout! No response received");
                self.waiting_for_response = false;
            }
        }
    }
}

/// A coordinator that sends messages to named actors.
struct Coordinator;

#[async_trait]
impl Actor for Coordinator {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some("schedule_to_worker") = msg.downcast_ref::<&str>().copied() {
            // Schedule a message to a named worker
            ctx.send_after(
                Destination::Name("worker".to_string()),
                Box::new("Delayed task".to_string()),
                Duration::from_secs(1),
            );
            println!("[Coordinator] Scheduled message to 'worker' in 1 second");
        }
    }
}

/// A worker that receives delayed messages.
struct Worker;

#[async_trait]
impl Actor for Worker {
    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(text) = msg.downcast_ref::<String>() {
            println!("[Worker] Received delayed message: {}", text);
        }
    }
}

#[tokio::main]
async fn main() {
    println!("=== Scheduled Messaging Example ===\n");

    let system = ActorSystem::new();

    // Example 1: Self-reminders
    println!("--- Example 1: Self-Reminders ---");
    let _reminder = system.spawn(ReminderActor {
        name: "Reminder".to_string(),
    });
    sleep(Duration::from_millis(500)).await;

    // Example 2: Timeout pattern
    println!("\n--- Example 2: Timeout Pattern ---");
    let timeout_actor = system.spawn(TimeoutActor {
        waiting_for_response: false,
    });
    sleep(Duration::from_millis(500)).await;

    // Simulate sending a response before timeout (comment out to see timeout)
    println!("[Main] Sending response to TimeoutActor...");
    system
        .send(timeout_actor.pid(), Box::new("response".to_string()))
        .await
        .unwrap();

    // Example 3: Cancelling timers
    println!("\n--- Example 3: Timer Cancellation ---");
    let cancel_demo = system.spawn(ReminderActor {
        name: "CancelDemo".to_string(),
    });

    // Schedule a message
    let timer_ref = system.send_after(
        Destination::Pid(cancel_demo.pid()),
        Box::new("This will be cancelled".to_string()),
        Duration::from_secs(10),
    );
    println!("[Main] Scheduled message with 10 second delay");

    // Cancel it immediately
    sleep(Duration::from_millis(100)).await;
    let cancelled = system.cancel_timer(timer_ref).unwrap();
    if cancelled {
        println!("[Main] ✓ Timer cancelled successfully");
    } else {
        println!("[Main] ✗ Timer already fired (unexpected)");
    }

    // Example 4: Scheduling to named processes
    println!("\n--- Example 4: Named Process Scheduling ---");
    let worker = system.spawn(Worker);
    system.register("worker", worker.pid()).unwrap();
    println!("[Main] Registered worker");

    let coordinator = system.spawn(Coordinator);
    system
        .send(coordinator.pid(), Box::new("schedule_to_worker"))
        .await
        .unwrap();

    // Example 5: Multiple scheduled messages
    println!("\n--- Example 5: Multiple Timers ---");
    let multi = system.spawn(ReminderActor {
        name: "Multi".to_string(),
    });

    for i in 1..=3 {
        system.send_after(
            Destination::Pid(multi.pid()),
            Box::new(format!("Timer {}", i)),
            Duration::from_millis(i * 500),
        );
    }
    println!("[Main] Scheduled 3 messages at different intervals");

    // Wait for all scheduled messages to be delivered
    println!("\n--- Waiting for scheduled messages ---");
    sleep(Duration::from_secs(5)).await;

    println!("\n=== Example Complete ===");
}
