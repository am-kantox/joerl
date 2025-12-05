//! Simple counter actor example.
//!
//! Demonstrates basic actor message handling.

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, Message};

struct Counter {
    count: i32,
}

#[async_trait]
impl Actor for Counter {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("Counter actor started with pid {}", ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "increment" => {
                    self.count += 1;
                    println!("[{}] Count incremented to: {}", ctx.pid(), self.count);
                }
                "decrement" => {
                    self.count -= 1;
                    println!("[{}] Count decremented to: {}", ctx.pid(), self.count);
                }
                "get" => {
                    println!("[{}] Current count: {}", ctx.pid(), self.count);
                }
                "stop" => {
                    println!("[{}] Stopping counter", ctx.pid());
                    ctx.stop(joerl::ExitReason::Normal);
                }
                _ => {
                    println!("[{}] Unknown command: {}", ctx.pid(), cmd);
                }
            }
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, ctx: &mut ActorContext) {
        println!("[{}] Counter stopped with reason: {}", ctx.pid(), reason);
    }
}

#[tokio::main]
async fn main() {
    println!("=== Counter Actor Example ===\n");

    let system = ActorSystem::new();
    let counter = system.spawn(Counter { count: 0 });

    // Send some messages
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("get")).await.unwrap();
    counter.send(Box::new("decrement")).await.unwrap();
    counter.send(Box::new("get")).await.unwrap();
    counter.send(Box::new("stop")).await.unwrap();

    // Wait for actor to process messages
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\nExample completed!");
}
