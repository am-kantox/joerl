//! Simple counter actor example.
//!
//! Demonstrates basic actor message handling.

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, Message};

/// Messages understood by [`Counter`].
///
/// A real enum is the recommended idiom: an unknown message is a compile error
/// at the send site rather than a silently ignored `Box<dyn Any>`.
#[derive(Debug)]
enum CounterMsg {
    Increment,
    Decrement,
    Get,
    Stop,
}

struct Counter {
    count: i32,
}

#[async_trait]
impl Actor for Counter {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("Counter actor started with pid {}", ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        match msg.downcast::<CounterMsg>() {
            Ok(cmd) => match *cmd {
                CounterMsg::Increment => {
                    self.count += 1;
                    println!("[{}] Count incremented to: {}", ctx.pid(), self.count);
                }
                CounterMsg::Decrement => {
                    self.count -= 1;
                    println!("[{}] Count decremented to: {}", ctx.pid(), self.count);
                }
                CounterMsg::Get => {
                    println!("[{}] Current count: {}", ctx.pid(), self.count);
                }
                CounterMsg::Stop => {
                    println!("[{}] Stopping counter", ctx.pid());
                    ctx.stop(joerl::ExitReason::Normal);
                }
            },
            Err(other) => {
                // Never drop a message silently.
                println!("[{}] Unexpected message: {other:?}", ctx.pid());
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
    counter.send(Box::new(CounterMsg::Increment)).await.unwrap();
    counter.send(Box::new(CounterMsg::Increment)).await.unwrap();
    counter.send(Box::new(CounterMsg::Increment)).await.unwrap();
    counter.send(Box::new(CounterMsg::Get)).await.unwrap();
    counter.send(Box::new(CounterMsg::Decrement)).await.unwrap();
    counter.send(Box::new(CounterMsg::Get)).await.unwrap();
    counter.send(Box::new(CounterMsg::Stop)).await.unwrap();

    // Wait for actor to process messages
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\nExample completed!");
}
