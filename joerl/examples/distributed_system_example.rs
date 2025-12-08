//! Distributed System Example
//!
//! Demonstrates the unified ActorSystem API for location-transparent actor messaging.
//! The same API works for both local and distributed messaging with no code changes.
//!
//! ## Running
//!
//! Terminal 1 - Start EPMD:
//! ```bash
//! cargo run --example epmd_server
//! ```
//!
//! Terminal 2 - Start node A:
//! ```bash
//! cargo run --example distributed_system_example -- node_a 5001
//! ```
//!
//! Terminal 3 - Start node B:
//! ```bash
//! cargo run --example distributed_system_example -- node_b 5002
//! ```
//!
//! Terminal 4 - Start node C:
//! ```bash
//! cargo run --example distributed_system_example -- node_c 5003
//! ```

use async_trait::async_trait;
use joerl::epmd::DEFAULT_EPMD_PORT;
use joerl::{Actor, ActorContext, ActorSystem, Message};
use std::time::Duration;
use tokio::time;
use tracing::{Level, error, info};

/// A simple echo actor
struct EchoActor {
    node_name: String,
}

#[async_trait]
impl Actor for EchoActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        info!(
            "[{}] Echo actor started (pid: {})",
            self.node_name,
            ctx.pid()
        );
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(text) = msg.downcast_ref::<String>() {
            info!(
                "[{}] Echo actor {} received: '{}'",
                self.node_name,
                ctx.pid(),
                text
            );
        }
    }
}

/// A coordinator actor that tracks cluster state
struct CoordinatorActor {
    node_name: String,
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
        if let Some(cmd) = msg.downcast_ref::<&str>()
            && *cmd == "status"
        {
            info!("[{}] Coordinator: system operational", self.node_name);
        }
    }
}

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
    let listen_address = format!("127.0.0.1:{}", port);
    let epmd_address = format!("127.0.0.1:{}", DEFAULT_EPMD_PORT);

    info!("========================================");
    info!("    Unified ActorSystem Example");
    info!("========================================");
    info!("");

    // Create distributed system using unified API
    info!("[{}] Creating distributed actor system...", node_name);
    let system = ActorSystem::new_distributed(&node_name, &listen_address, &epmd_address)
        .await
        .map_err(|e| format!("Failed to create distributed system: {}", e))?;

    info!("[{}] Node: {}", node_name, system.node().unwrap());
    info!("[{}] Distributed: {}", node_name, system.is_distributed());
    info!("");

    // Spawn local actors - same API as non-distributed!
    info!("[{}] Spawning local actors...", node_name);

    let echo1 = system.spawn(EchoActor {
        node_name: node_name.clone(),
    });

    let echo2 = system.spawn(EchoActor {
        node_name: node_name.clone(),
    });

    let coordinator = system.spawn(CoordinatorActor {
        node_name: node_name.clone(),
    });

    info!("[{}] Local actors spawned:", node_name);
    info!("  - Echo 1: {}", echo1.pid());
    info!("  - Echo 2: {}", echo2.pid());
    info!("  - Coordinator: {}", coordinator.pid());
    info!("");

    // Graceful shutdown handler
    let system_clone = system.clone();
    let node_name_clone = node_name.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("");
        info!("[{}] Received Ctrl+C, shutting down...", node_name_clone);
        if let Err(e) = system_clone.shutdown().await {
            error!("[{}] Shutdown error: {}", node_name_clone, e);
        }
        std::process::exit(0);
    });

    // Main loop - discover peers and demonstrate messaging
    let mut interval = time::interval(Duration::from_secs(5));
    let mut iteration = 0;

    loop {
        interval.tick().await;
        iteration += 1;

        info!("[{}] --- Iteration {} ---", node_name, iteration);

        // List connected nodes using Erlang-style API
        let connected_nodes = system.nodes();
        info!("[{}] Connected nodes: {:?}", node_name, connected_nodes);

        // Discover peers via EPMD
        match system.list_nodes().await {
            Ok(nodes) => {
                let peers: Vec<_> = nodes
                    .iter()
                    .filter(|n| n.name != node_name)
                    .map(|n| &n.name)
                    .collect();

                if peers.is_empty() {
                    info!("[{}] No other nodes discovered yet", node_name);
                } else {
                    info!(
                        "[{}] Cluster members: {} (self) + {:?}",
                        node_name, node_name, peers
                    );

                    // Try connecting to each peer
                    for peer_name in peers {
                        match system.connect_to_node(peer_name).await {
                            Ok(_) => {
                                info!("[{}] Connected to {}", node_name, peer_name);
                            }
                            Err(e) => {
                                info!("[{}] Connection to {} failed: {}", node_name, peer_name, e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("[{}] Failed to list nodes: {}", node_name, e);
            }
        }

        // Send local messages to demonstrate functionality
        if iteration % 2 == 0 {
            info!("[{}] Sending local test messages...", node_name);
            let _ = echo1
                .send(Box::new(format!(
                    "Local hello from iteration {}",
                    iteration
                )))
                .await;
            let _ = coordinator.send(Box::new("status")).await;
        }

        info!("");
    }
}
