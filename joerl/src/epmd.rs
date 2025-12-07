//! EPMD (Erlang Port Mapper Daemon) equivalent for joerl.
//!
//! Provides node discovery and location transparency for distributed actors.
//! Like Erlang's EPMD, this module enables nodes to:
//! - Register themselves with a discovery service
//! - Query other nodes' connection information
//! - Maintain a cluster registry
//!
//! ## Architecture
//!
//! - **EPMD Server**: Standalone TCP service maintaining node registry
//! - **EPMD Client**: Used by nodes to register and query
//! - **Protocol**: Simple binary protocol for registration/lookup
//! - **Custom Implementations**: Protocol is public, allowing custom EPMD servers
//!
//! ## Usage
//!
//! ```no_run
//! use joerl::epmd::{EpmdServer, EpmdClient};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Start EPMD server
//!     let server = EpmdServer::new("127.0.0.1:4369");
//!     tokio::spawn(async move { server.run().await });
//!
//!     // Register a node
//!     let client = EpmdClient::new("127.0.0.1:4369");
//!     client.register("node1", "127.0.0.1", 5000).await.unwrap();
//!
//!     // Lookup a node
//!     if let Some(info) = client.lookup("node1").await.unwrap() {
//!         println!("node1 is at {}:{}", info.host, info.port);
//!     }
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Default EPMD port (same as Erlang)
pub const DEFAULT_EPMD_PORT: u16 = 4370;

/// EPMD protocol errors
#[derive(Debug, Error)]
pub enum EpmdError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Node already registered: {0}")]
    NodeAlreadyRegistered(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type Result<T> = std::result::Result<T, EpmdError>;

/// Node information stored in EPMD registry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeInfo {
    /// Node name (e.g., "node1@hostname")
    pub name: String,

    /// Host address
    pub host: String,

    /// Port for node connections
    pub port: u16,

    /// Additional metadata
    pub metadata: NodeMetadata,
}

impl NodeInfo {
    /// Creates new node information
    pub fn new(name: String, host: String, port: u16) -> Self {
        Self {
            name,
            host,
            port,
            metadata: NodeMetadata::default(),
        }
    }

    /// Creates node info with metadata
    pub fn with_metadata(name: String, host: String, port: u16, metadata: NodeMetadata) -> Self {
        Self {
            name,
            host,
            port,
            metadata,
        }
    }

    /// Returns the full node address (host:port)
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}:{}", self.name, self.host, self.port)
    }
}

/// Metadata about a node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeMetadata {
    /// Protocol version
    pub protocol_version: u32,

    /// Node capabilities (future extensibility)
    pub capabilities: Vec<String>,

    /// Custom attributes
    pub attributes: std::collections::HashMap<String, String>,
}

impl Default for NodeMetadata {
    fn default() -> Self {
        Self {
            protocol_version: 1,
            capabilities: vec![],
            attributes: std::collections::HashMap::new(),
        }
    }
}

/// EPMD protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EpmdMessage {
    /// Register a node with EPMD
    Register {
        name: String,
        host: String,
        port: u16,
        metadata: NodeMetadata,
    },

    /// Unregister a node from EPMD
    Unregister { name: String },

    /// Lookup a node by name
    Lookup { name: String },

    /// List all registered nodes
    ListNodes,

    /// Ping to check if EPMD is alive
    Ping,

    // Response messages
    /// Registration successful
    RegisterOk,

    /// Registration failed
    RegisterError { reason: String },

    /// Unregistration successful
    UnregisterOk,

    /// Lookup result
    LookupResult { node: Option<NodeInfo> },

    /// List of all nodes
    NodeList { nodes: Vec<NodeInfo> },

    /// Pong response
    Pong,

    /// Keep-alive message (sent periodically by registered nodes)
    KeepAlive { name: String },

    /// Keep-alive acknowledgment
    KeepAliveAck,
}

impl EpmdMessage {
    /// Serializes message to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        bincode::serialize(self).map_err(|e| EpmdError::SerializationError(e.to_string()))
    }

    /// Deserializes message from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        bincode::deserialize(bytes).map_err(|e| EpmdError::SerializationError(e.to_string()))
    }
}

// Sub-modules
mod client;
mod server;

pub use client::EpmdClient;
pub use server::EpmdServer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_info_creation() {
        let info = NodeInfo::new("test_node".to_string(), "127.0.0.1".to_string(), 5000);
        assert_eq!(info.name, "test_node");
        assert_eq!(info.host, "127.0.0.1");
        assert_eq!(info.port, 5000);
        assert_eq!(info.address(), "127.0.0.1:5000");
    }

    #[test]
    fn test_message_serialization() {
        let msg = EpmdMessage::Register {
            name: "test".to_string(),
            host: "localhost".to_string(),
            port: 5000,
            metadata: NodeMetadata::default(),
        };

        let bytes = msg.to_bytes().unwrap();
        let deserialized = EpmdMessage::from_bytes(&bytes).unwrap();

        match deserialized {
            EpmdMessage::Register { name, .. } => assert_eq!(name, "test"),
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_node_info_display() {
        let info = NodeInfo::new("node1".to_string(), "192.168.1.100".to_string(), 4000);
        assert_eq!(info.to_string(), "node1@192.168.1.100:4000");
    }
}
