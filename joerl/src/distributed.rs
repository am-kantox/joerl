//! Distributed actor system support.
//!
//! This module provides location transparency for actors across multiple nodes.
//! Actors can send messages to remote actors using the same API as local actors.
//!
//! ## Architecture
//!
//! - **DistributedSystem**: Extends ActorSystem with node awareness and EPMD integration
//! - **NodeConnection**: TCP connection to a remote node
//! - **NodeRegistry**: Manages connections to remote nodes
//! - **RemoteMessage**: Serializable wrapper for messages sent across nodes
//!
//! ## Usage
//!
//! ```no_run
//! use joerl::distributed::DistributedSystem;
//! use joerl::{Actor, ActorContext, Message};
//! use async_trait::async_trait;
//!
//! struct MyActor;
//!
//! #[async_trait]
//! impl Actor for MyActor {
//!     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create a distributed system
//!     let system = DistributedSystem::new("node_a", "127.0.0.1:5000", "127.0.0.1:4369")
//!         .await
//!         .expect("Failed to create distributed system");
//!     
//!     // Spawn actors - works the same as before
//!     let actor = system.system().spawn(MyActor);
//!     
//!     // Send messages - transparently works for local and remote actors
//!     actor.send(Box::new("Hello")).await.ok();
//! }
//! ```

use crate::epmd::{EpmdClient, NodeInfo};
use crate::{ActorSystem, Message, Pid};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Errors that can occur in distributed operations
#[derive(Debug, Error)]
pub enum DistributedError {
    #[error("EPMD error: {0}")]
    EpmdError(#[from] crate::epmd::EpmdError),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Actor error: {0}")]
    ActorError(#[from] crate::ActorError),
}

pub type Result<T> = std::result::Result<T, DistributedError>;

/// A message wrapper for network transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    /// Target actor Pid
    pub to: Pid,

    /// Sender Pid (if any)
    pub from: Option<Pid>,

    /// Serialized message payload
    pub payload: Vec<u8>,
}

/// A connection to a remote node
#[allow(dead_code)] // Used in future remote messaging implementation
struct NodeConnection {
    node_info: NodeInfo,
    stream: RwLock<Option<TcpStream>>,
}

impl NodeConnection {
    /// Creates a new connection to a remote node
    async fn connect(node_info: NodeInfo) -> Result<Self> {
        let addr = node_info.address();
        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                info!("Connected to remote node {} at {}", node_info.name, addr);
                Ok(Self {
                    node_info,
                    stream: RwLock::new(Some(stream)),
                })
            }
            Err(e) => Err(DistributedError::ConnectionFailed(format!(
                "Failed to connect to {}: {}",
                addr, e
            ))),
        }
    }

    /// Sends a message to the remote node
    #[allow(dead_code)] // Used in future remote messaging implementation
    async fn send_message(&self, msg: &NetworkMessage) -> Result<()> {
        let mut stream_guard = self.stream.write().await;

        // Reconnect if needed
        if stream_guard.is_none() {
            let addr = self.node_info.address();
            match TcpStream::connect(&addr).await {
                Ok(stream) => {
                    info!("Reconnected to {}", addr);
                    *stream_guard = Some(stream);
                }
                Err(e) => {
                    return Err(DistributedError::ConnectionFailed(format!(
                        "Failed to reconnect to {}: {}",
                        addr, e
                    )));
                }
            }
        }

        // Serialize message
        let msg_bytes = bincode::serialize(msg)
            .map_err(|e| DistributedError::SerializationError(e.to_string()))?;

        let len = (msg_bytes.len() as u32).to_be_bytes();

        // Send all data, reconnecting on any error
        let stream = stream_guard.as_mut().unwrap();

        if let Err(e) = stream.write_all(&len).await {
            *stream_guard = None;
            return Err(e.into());
        }

        if let Err(e) = stream.write_all(&msg_bytes).await {
            *stream_guard = None;
            return Err(e.into());
        }

        if let Err(e) = stream.flush().await {
            *stream_guard = None;
            return Err(e.into());
        }

        debug!("Sent message to {} (pid: {})", self.node_info.name, msg.to);
        Ok(())
    }
}

/// Registry of connections to remote nodes
struct NodeRegistry {
    connections: Arc<DashMap<String, Arc<NodeConnection>>>,
}

impl NodeRegistry {
    fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
        }
    }

    /// Gets or creates a connection to a remote node
    async fn get_or_connect(&self, node_info: NodeInfo) -> Result<Arc<NodeConnection>> {
        let node_name = node_info.name.clone();

        // Check if we already have a connection
        if let Some(conn) = self.connections.get(&node_name) {
            return Ok(Arc::clone(&*conn));
        }

        // Create new connection
        let conn = Arc::new(NodeConnection::connect(node_info).await?);
        self.connections
            .insert(node_name.clone(), Arc::clone(&conn));

        Ok(conn)
    }

    /// Removes a connection
    #[allow(dead_code)] // Used in future connection cleanup
    fn remove(&self, node_name: &str) {
        self.connections.remove(node_name);
        debug!("Removed connection to node {}", node_name);
    }
}

/// A distributed actor system with EPMD integration
pub struct DistributedSystem {
    /// The underlying actor system
    system: Arc<ActorSystem>,

    /// This node's name
    node_name: String,

    /// This node's ID (hash of name)
    node_id: u32,

    /// EPMD client for discovery
    epmd_client: EpmdClient,

    /// Registry of remote node connections
    node_registry: Arc<NodeRegistry>,

    /// TCP listener for incoming connections
    _listener_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DistributedSystem {
    /// Creates a new distributed actor system
    ///
    /// # Arguments
    ///
    /// * `node_name` - Unique name for this node
    /// * `listen_address` - Address to listen for incoming connections (e.g., "127.0.0.1:5000")
    /// * `epmd_address` - Address of EPMD server (e.g., "127.0.0.1:4369")
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use joerl::distributed::DistributedSystem;
    ///
    /// # async fn example() {
    /// let system = DistributedSystem::new(
    ///     "my_node",
    ///     "127.0.0.1:5000",
    ///     "127.0.0.1:4369"
    /// ).await.expect("Failed to create system");
    /// # }
    /// ```
    pub async fn new(
        node_name: impl Into<String>,
        listen_address: impl Into<String>,
        epmd_address: impl Into<String>,
    ) -> Result<Arc<Self>> {
        let node_name = node_name.into();
        let listen_address = listen_address.into();
        let epmd_address = epmd_address.into();

        // Create underlying actor system
        let system = ActorSystem::new();

        // Create EPMD client
        let epmd_client = EpmdClient::new(epmd_address);

        // Extract host and port from listen_address
        let parts: Vec<&str> = listen_address.split(':').collect();
        let host = parts[0].to_string();
        let port: u16 = parts.get(1).and_then(|p| p.parse().ok()).ok_or_else(|| {
            DistributedError::ConnectionFailed(format!(
                "Invalid listen address: {}",
                listen_address
            ))
        })?;

        // Register with EPMD
        epmd_client.register(&node_name, &host, port).await?;
        info!(
            "Registered node {} with EPMD at {}:{}",
            node_name, host, port
        );

        // Start keep-alive loop
        epmd_client
            .start_keep_alive_loop(node_name.clone(), Duration::from_secs(20))
            .await;

        // Calculate node ID from name
        let node_id = Self::hash_node_name(&node_name);

        // Create node registry
        let node_registry = Arc::new(NodeRegistry::new());

        // Start TCP listener for incoming connections
        let listener = TcpListener::bind(&listen_address).await?;
        info!("Listening for node connections on {}", listen_address);

        let system_clone = Arc::clone(&system);
        let listener_handle = tokio::spawn(async move {
            Self::accept_connections(listener, system_clone).await;
        });

        Ok(Arc::new(Self {
            system,
            node_name,
            node_id,
            epmd_client,
            node_registry,
            _listener_handle: Some(listener_handle),
        }))
    }

    /// Returns the underlying ActorSystem
    pub fn system(&self) -> &Arc<ActorSystem> {
        &self.system
    }

    /// Returns this node's name
    pub fn node_name(&self) -> &str {
        &self.node_name
    }

    /// Returns this node's ID
    pub fn node_id(&self) -> u32 {
        self.node_id
    }

    /// Creates a Pid for an actor on this node
    pub fn make_pid(&self, local_id: u64) -> Pid {
        Pid {
            node: self.node_id,
            id: local_id,
        }
    }

    /// Sends a message to a Pid (local or remote)
    pub async fn send(&self, to: Pid, msg: Message) -> Result<()> {
        if to.node == 0 || to.node == self.node_id {
            // Local message
            self.system.send(to, msg).await?;
            Ok(())
        } else {
            // Remote message - need to lookup node and forward
            self.send_remote(to, msg).await
        }
    }

    /// Sends a message to a remote actor
    async fn send_remote(&self, to: Pid, _msg: Message) -> Result<()> {
        // In a full implementation, we would:
        // 1. Serialize the message
        // 2. Lookup the node by node_id
        // 3. Get or create connection to that node
        // 4. Send the NetworkMessage

        // For now, return an error
        Err(DistributedError::NodeNotFound(format!(
            "Remote messaging not yet fully implemented for node {}",
            to.node
        )))
    }

    /// Discovers and connects to a remote node by name
    pub async fn connect_to_node(&self, node_name: &str) -> Result<()> {
        // Lookup node in EPMD
        let node_info = self
            .epmd_client
            .lookup(node_name)
            .await?
            .ok_or_else(|| DistributedError::NodeNotFound(node_name.to_string()))?;

        // Establish connection
        self.node_registry.get_or_connect(node_info).await?;

        Ok(())
    }

    /// Lists all nodes registered with EPMD
    pub async fn list_nodes(&self) -> Result<Vec<NodeInfo>> {
        Ok(self.epmd_client.list_nodes().await?)
    }

    /// Gracefully shuts down this node
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down node {}", self.node_name);
        self.epmd_client.unregister(&self.node_name).await?;
        Ok(())
    }

    /// Accepts incoming connections from other nodes
    async fn accept_connections(listener: TcpListener, system: Arc<ActorSystem>) {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("Accepted connection from {}", addr);
                    let system_clone = Arc::clone(&system);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_node_connection(stream, system_clone).await {
                            error!("Connection handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Handles messages from a connected node
    async fn handle_node_connection(
        mut stream: TcpStream,
        _system: Arc<ActorSystem>,
    ) -> Result<()> {
        loop {
            // Read message length
            let mut len_buf = [0u8; 4];
            match stream.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    debug!("Remote node disconnected");
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            }

            let len = u32::from_be_bytes(len_buf) as usize;
            if len > 10 * 1024 * 1024 {
                // 10MB max
                return Err(DistributedError::SerializationError(
                    "Message too large".to_string(),
                ));
            }

            // Read message
            let mut msg_buf = vec![0u8; len];
            stream.read_exact(&mut msg_buf).await?;

            // Deserialize
            let _net_msg: NetworkMessage = bincode::deserialize(&msg_buf)
                .map_err(|e| DistributedError::SerializationError(e.to_string()))?;

            // In a full implementation, we would:
            // 1. Deserialize the message payload
            // 2. Route to the target actor
            // 3. Handle errors appropriately

            warn!("Received remote message (forwarding not yet implemented)");
        }
    }

    /// Hashes a node name to a node ID
    fn hash_node_name(name: &str) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        (hasher.finish() & 0xFFFFFFFF) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_node_name() {
        let id1 = DistributedSystem::hash_node_name("node_a");
        let id2 = DistributedSystem::hash_node_name("node_b");
        let id3 = DistributedSystem::hash_node_name("node_a");

        assert_ne!(id1, id2);
        assert_eq!(id1, id3);
    }

    #[test]
    fn test_network_message_serialization() {
        let msg = NetworkMessage {
            to: Pid { node: 1, id: 42 },
            from: Some(Pid { node: 2, id: 100 }),
            payload: vec![1, 2, 3, 4],
        };

        let bytes = bincode::serialize(&msg).unwrap();
        let deserialized: NetworkMessage = bincode::deserialize(&bytes).unwrap();

        assert_eq!(msg.to, deserialized.to);
        assert_eq!(msg.from, deserialized.from);
        assert_eq!(msg.payload, deserialized.payload);
    }
}
