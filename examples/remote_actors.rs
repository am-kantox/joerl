//! Remote actors example.
//!
//! Demonstrates the conceptual foundation for distributed actors in joerl.
//! This example shows how actors on different "nodes" (simulated with separate
//! actor systems) can communicate through a message broker pattern.
//!
//! In Erlang/OTP, distributed actors are transparent - you can send messages to
//! remote actors using the same syntax as local actors. This example demonstrates
//! the underlying concepts that would enable such a system.
//!
//! ## Architecture
//!
//! - **Local Actors**: Regular actors within a single ActorSystem
//! - **Node Proxy**: Actors that represent remote nodes and forward messages
//! - **Message Serialization**: Messages must be serializable to cross node boundaries
//! - **Location Transparency**: Remote actors should behave like local ones
//!
//! ## Running
//!
//! ```bash
//! cargo run --example remote_actors
//! ```

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, Message, Pid};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Serializable Message Types
// ============================================================================

/// A simple serializable message that can cross node boundaries
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum RemoteMessage {
    /// Echo request with a string payload
    Echo { content: String, reply_to: String },
    /// Echo response
    EchoReply { content: String, from_node: String },
    /// Compute request
    Compute { a: i32, b: i32, reply_to: String },
    /// Compute response
    ComputeReply { result: i32, from_node: String },
}

// ============================================================================
// Node Registry - simulates DNS/discovery for actor nodes
// ============================================================================

type NodeEntry = (Arc<ActorSystem>, Pid);

#[derive(Clone)]
struct NodeRegistry {
    nodes: Arc<RwLock<HashMap<String, NodeEntry>>>,
}

impl NodeRegistry {
    fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn register(&self, node_name: String, system: Arc<ActorSystem>, proxy_pid: Pid) {
        self.nodes
            .write()
            .await
            .insert(node_name, (system, proxy_pid));
    }

    async fn get_node(&self, node_name: &str) -> Option<(Arc<ActorSystem>, Pid)> {
        self.nodes.read().await.get(node_name).cloned()
    }

    async fn list_nodes(&self) -> Vec<String> {
        self.nodes.read().await.keys().cloned().collect()
    }
}

// ============================================================================
// Node Proxy Actor - represents a remote node's entry point
// ============================================================================

/// Proxy actor that receives messages destined for actors on this node
struct NodeProxyActor {
    node_name: String,
    registry: NodeRegistry,
}

impl NodeProxyActor {
    fn new(node_name: String, registry: NodeRegistry) -> Self {
        Self {
            node_name,
            registry,
        }
    }
}

#[async_trait]
impl Actor for NodeProxyActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!(
            "[Node:{}] Proxy started with pid {}",
            self.node_name,
            ctx.pid()
        );
    }

    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(remote_msg) = msg.downcast_ref::<RemoteMessage>() {
            println!(
                "[Node:{}] Received remote message: {:?}",
                self.node_name, remote_msg
            );

            match remote_msg {
                RemoteMessage::Echo { content, reply_to } => {
                    // Process echo request and send reply
                    println!(
                        "[Node:{}] Processing Echo request: '{}'",
                        self.node_name, content
                    );

                    if let Some((system, proxy_pid)) = self.registry.get_node(reply_to).await {
                        let reply = RemoteMessage::EchoReply {
                            content: content.clone(),
                            from_node: self.node_name.clone(),
                        };

                        if let Err(e) = system.send(proxy_pid, Box::new(reply)).await {
                            eprintln!("[Node:{}] Failed to send reply: {}", self.node_name, e);
                        }
                    }
                }
                RemoteMessage::Compute { a, b, reply_to } => {
                    // Process compute request
                    let result = a + b;
                    println!(
                        "[Node:{}] Computing {} + {} = {}",
                        self.node_name, a, b, result
                    );

                    if let Some((system, proxy_pid)) = self.registry.get_node(reply_to).await {
                        let reply = RemoteMessage::ComputeReply {
                            result,
                            from_node: self.node_name.clone(),
                        };

                        if let Err(e) = system.send(proxy_pid, Box::new(reply)).await {
                            eprintln!("[Node:{}] Failed to send reply: {}", self.node_name, e);
                        }
                    }
                }
                RemoteMessage::EchoReply { content, from_node } => {
                    println!(
                        "[Node:{}] Received Echo reply from {}: '{}'",
                        self.node_name, from_node, content
                    );
                }
                RemoteMessage::ComputeReply { result, from_node } => {
                    println!(
                        "[Node:{}] Received Compute reply from {}: {}",
                        self.node_name, from_node, result
                    );
                }
            }
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, ctx: &mut ActorContext) {
        println!(
            "[Node:{}] Proxy {} stopped: {}",
            self.node_name,
            ctx.pid(),
            reason
        );
    }
}

// ============================================================================
// Worker Actor - performs actual work on the node
// ============================================================================

struct WorkerActor {
    node_name: String,
    worker_id: usize,
}

#[async_trait]
impl Actor for WorkerActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!(
            "[Node:{}] Worker-{} started with pid {}",
            self.node_name,
            self.worker_id,
            ctx.pid()
        );
    }

    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(work) = msg.downcast_ref::<&str>() {
            println!(
                "[Node:{}] Worker-{} processing: {}",
                self.node_name, self.worker_id, work
            );

            // Simulate work
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            println!(
                "[Node:{}] Worker-{} completed work",
                self.node_name, self.worker_id
            );
        }
    }
}

// ============================================================================
// Example Runner
// ============================================================================

#[tokio::main]
async fn main() {
    println!("=== Remote Actors Example ===\n");
    println!("This example demonstrates distributed actor concepts:");
    println!("- Multiple actor systems (nodes)");
    println!("- Node discovery and registration");
    println!("- Remote message passing");
    println!("- Location transparency\n");

    let registry = NodeRegistry::new();

    // Create Node A
    println!("Creating Node A...");
    let system_a = ActorSystem::new();
    let proxy_a = system_a.spawn(NodeProxyActor::new("node_a".to_string(), registry.clone()));
    registry
        .register("node_a".to_string(), system_a.clone(), proxy_a.pid())
        .await;

    // Create workers on Node A
    let _worker_a1 = system_a.spawn(WorkerActor {
        node_name: "node_a".to_string(),
        worker_id: 1,
    });

    // Create Node B
    println!("Creating Node B...");
    let system_b = ActorSystem::new();
    let proxy_b = system_b.spawn(NodeProxyActor::new("node_b".to_string(), registry.clone()));
    registry
        .register("node_b".to_string(), system_b.clone(), proxy_b.pid())
        .await;

    // Create workers on Node B
    let _worker_b1 = system_b.spawn(WorkerActor {
        node_name: "node_b".to_string(),
        worker_id: 1,
    });

    // Create Node C
    println!("Creating Node C...");
    let system_c = ActorSystem::new();
    let proxy_c = system_c.spawn(NodeProxyActor::new("node_c".to_string(), registry.clone()));
    registry
        .register("node_c".to_string(), system_c.clone(), proxy_c.pid())
        .await;

    println!("\n--- Registered Nodes ---");
    let nodes = registry.list_nodes().await;
    println!("Available nodes: {:?}\n", nodes);

    // Wait for actors to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("--- Remote Communication Examples ---\n");

    // Example 1: Node A sends Echo request to Node B
    println!("Example 1: Node A -> Node B (Echo)");
    if let Some((system, pid)) = registry.get_node("node_b").await {
        let msg = RemoteMessage::Echo {
            content: "Hello from Node A!".to_string(),
            reply_to: "node_a".to_string(),
        };
        system.send(pid, Box::new(msg)).await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Example 2: Node C sends Compute request to Node B
    println!("\nExample 2: Node C -> Node B (Compute)");
    if let Some((system, pid)) = registry.get_node("node_b").await {
        let msg = RemoteMessage::Compute {
            a: 42,
            b: 58,
            reply_to: "node_c".to_string(),
        };
        system.send(pid, Box::new(msg)).await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Example 3: Node B sends Echo to Node A
    println!("\nExample 3: Node B -> Node A (Echo)");
    if let Some((system, pid)) = registry.get_node("node_a").await {
        let msg = RemoteMessage::Echo {
            content: "Greetings from Node B!".to_string(),
            reply_to: "node_b".to_string(),
        };
        system.send(pid, Box::new(msg)).await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Example 4: Round-robin communication
    println!("\nExample 4: Round-robin (A -> B -> C -> A)");
    if let Some((system, pid)) = registry.get_node("node_b").await {
        let msg = RemoteMessage::Echo {
            content: "Starting round-robin".to_string(),
            reply_to: "node_a".to_string(),
        };
        system.send(pid, Box::new(msg)).await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    if let Some((system, pid)) = registry.get_node("node_c").await {
        let msg = RemoteMessage::Compute {
            a: 10,
            b: 20,
            reply_to: "node_b".to_string(),
        };
        system.send(pid, Box::new(msg)).await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    println!("\n--- Summary ---");
    println!("✓ Created {} distributed nodes", nodes.len());
    println!("✓ Demonstrated location-transparent messaging");
    println!("✓ Showed serializable message passing between nodes");
    println!("✓ Illustrated request-reply pattern across nodes");
    println!("\nIn Erlang/OTP, this would be:");
    println!("  {{node_b, 'node@host'}} ! {{echo, self(), \"Hello\"}}");
    println!("\nIn joerl, you'd build a transport layer (TCP/UDP/etc.) with:");
    println!("  - Node discovery (DNS, multicast, etc.)");
    println!("  - Message serialization (bincode, serde_json, etc.)");
    println!("  - Connection management and fault tolerance");
    println!("  - Location-transparent Pid resolution");

    println!("\nExample completed!");
}
