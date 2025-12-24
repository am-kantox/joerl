//! Example: GenServer sending messages to itself
//!
//! Demonstrates a GenServer that sends cast messages to itself to implement
//! a self-incrementing counter loop.
//!
//! This pattern is useful for:
//! - Background processing loops
//! - Periodic tasks without using timers
//! - State machines that need to trigger their own state transitions
//! - Work queues that process items sequentially

use async_trait::async_trait;
use joerl::gen_server::{CallResponse, GenServer, GenServerContext, GenServerRef, spawn};
use joerl::{ActorSystem, ExitReason};
use std::sync::Arc;

// ============================================================================
// Self-Incrementing Counter
// ============================================================================

struct Counter;

#[derive(Debug)]
enum CounterCall {
    GetValue,
    Start { server_ref: GenServerRef<Counter> },
}

#[derive(Debug)]
enum CounterCast {
    Increment { server_ref: GenServerRef<Counter> },
}

#[async_trait]
impl GenServer for Counter {
    type State = (i32, i32); // (current_value, max_value)
    type Call = CounterCall;
    type Cast = CounterCast;
    type CallReply = i32;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        println!("[Counter] Initializing");
        (0, 10) // Start at 0, stop at 10
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match call {
            CounterCall::GetValue => {
                println!("[Counter] Current value: {}", state.0);
                CallResponse::Reply(state.0)
            }
            CounterCall::Start { server_ref } => {
                println!("[Counter] Starting self-incrementing loop");
                // Send the first increment message to self
                // The server_ref in the message allows continued self-messaging
                let _ = server_ref
                    .cast(CounterCast::Increment {
                        server_ref: server_ref.clone(),
                    })
                    .await;
                CallResponse::Reply(state.0)
            }
        }
    }

    async fn handle_cast(
        &mut self,
        cast: Self::Cast,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) {
        match cast {
            CounterCast::Increment { server_ref } => {
                state.0 += 1;
                println!("[Counter] Incremented to {}", state.0);

                // Small delay to make output readable
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

                if state.0 < state.1 {
                    // Send next increment message to self
                    let _ = server_ref
                        .cast(CounterCast::Increment {
                            server_ref: server_ref.clone(),
                        })
                        .await;
                } else {
                    println!("[Counter] Reached max value {}, stopping", state.1);
                    ctx.stop(ExitReason::Normal);
                }
            }
        }
    }

    async fn terminate(
        &mut self,
        reason: &ExitReason,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        println!(
            "[Counter] Terminated with reason: {}, final value: {}",
            reason, state.0
        );
    }
}

#[tokio::main]
async fn main() {
    println!("=== GenServer Self-Messaging Example ===\n");

    let system = Arc::new(ActorSystem::new());

    // Spawn the counter GenServer
    let counter_ref = spawn(&system, Counter);
    println!("[Main] Spawned counter with PID: {}\n", counter_ref.pid());

    // Get initial value
    let value = counter_ref.call(CounterCall::GetValue).await.unwrap();
    println!("[Main] Initial value: {}\n", value);

    // Start the self-incrementing loop by passing a reference to itself
    let _ = counter_ref
        .call(CounterCall::Start {
            server_ref: counter_ref.clone(),
        })
        .await;

    // Wait for the counter to finish (it will stop itself at max value)
    tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

    println!("\n=== Example completed ===");
}
