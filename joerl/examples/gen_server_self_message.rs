//! Example: GenServer sending messages to itself
//!
//! Demonstrates two patterns for GenServer self-messaging:
//! 1. Using cast() to send immediate messages to itself
//! 2. Using send_after() to schedule delayed messages (via scheduler)
//!
//! This pattern is useful for:
//! - Background processing loops
//! - Periodic tasks and timers
//! - State machines that need to trigger their own state transitions
//! - Work queues that process items sequentially

use async_trait::async_trait;
use joerl::gen_server::{CallResponse, GenServer, GenServerContext, GenServerRef, spawn};
use joerl::{ActorSystem, ExitReason, Message};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Example 1: Immediate self-messaging with cast()
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

// ============================================================================
// Example 2: Scheduled self-messaging with send_after()
// ============================================================================

struct Ticker;

#[derive(Debug)]
enum TickerCall {
    Start,
}

#[derive(Debug)]
enum TickerCast {}

#[async_trait]
impl GenServer for Ticker {
    type State = (u32, u32); // (tick_count, max_ticks)
    type Call = TickerCall;
    type Cast = TickerCast;
    type CallReply = u32;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        println!("[Ticker] Initializing");
        (0, 5) // Start at 0, max 5 ticks
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match call {
            TickerCall::Start => {
                println!("[Ticker] Starting periodic ticker");
                // Schedule first tick to ourselves after 100ms using send_info_after
                let timer_ref = ctx.send_info_after(Box::new("tick"), Duration::from_millis(100));
                println!("[Ticker] Scheduled timer: {:?}", timer_ref);
                CallResponse::Reply(state.0)
            }
        }
    }

    async fn handle_cast(
        &mut self,
        _cast: Self::Cast,
        _state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        // No casts for this server
    }

    async fn handle_info(
        &mut self,
        msg: Message,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) {
        // Handle scheduled "tick" messages
        if let Some(tick) = msg.downcast_ref::<&str>()
            && *tick == "tick"
        {
            state.0 += 1;
            println!("[Ticker] Tick #{}", state.0);

            if state.0 < state.1 {
                // Schedule next tick after 100ms using send_info_after
                ctx.send_info_after(Box::new("tick"), Duration::from_millis(100));
            } else {
                println!("[Ticker] Reached max ticks ({}), stopping", state.1);
                ctx.stop(ExitReason::Normal);
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
            "[Ticker] Terminated with reason: {}, final ticks: {}",
            reason, state.0
        );
    }
}

#[tokio::main]
async fn main() {
    println!("=== GenServer Self-Messaging Examples ===\n");

    let system = Arc::new(ActorSystem::new());

    // ========================================================================
    // Example 1: Immediate self-messaging with cast()
    // ========================================================================
    println!("--- Example 1: Immediate Self-Messaging ---");

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
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ========================================================================
    // Example 2: Scheduled self-messaging with send_after()
    // ========================================================================
    println!("\n--- Example 2: Scheduled Self-Messaging (Ticker) ---");

    // Spawn the ticker GenServer
    let ticker_ref = spawn(&system, Ticker);
    println!("[Main] Spawned ticker with PID: {}\n", ticker_ref.pid());

    // Start the periodic ticker
    let _ = ticker_ref.call(TickerCall::Start).await;

    // Wait for ticks to complete (5 ticks * 100ms + buffer)
    tokio::time::sleep(Duration::from_millis(650)).await;

    println!("\n=== All examples completed ===");
}
