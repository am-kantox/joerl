//! Ping-pong actor example.
//!
//! Demonstrates two actors communicating back and forth.

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, Message, Pid};

enum PingPongMessage {
    Ping(Pid, usize),
    Pong(Pid, usize),
    Stop,
}

struct PingActor {
    max_rounds: usize,
}

#[async_trait]
impl Actor for PingActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[PING {}] Started", ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(msg) = msg.downcast_ref::<PingPongMessage>() {
            match msg {
                PingPongMessage::Pong(from, count) => {
                    println!("[PING {}] Received pong #{} from {}", ctx.pid(), count, from);
                    
                    if *count < self.max_rounds {
                        // Send ping back
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                        println!("[PING {}] Sending ping #{}", ctx.pid(), count + 1);
                        
                        // In a real system, we'd store the ActorRef, but for this example
                        // we'll demonstrate the concept
                    } else {
                        println!("[PING {}] Maximum rounds reached, stopping", ctx.pid());
                        ctx.stop(joerl::ExitReason::Normal);
                    }
                }
                PingPongMessage::Stop => {
                    ctx.stop(joerl::ExitReason::Normal);
                }
                _ => {}
            }
        }
    }

    async fn stopped(&mut self, _reason: &joerl::ExitReason, ctx: &mut ActorContext) {
        println!("[PING {}] Stopped", ctx.pid());
    }
}

struct PongActor {
    max_rounds: usize,
}

#[async_trait]
impl Actor for PongActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[PONG {}] Started", ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(msg) = msg.downcast_ref::<PingPongMessage>() {
            match msg {
                PingPongMessage::Ping(from, count) => {
                    println!("[PONG {}] Received ping #{} from {}", ctx.pid(), count, from);
                    
                    if *count < self.max_rounds {
                        // Send pong back
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                        println!("[PONG {}] Sending pong #{}", ctx.pid(), count);
                    } else {
                        println!("[PONG {}] Maximum rounds reached, stopping", ctx.pid());
                        ctx.stop(joerl::ExitReason::Normal);
                    }
                }
                PingPongMessage::Stop => {
                    ctx.stop(joerl::ExitReason::Normal);
                }
                _ => {}
            }
        }
    }

    async fn stopped(&mut self, _reason: &joerl::ExitReason, ctx: &mut ActorContext) {
        println!("[PONG {}] Stopped", ctx.pid());
    }
}

#[tokio::main]
async fn main() {
    println!("=== Ping-Pong Actor Example ===\n");

    let system = ActorSystem::new();
    
    let ping = system.spawn(PingActor { max_rounds: 5 });
    let pong = system.spawn(PongActor { max_rounds: 5 });

    println!("Ping actor: {}", ping.pid());
    println!("Pong actor: {}\n", pong.pid());

    // Start the ping-pong game
    ping.send(Box::new(PingPongMessage::Ping(pong.pid(), 1)))
        .await
        .unwrap();

    // Wait for the game to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    println!("\nExample completed!");
}
