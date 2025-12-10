//! Example demonstrating selective receive with RPC-style pattern.
//!
//! This example shows how to use selective receive (similar to Erlang's
//! `receive` with pattern matching) to implement RPC with correlation IDs.
//! Actors can wait for specific messages while leaving others in the mailbox.
//!
//! Run with: cargo run --example actor_selective_receive

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, Message, Pid};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Request message with correlation ID
#[derive(Debug, Clone)]
struct Request {
    correlation_id: u64,
    from: Pid,
    query: String,
}

/// Response message with correlation ID
#[derive(Debug, Clone)]
struct Response {
    correlation_id: u64,
    result: String,
}

/// Command to trigger RPC
#[derive(Debug, Clone)]
enum Command {
    MakeRequest(String),
    MakeMultipleRequests,
}

/// Server actor that responds to requests
struct ServerActor {
    name: String,
}

#[async_trait]
impl Actor for ServerActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(req) = msg.downcast_ref::<Request>() {
            println!(
                "🔵 [{}] Received request {} from {:?}: '{}'",
                self.name, req.correlation_id, req.from, req.query
            );

            // Simulate processing time
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Send response back
            let response = Response {
                correlation_id: req.correlation_id,
                result: format!("[{}] Processed: {}", self.name, req.query),
            };

            ctx.send(req.from, Box::new(response)).await.ok();
            println!(
                "✅ [{}] Sent response for request {}",
                self.name, req.correlation_id
            );
        }
    }
}

/// Client actor that makes RPC calls using selective receive
struct ClientActor {
    name: String,
    server: Pid,
    next_id: u64,
    results: Arc<Mutex<Vec<(u64, String)>>>,
}

#[async_trait]
impl Actor for ClientActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        // Handle commands
        if let Some(cmd) = msg.downcast_ref::<Command>() {
            match cmd {
                Command::MakeRequest(query) => {
                    let request_id = self.next_id;
                    self.next_id += 1;

                    println!(
                        "🟢 [{}] Making RPC call {} with query: '{}'",
                        self.name, request_id, query
                    );

                    // Send request
                    let request = Request {
                        correlation_id: request_id,
                        from: ctx.pid(),
                        query: query.clone(),
                    };
                    ctx.send(self.server, Box::new(request)).await.ok();

                    // Use selective receive to wait for specific response
                    // Other messages will remain in mailbox
                    let response = ctx
                        .receive_timeout(
                            |msg| {
                                msg.downcast_ref::<Response>()
                                    .filter(|r| r.correlation_id == request_id)
                                    .cloned()
                            },
                            Duration::from_secs(2),
                        )
                        .await;

                    match response {
                        Some(resp) => {
                            println!(
                                "✨ [{}] Got response for request {}: '{}'",
                                self.name, request_id, resp.result
                            );
                            self.results
                                .lock()
                                .await
                                .push((resp.correlation_id, resp.result));
                        }
                        None => {
                            println!(
                                "⏰ [{}] Timeout waiting for response to request {}",
                                self.name, request_id
                            );
                        }
                    }
                }

                Command::MakeMultipleRequests => {
                    // Make multiple concurrent requests
                    // Each will wait for its specific response using correlation ID
                    let queries = vec![
                        "What is 2+2?",
                        "What is the capital of France?",
                        "What is Rust?",
                    ];

                    for query in queries {
                        let request_id = self.next_id;
                        self.next_id += 1;

                        println!(
                            "🟢 [{}] Making RPC call {} with query: '{}'",
                            self.name, request_id, query
                        );

                        let request = Request {
                            correlation_id: request_id,
                            from: ctx.pid(),
                            query: query.to_string(),
                        };
                        ctx.send(self.server, Box::new(request)).await.ok();
                    }

                    // Now collect all responses - selective receive ensures
                    // we get them in any order they arrive
                    for _ in 0..3 {
                        let response = ctx
                            .receive_timeout(
                                |msg| msg.downcast_ref::<Response>().cloned(),
                                Duration::from_secs(2),
                            )
                            .await;

                        if let Some(resp) = response {
                            println!(
                                "✨ [{}] Got response {}: '{}'",
                                self.name, resp.correlation_id, resp.result
                            );
                            self.results
                                .lock()
                                .await
                                .push((resp.correlation_id, resp.result));
                        }
                    }
                }
            }
        }
    }
}

/// Actor that demonstrates try_receive (non-blocking)
struct MonitorActor {
    stats: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Actor for MonitorActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>()
            && *cmd == "check"
        {
            println!("👀 [Monitor] Checking for any responses...");

            // Try to receive without blocking
            // This checks pending queue and current mailbox
            let response = ctx.try_receive(|msg| msg.downcast_ref::<Response>().cloned());

            match response {
                Some(resp) => {
                    let stat = format!("Found response {} in mailbox", resp.correlation_id);
                    println!("📊 [Monitor] {}", stat);
                    self.stats.lock().await.push(stat);
                }
                None => {
                    println!("📊 [Monitor] No responses in mailbox");
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    println!("=== Selective Receive Example ===\n");

    let system = Arc::new(ActorSystem::new());

    // Spawn server
    let server = system.spawn(ServerActor {
        name: "Server".to_string(),
    });

    // Spawn client
    let results = Arc::new(Mutex::new(Vec::new()));
    let client = system.spawn(ClientActor {
        name: "Client".to_string(),
        server: server.pid(),
        next_id: 1,
        results: results.clone(),
    });

    println!("=== Single RPC Call ===\n");

    // Make a single RPC call
    client
        .send(Box::new(Command::MakeRequest("Hello, Server!".to_string())))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    println!("\n=== Multiple Concurrent RPC Calls ===\n");

    // Make multiple concurrent calls
    client
        .send(Box::new(Command::MakeMultipleRequests))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("\n=== Try Receive (Non-blocking) ===\n");

    // Spawn monitor that uses try_receive
    let monitor_stats = Arc::new(Mutex::new(Vec::new()));
    let monitor = system.spawn(MonitorActor {
        stats: monitor_stats.clone(),
    });

    // Send some responses to server's mailbox for monitor to find
    for i in 1..=3 {
        monitor
            .send(Box::new(Response {
                correlation_id: 100 + i,
                result: format!("Background response {}", i),
            }))
            .await
            .unwrap();
    }

    // Check multiple times
    for _ in 0..4 {
        monitor.send(Box::new("check")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Print results
    println!("\n=== Results ===");
    let all_results = results.lock().await;
    println!("Client received {} responses:", all_results.len());
    for (id, result) in all_results.iter() {
        println!("  [{}] {}", id, result);
    }

    let monitor_results = monitor_stats.lock().await;
    println!("\nMonitor stats: {} items found", monitor_results.len());
    for stat in monitor_results.iter() {
        println!("  {}", stat);
    }

    println!("\n=== Key Concepts Demonstrated ===");
    println!("1. RPC with correlation IDs - wait for specific responses");
    println!("2. Selective receive with timeout - don't block forever");
    println!("3. Multiple concurrent requests - each gets its own response");
    println!("4. Non-blocking try_receive - check mailbox without waiting");
    println!("5. Messages not matching pattern stay in mailbox for later\n");
}
