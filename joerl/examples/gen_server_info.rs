//! Example: GenServer handle_info with named servers
//!
//! Demonstrates:
//! 1. Spawning a GenServer with a registered name using spawn_named()
//! 2. Sending info messages to the GenServer
//! 3. Looking up GenServers by name using whereis()
//! 4. Using handle_info() to process arbitrary messages
//!
//! This pattern is useful for:
//! - Receiving messages from non-GenServer actors
//! - Processing system notifications
//! - Handling external events
//! - Integrating with other messaging patterns

use async_trait::async_trait;
use joerl::gen_server::{CallResponse, GenServer, GenServerContext, spawn_named};
use joerl::{ActorSystem, ExitReason, Message};
use std::sync::Arc;

// ============================================================================
// Message Logger - Logs all info messages it receives
// ============================================================================

struct MessageLogger;

#[derive(Debug)]
enum LoggerCall {
    GetCount,
}

#[derive(Debug)]
enum LoggerCast {
    #[allow(dead_code)]
    Clear,
}

#[async_trait]
impl GenServer for MessageLogger {
    type State = Vec<String>; // Store received messages
    type Call = LoggerCall;
    type Cast = LoggerCast;
    type CallReply = usize;

    async fn init(&mut self, ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        println!("[Logger] Started with PID: {}", ctx.pid());
        Vec::new()
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match call {
            LoggerCall::GetCount => {
                println!("[Logger] Message count requested: {}", state.len());
                CallResponse::Reply(state.len())
            }
        }
    }

    async fn handle_cast(
        &mut self,
        cast: Self::Cast,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        match cast {
            LoggerCast::Clear => {
                println!("[Logger] Clearing {} messages", state.len());
                state.clear();
            }
        }
    }

    async fn handle_info(
        &mut self,
        msg: Message,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        // Try to downcast to different message types
        if let Some(text) = msg.downcast_ref::<String>() {
            println!("[Logger] Received String: {}", text);
            state.push(text.clone());
        } else if let Some(text) = msg.downcast_ref::<&str>() {
            println!("[Logger] Received &str: {}", text);
            state.push(text.to_string());
        } else if let Some(num) = msg.downcast_ref::<i32>() {
            println!("[Logger] Received i32: {}", num);
            state.push(format!("number: {}", num));
        } else {
            println!("[Logger] Received unknown message type");
        }
    }

    async fn terminate(
        &mut self,
        reason: &ExitReason,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        println!(
            "[Logger] Terminating with reason: {}, logged {} messages",
            reason,
            state.len()
        );
    }
}

// ============================================================================
// Event Handler - Processes different event types
// ============================================================================

struct EventHandler;

#[derive(Debug, Clone)]
enum Event {
    UserLogin { username: String },
    UserLogout { username: String },
    DataUpdated { table: String, rows: usize },
}

#[derive(Debug)]
enum HandlerCall {
    GetStats,
}

#[derive(Debug)]
enum HandlerCast {}

type Stats = (usize, usize, usize); // (logins, logouts, updates)

#[async_trait]
impl GenServer for EventHandler {
    type State = Stats;
    type Call = HandlerCall;
    type Cast = HandlerCast;
    type CallReply = Stats;

    async fn init(&mut self, ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        println!("[EventHandler] Started with PID: {}", ctx.pid());
        (0, 0, 0)
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match call {
            HandlerCall::GetStats => CallResponse::Reply(*state),
        }
    }

    async fn handle_cast(
        &mut self,
        _cast: Self::Cast,
        _state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
    }

    async fn handle_info(
        &mut self,
        msg: Message,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        if let Some(event) = msg.downcast_ref::<Event>() {
            match event {
                Event::UserLogin { username } => {
                    state.0 += 1;
                    println!(
                        "[EventHandler] User logged in: {} (total: {})",
                        username, state.0
                    );
                }
                Event::UserLogout { username } => {
                    state.1 += 1;
                    println!(
                        "[EventHandler] User logged out: {} (total: {})",
                        username, state.1
                    );
                }
                Event::DataUpdated { table, rows } => {
                    state.2 += 1;
                    println!(
                        "[EventHandler] Data updated in {}: {} rows (total events: {})",
                        table, rows, state.2
                    );
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    println!("=== GenServer handle_info Example ===\n");

    let system = Arc::new(ActorSystem::new());

    // ========================================================================
    // Example 1: Named message logger
    // ========================================================================
    println!("--- Example 1: Named Message Logger ---");

    // Spawn a named GenServer
    let logger = spawn_named(&system, MessageLogger, "logger").unwrap();
    println!("[Main] Spawned logger: {}\n", logger.pid());

    // Send various info messages
    logger
        .send_info(Box::new("Hello from info!".to_string()))
        .await
        .unwrap();
    logger.send_info(Box::new("Another message")).await.unwrap();
    logger.send_info(Box::new(42i32)).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Look up by name and send more messages
    let logger_pid = system.whereis("logger").unwrap();
    println!("\n[Main] Found logger by name: {}", logger_pid);
    // Note: You must use send_info(), not system.send() directly
    logger
        .send_info(Box::new("Message via GenServerRef!".to_string()))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Get count via call
    let count = logger.call(LoggerCall::GetCount).await.unwrap();
    println!("[Main] Logger has {} messages\n", count);

    // ========================================================================
    // Example 2: Event handler with custom types
    // ========================================================================
    println!("--- Example 2: Event Handler ---");

    let event_handler = spawn_named(&system, EventHandler, "events").unwrap();
    println!("[Main] Spawned event handler: {}\n", event_handler.pid());

    // Send various events
    event_handler
        .send_info(Box::new(Event::UserLogin {
            username: "alice".to_string(),
        }))
        .await
        .unwrap();

    event_handler
        .send_info(Box::new(Event::UserLogin {
            username: "bob".to_string(),
        }))
        .await
        .unwrap();

    event_handler
        .send_info(Box::new(Event::DataUpdated {
            table: "users".to_string(),
            rows: 5,
        }))
        .await
        .unwrap();

    event_handler
        .send_info(Box::new(Event::UserLogout {
            username: "alice".to_string(),
        }))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Get stats
    let stats = event_handler.call(HandlerCall::GetStats).await.unwrap();
    println!(
        "\n[Main] Event stats: {} logins, {} logouts, {} updates\n",
        stats.0, stats.1, stats.2
    );

    // ========================================================================
    // Example 3: Looking up and sending to a named GenServer
    // ========================================================================
    println!("--- Example 3: Send to Named GenServer ---");

    // You can also create a GenServerRef from a looked-up Pid
    if let Some(logger_pid) = system.whereis("logger") {
        println!("[Main] Found logger via whereis: {}", logger_pid);

        // Send one more info message
        logger
            .send_info(Box::new("Final message!".to_string()))
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Get final count
        let final_count = logger.call(LoggerCall::GetCount).await.unwrap();
        println!("[Main] Final message count: {}", final_count);
    }

    println!("\n=== Example completed ===");
}
