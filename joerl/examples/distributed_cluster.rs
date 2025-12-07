//! Distributed cluster example with EPMD.
//!
//! Demonstrates a complete distributed actor system using EPMD for node discovery.
//! This example shows how to build a multi-node cluster where nodes can discover
//! each other and communicate transparently.
//!
//! ## Running
//!
//! Terminal 1 - Start EPMD server:
//! ```bash
//! cargo run --example epmd_server
//! ```
//!
//! Terminal 2 - Start first node:
//! ```bash
//! cargo run --example distributed_cluster -- node_a 5001
//! ```
//!
//! Terminal 3 - Start second node:
//! ```bash
//! cargo run --example distributed_cluster -- node_b 5002
//! ```
//!
//! Terminal 4 - Start third node:
//! ```bash
//! cargo run --example distributed_cluster -- node_c 5003
//! ```
//!
//! Each node will:
//! - Register itself with EPMD
//! - Discover other nodes
//! - Send messages to peers
//! - Demonstrate location transparency

use async_trait::async_trait;
use joerl::epmd::{DEFAULT_EPMD_PORT, EpmdClient};
use joerl::{Actor, ActorContext, ActorSystem, Message};
use std::time::Duration;
use tokio::time;
use tracing::{Level, error, info};

// ============================================================================
// Cluster Node Manager
// ============================================================================

/// Manages a node in the cluster
struct ClusterNode {
    node_name: String,
    epmd_client: EpmdClient,
    system: std::sync::Arc<ActorSystem>,
}

impl ClusterNode {
    async fn new(node_name: String, port: u16, epmd_address: String) -> Self {
        let epmd_client = EpmdClient::new(epmd_address);
        let system = ActorSystem::new();

        // Register with EPMD
        match epmd_client.register(&node_name, "127.0.0.1", port).await {
            Ok(_) => info!("[{}] Registered with EPMD on port {}", node_name, port),
            Err(e) => error!("[{}] Failed to register: {}", node_name, e),
        }

        // Start keep-alive loop
        epmd_client
            .start_keep_alive_loop(node_name.clone(), Duration::from_secs(20))
            .await;

        Self {
            node_name,
            epmd_client,
            system,
        }
    }

    async fn discover_peers(&self) -> Vec<String> {
        match self.epmd_client.list_nodes().await {
            Ok(nodes) => {
                let peers: Vec<String> = nodes
                    .into_iter()
                    .filter(|n| n.name != self.node_name)
                    .map(|n| n.name)
                    .collect();

                if !peers.is_empty() {
                    info!(
                        "[{}] Discovered {} peer(s): {:?}",
                        self.node_name,
                        peers.len(),
                        peers
                    );
                }

                peers
            }
            Err(e) => {
                error!("[{}] Failed to discover peers: {}", self.node_name, e);
                vec![]
            }
        }
    }

    async fn shutdown(&self) {
        info!("[{}] Shutting down...", self.node_name);
        if let Err(e) = self.epmd_client.unregister(&self.node_name).await {
            error!("[{}] Failed to unregister: {}", self.node_name, e);
        }
    }
}

// ============================================================================
// Example Actors
// ============================================================================

/// A worker actor that can receive messages
struct WorkerActor {
    node_name: String,
    worker_id: usize,
    messages_received: usize,
}

#[async_trait]
impl Actor for WorkerActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        info!(
            "[{}] Worker-{} started (pid: {})",
            self.node_name,
            self.worker_id,
            ctx.pid()
        );
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(text) = msg.downcast_ref::<String>() {
            self.messages_received += 1;
            info!(
                "[{}] Worker-{} received message #{}: '{}'",
                self.node_name, self.worker_id, self.messages_received, text
            );
        } else if let Some(cmd) = msg.downcast_ref::<&str>() {
            if *cmd == "status" {
                info!(
                    "[{}] Worker-{} status: {} messages received",
                    self.node_name, self.worker_id, self.messages_received
                );
            } else if *cmd == "shutdown" {
                info!(
                    "[{}] Worker-{} shutting down",
                    self.node_name, self.worker_id
                );
                ctx.stop(joerl::ExitReason::Normal);
            }
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, _ctx: &mut ActorContext) {
        info!(
            "[{}] Worker-{} stopped: {}",
            self.node_name, self.worker_id, reason
        );
    }
}

/// A coordinator actor that manages cluster-wide tasks
struct CoordinatorActor {
    node_name: String,
    tasks_completed: usize,
}

#[async_trait]
impl Actor for CoordinatorActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        info!(
            "[{}] Coordinator started (pid: {})",
            self.node_name,
            ctx.pid()
        );
    }

    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(task) = msg.downcast_ref::<String>() {
            self.tasks_completed += 1;
            info!(
                "[{}] Coordinator processing task #{}: '{}'",
                self.node_name, self.tasks_completed, task
            );

            // Simulate work
            tokio::time::sleep(Duration::from_millis(100)).await;

            info!(
                "[{}] Coordinator completed task #{}",
                self.node_name, self.tasks_completed
            );
        }
    }
}

// ============================================================================
// Main Program
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .compact()
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <node_name> <port>", args[0]);
        eprintln!("Example: {} node_a 5001", args[0]);
        std::process::exit(1);
    }

    let node_name = args[1].clone();
    let port: u16 = args[2].parse()?;
    let epmd_address = format!("127.0.0.1:{}", DEFAULT_EPMD_PORT);

    info!("========================================");
    info!("     Distributed Cluster Example");
    info!("========================================");
    info!("");

    // Create cluster node
    let node = ClusterNode::new(node_name.clone(), port, epmd_address).await;

    // Spawn actors on this node
    info!("[{}] Spawning local actors...", node_name);
    let _worker1 = node.system.spawn(WorkerActor {
        node_name: node_name.clone(),
        worker_id: 1,
        messages_received: 0,
    });

    let _worker2 = node.system.spawn(WorkerActor {
        node_name: node_name.clone(),
        worker_id: 2,
        messages_received: 0,
    });

    let _coordinator = node.system.spawn(CoordinatorActor {
        node_name: node_name.clone(),
        tasks_completed: 0,
    });

    info!("[{}] Local actors spawned", node_name);
    info!("");

    // Discovery and demonstration loop
    let mut interval = time::interval(Duration::from_secs(5));
    let mut iteration = 0;

    // Handle Ctrl+C gracefully
    let node_for_shutdown = std::sync::Arc::new(tokio::sync::Mutex::new(node));
    let shutdown_node = node_for_shutdown.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("");
        info!("Received Ctrl+C, shutting down gracefully...");
        let node = shutdown_node.lock().await;
        node.shutdown().await;
        std::process::exit(0);
    });

    loop {
        interval.tick().await;
        iteration += 1;

        let node = node_for_shutdown.lock().await;

        info!("[{}] --- Iteration {} ---", node.node_name, iteration);

        // Discover peers
        let peers = node.discover_peers().await;

        if peers.is_empty() {
            info!("[{}] No peers discovered yet. Waiting...", node.node_name);
        } else {
            info!(
                "[{}] Active cluster: {} + {} peer(s)",
                node.node_name,
                node.node_name,
                peers.len()
            );

            // In a real implementation, you would:
            // 1. Lookup peer node info from EPMD
            // 2. Establish TCP connections to peers
            // 3. Send messages to remote actors
            // 4. Handle remote actor responses

            info!(
                "[{}] ℹ️  In production: would send messages to peers: {:?}",
                node.node_name, peers
            );
        }

        // Send local messages to demonstrate actor system works
        if iteration % 3 == 0 {
            info!("[{}] Sending local test messages...", node.node_name);
            let _ = _worker1
                .send(Box::new(format!(
                    "Local message at iteration {}",
                    iteration
                )))
                .await;
            let _ = _coordinator
                .send(Box::new(format!("Coordinate task {}", iteration)))
                .await;
        }

        info!("");
    }
}
