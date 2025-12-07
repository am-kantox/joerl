//! Standalone EPMD server.
//!
//! This is a standalone EPMD (Erlang Port Mapper Daemon) server for joerl.
//! It provides node discovery and registration services for distributed actor systems.
//!
//! ## Usage
//!
//! Start the EPMD server:
//!
//! ```bash
//! cargo run --example epmd_server
//! ```
//!
//! By default, it listens on `127.0.0.1:4369` (the standard EPMD port).
//!
//! You can specify a custom address:
//!
//! ```bash
//! cargo run --example epmd_server -- 0.0.0.0:4369
//! ```
//!
//! ## Protocol
//!
//! The server implements a simple binary protocol for:
//! - Node registration (name, host, port)
//! - Node lookup by name
//! - Listing all registered nodes
//! - Keep-alive messages
//! - Ping/pong health checks
//!
//! ## Architecture
//!
//! Like Erlang's EPMD, this server:
//! - Maintains a registry of nodes in memory
//! - Removes dead nodes after keep-alive timeout
//! - Handles concurrent connections
//! - Uses a simple, efficient binary protocol

use joerl::epmd::{DEFAULT_EPMD_PORT, EpmdServer};
use std::time::Duration;
use tracing::{Level, info};

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
    let address = if args.len() > 1 {
        args[1].clone()
    } else {
        format!("127.0.0.1:{}", DEFAULT_EPMD_PORT)
    };

    info!("========================================");
    info!("      joerl EPMD Server v0.3.0");
    info!("========================================");
    info!("");
    info!("Starting EPMD server on {}", address);
    info!("Press Ctrl+C to stop");
    info!("");

    // Create and run EPMD server
    let server = EpmdServer::new(address).with_keep_alive_timeout(Duration::from_secs(60));

    // Run the server (blocks forever)
    server.run().await?;

    Ok(())
}
