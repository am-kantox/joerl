//! Remote Ping-Pong Example
//!
//! Demonstrates remote messaging between two distributed nodes using location transparency.
//! This example shows how to:
//! - Register custom message types for serialization
//! - Create distributed actor systems
//! - Send messages between actors on different nodes
//! - Handle replies across node boundaries
//!
//! # Running
//!
//! Open two terminals:
//!
//! Terminal 1 (Server node):
//! ```sh
//! cargo run --example remote_ping_pong -- server
//! ```
//!
//! Terminal 2 (Client node):
//! ```sh
//! cargo run --example remote_ping_pong -- client
//! ```

use async_trait::async_trait;
use joerl::distributed::DistributedSystem;
use joerl::serialization::{SerializableMessage, SerializationError, register_message_type};
use joerl::{Actor, ActorContext, Message, Pid};
use std::any::Any;
use std::env;
use std::sync::Arc;
use tokio::time::{Duration, sleep};

// ============================================================================
// Message Definitions
// ============================================================================

/// Ping message with sender information for replies
#[derive(Debug, Clone)]
struct PingMessage {
    sequence: u32,
    sender: Pid,
}

impl SerializableMessage for PingMessage {
    fn message_type_id(&self) -> &'static str {
        "example::PingMessage"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.sequence.to_le_bytes());
        bytes.extend_from_slice(&self.sender.node().to_le_bytes());
        bytes.extend_from_slice(&self.sender.id().to_le_bytes());
        Ok(bytes)
    }
}

fn deserialize_ping(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() != 16 {
        return Err(SerializationError::InvalidFormat(
            "PingMessage requires 16 bytes".into(),
        ));
    }
    let sequence = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let node = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let id = u64::from_le_bytes([
        data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
    ]);
    let sender = Pid::with_node(node, id);
    Ok(Box::new(PingMessage { sequence, sender }))
}

/// Pong message as reply
#[derive(Debug, Clone)]
struct PongMessage {
    sequence: u32,
}

impl SerializableMessage for PongMessage {
    fn message_type_id(&self) -> &'static str {
        "example::PongMessage"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        Ok(self.sequence.to_le_bytes().to_vec())
    }
}

fn deserialize_pong(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() != 4 {
        return Err(SerializationError::InvalidFormat(
            "PongMessage requires 4 bytes".into(),
        ));
    }
    let sequence = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    Ok(Box::new(PongMessage { sequence }))
}

// ============================================================================
// Server Actor (Pong Responder)
// ============================================================================

/// Server actor that responds to pings with pongs
struct ServerActor {
    dist_system: Arc<DistributedSystem>,
}

#[async_trait]
impl Actor for ServerActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("✅ Server actor started with PID: {}", ctx.pid());
        println!("   Waiting for ping messages...");
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(boxed) = msg.downcast_ref::<Box<dyn SerializableMessage>>()
            && let Some(ping) = boxed.as_ref().as_any().downcast_ref::<PingMessage>()
        {
            println!("🏓 Received PING #{} from {}", ping.sequence, ping.sender);

            // Send pong back to sender
            let pong = PongMessage {
                sequence: ping.sequence,
            };
            let pong_msg: Message = Box::new(Box::new(pong) as Box<dyn SerializableMessage>);

            match self
                .dist_system
                .send(ctx.pid(), ping.sender, pong_msg)
                .await
            {
                Ok(_) => println!("   ↪️  Sent PONG #{} back", ping.sequence),
                Err(e) => eprintln!("   ❌ Failed to send pong: {}", e),
            }
        }
    }
}

// ============================================================================
// Client Actor (Ping Sender)
// ============================================================================

/// Client actor that sends pings and tracks pongs
struct ClientActor {
    server_pid: Pid,
    dist_system: Arc<DistributedSystem>,
    sent_count: u32,
    received_count: u32,
    max_pings: u32,
}

#[async_trait]
impl Actor for ClientActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("✅ Client actor started with PID: {}", ctx.pid());
        println!("   Target server: {}", self.server_pid);

        // Send initial ping
        self.send_ping(ctx).await;
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(boxed) = msg.downcast_ref::<Box<dyn SerializableMessage>>()
            && let Some(pong) = boxed.as_ref().as_any().downcast_ref::<PongMessage>()
        {
            self.received_count += 1;
            println!(
                "🏓 Received PONG #{} ({}/{} complete)",
                pong.sequence, self.received_count, self.max_pings
            );

            // Send another ping if we haven't hit the limit
            if self.sent_count < self.max_pings {
                sleep(Duration::from_millis(500)).await;
                self.send_ping(ctx).await;
            } else {
                println!("\n✨ All pings completed successfully!");
                println!(
                    "   Sent: {}, Received: {}",
                    self.sent_count, self.received_count
                );
            }
        }
    }
}

impl ClientActor {
    async fn send_ping(&mut self, ctx: &ActorContext) {
        self.sent_count += 1;
        let ping = PingMessage {
            sequence: self.sent_count,
            sender: ctx.pid(),
        };

        let ping_msg: Message = Box::new(Box::new(ping) as Box<dyn SerializableMessage>);

        println!("🏓 Sending PING #{}...", self.sent_count);
        match self
            .dist_system
            .send(ctx.pid(), self.server_pid, ping_msg)
            .await
        {
            Ok(_) => {}
            Err(e) => eprintln!("❌ Failed to send ping: {}", e),
        }
    }
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Register message types globally (must be done before any messaging)
    println!("📋 Registering message types...");
    register_message_type("example::PingMessage", Box::new(deserialize_ping));
    register_message_type("example::PongMessage", Box::new(deserialize_pong));

    let args: Vec<String> = env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("server");

    match mode {
        "server" => run_server().await,
        "client" => run_client().await,
        _ => {
            eprintln!("Usage: {} [server|client]", args[0]);
            eprintln!("\nExamples:");
            eprintln!("  {} server  # Start server node", args[0]);
            eprintln!("  {} client  # Start client node", args[0]);
            std::process::exit(1);
        }
    }
}

async fn run_server() {
    println!("\n🚀 Starting SERVER node...\n");

    // Create distributed system for server
    let node = DistributedSystem::new("server_node", "127.0.0.1:16000", "127.0.0.1:4369")
        .await
        .expect("Failed to create server node");

    println!("✅ Server node created:");
    println!("   Node name: server_node");
    println!("   Node ID: {}", node.node_id());
    println!("   Listening on: 127.0.0.1:16000\n");

    // Spawn server actor
    let server_actor = ServerActor {
        dist_system: Arc::clone(&node),
    };
    let server_ref = node.system().spawn(server_actor);

    println!("📍 Server actor Pid: {}\n", server_ref.pid());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Server is ready! Waiting for clients...");
    println!("Press Ctrl+C to stop");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Keep server running
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");

    println!("\n\n🛑 Shutting down server...");
    node.shutdown().await.ok();
}

async fn run_client() {
    println!("\n🚀 Starting CLIENT node...\n");

    // Create distributed system for client
    let node = DistributedSystem::new("client_node", "127.0.0.1:16001", "127.0.0.1:4369")
        .await
        .expect("Failed to create client node");

    println!("✅ Client node created:");
    println!("   Node name: client_node");
    println!("   Node ID: {}", node.node_id());
    println!("   Listening on: 127.0.0.1:16001\n");

    // Wait for nodes to register
    println!("⏳ Waiting for EPMD registration...");
    sleep(Duration::from_millis(200)).await;

    // Connect to server node
    println!("🔌 Connecting to server_node...");
    node.connect_to_node("server_node")
        .await
        .expect("Failed to connect to server");
    println!("✅ Connected to server_node\n");

    // Look up server actor Pid
    // In a real application, you'd use a registry or discovery mechanism
    // For this example, we'll construct the Pid directly
    let server_node_id = DistributedSystem::hash_node_name("server_node");
    let server_pid = Pid::with_node(server_node_id, 1); // Assuming first actor spawned

    println!("📍 Target server Pid: {}\n", server_pid);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Starting ping-pong exchange...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Spawn client actor
    let client_actor = ClientActor {
        server_pid,
        dist_system: Arc::clone(&node),
        sent_count: 0,
        received_count: 0,
        max_pings: 5,
    };
    node.system().spawn(client_actor);

    // Wait for ping-pong to complete
    sleep(Duration::from_secs(10)).await;

    println!("\n🛑 Shutting down client...");
    node.shutdown().await.ok();
}
