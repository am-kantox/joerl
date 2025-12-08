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
use crate::serialization::{SerializableEnvelope, SerializableMessage, get_global_registry};
use crate::{ActorSystem, Message, Pid};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
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

    /// Sender Pid (mandatory for reply patterns and debugging)
    pub from: Pid,

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
///
/// TODO: Connection Pooling
/// Future enhancement: Add connection pooling with configurable pool_size.
/// - pool_size: usize parameter for max connections per node
/// - Connection reuse and rotation strategy
/// - Pool health monitoring and cleanup
///
///   This would improve throughput for high-traffic scenarios.
pub struct NodeRegistry {
    connections_by_name: Arc<DashMap<String, Arc<NodeConnection>>>,
    connections_by_id: Arc<DashMap<u32, Arc<NodeConnection>>>,
    node_id_to_name: Arc<DashMap<u32, String>>,
    connection_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self {
            connections_by_name: Arc::new(DashMap::new()),
            connections_by_id: Arc::new(DashMap::new()),
            node_id_to_name: Arc::new(DashMap::new()),
            connection_locks: Arc::new(DashMap::new()),
        }
    }
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a list of all connected node names.
    pub fn list_connected_nodes(&self) -> Vec<String> {
        self.connections_by_name
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Resolves a node ID to its name.
    pub fn get_node_name(&self, node_id: u32) -> Option<String> {
        self.node_id_to_name.get(&node_id).map(|name| name.clone())
    }

    /// Gets or creates a connection to a remote node
    async fn get_or_connect(&self, node_info: NodeInfo) -> Result<Arc<NodeConnection>> {
        let node_name = node_info.name.clone();

        // Check if we already have a connection
        if let Some(conn) = self.connections_by_name.get(&node_name) {
            return Ok(Arc::clone(&*conn));
        }

        // Get or create lock for this node to ensure single connection
        let lock = self
            .connection_locks
            .entry(node_name.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();

        // Acquire lock to prevent concurrent connection attempts
        let _guard = lock.lock().await;

        // Double-check after acquiring lock
        if let Some(conn) = self.connections_by_name.get(&node_name) {
            return Ok(Arc::clone(&*conn));
        }

        // Create new connection
        let conn = Arc::new(NodeConnection::connect(node_info).await?);

        // Calculate node_id from node_name
        let node_id = DistributedSystem::hash_node_name(&node_name);

        // Insert into all maps
        self.connections_by_name
            .insert(node_name.clone(), Arc::clone(&conn));
        self.connections_by_id.insert(node_id, Arc::clone(&conn));
        self.node_id_to_name.insert(node_id, node_name.clone());

        info!(
            "Established connection to node {} (id: {})",
            node_name, node_id
        );

        Ok(conn)
    }

    /// Gets a connection by node ID
    async fn get_by_node_id(&self, node_id: u32) -> Result<Arc<NodeConnection>> {
        self.connections_by_id
            .get(&node_id)
            .map(|conn| Arc::clone(&*conn))
            .ok_or_else(|| {
                let node_name = self
                    .node_id_to_name
                    .get(&node_id)
                    .map(|name| name.clone())
                    .unwrap_or_else(|| format!("unknown_node_{}", node_id));
                DistributedError::NodeNotFound(node_name)
            })
    }

    /// Removes a connection
    #[allow(dead_code)] // Used in future connection cleanup
    fn remove(&self, node_name: &str) {
        // Calculate node_id for removal
        let node_id = DistributedSystem::hash_node_name(node_name);

        // Remove from all maps
        self.connections_by_name.remove(node_name);
        self.connections_by_id.remove(&node_id);
        self.node_id_to_name.remove(&node_id);
        self.connection_locks.remove(node_name);

        debug!("Removed connection to node {} (id: {})", node_name, node_id);
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

        // Calculate node ID from name
        let node_id = Self::hash_node_name(&node_name);

        // NOTE: DistributedSystem is deprecated - use ActorSystem::new_distributed() instead
        // This creates a local system temporarily for backward compatibility
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
            "Registered node {} with EPMD at {}:{} (node_id: {})",
            node_name, host, port, node_id
        );

        // Start keep-alive loop
        epmd_client
            .start_keep_alive_loop(node_name.clone(), Duration::from_secs(20))
            .await;

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
    pub async fn send(&self, from: Pid, to: Pid, msg: Message) -> Result<()> {
        if to.node == 0 || to.node == self.node_id {
            // Local message
            self.system.send(to, msg).await?;
            Ok(())
        } else {
            // Remote message - need to lookup node and forward
            self.send_remote(from, to, msg).await
        }
    }

    /// Sends a message to a remote actor
    async fn send_remote(&self, from: Pid, to: Pid, msg: Message) -> Result<()> {
        // 1. Serialize the message
        let payload = serialize_message(&msg)?;

        // 2. Create NetworkMessage with sender
        let net_msg = NetworkMessage { to, from, payload };

        // 3. Lookup node by node_id
        let conn = self.node_registry.get_by_node_id(to.node).await?;

        // 4. Send via TCP connection
        conn.send_message(&net_msg).await?;

        debug!("Sent remote message from {} to {}", from, to);
        Ok(())
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
    async fn handle_node_connection(mut stream: TcpStream, system: Arc<ActorSystem>) -> Result<()> {
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

            // Deserialize NetworkMessage
            let net_msg: NetworkMessage = bincode::deserialize(&msg_buf)
                .map_err(|e| DistributedError::SerializationError(e.to_string()))?;

            debug!(
                "Received remote message from {} to {}",
                net_msg.from, net_msg.to
            );

            // Deserialize payload
            match deserialize_message(&net_msg.payload) {
                Ok(msg) => {
                    // Route to target actor
                    if let Err(e) = system.send(net_msg.to, msg).await {
                        warn!("Failed to deliver remote message to {}: {}", net_msg.to, e);
                        // TODO: Send error response to sender
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to deserialize message payload from {}: {}",
                        net_msg.from, e
                    );
                    // TODO: Send error response to sender
                }
            }
        }
    }

    /// Hashes a node name to a node ID.
    ///
    /// This is useful when you need to construct Pids for remote actors.
    /// The hash is deterministic - the same node name always produces the same ID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::distributed::DistributedSystem;
    ///
    /// let node_id = DistributedSystem::hash_node_name("my_node");
    /// println!("Node ID: {}", node_id);
    /// ```
    pub fn hash_node_name(name: &str) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        (hasher.finish() & 0xFFFFFFFF) as u32
    }
}

/// Serializes a Message into bytes for network transport.
///
/// The message must implement SerializableMessage, otherwise an error is returned.
fn serialize_message(msg: &Message) -> Result<Vec<u8>> {
    // Try to downcast to SerializableMessage
    let serializable = msg
        .downcast_ref::<Box<dyn SerializableMessage>>()
        .ok_or_else(|| {
            DistributedError::SerializationError(
                "Message does not implement SerializableMessage".to_string(),
            )
        })?;

    // Wrap in envelope and serialize
    let envelope = SerializableEnvelope::wrap(serializable.as_ref())
        .map_err(|e| DistributedError::SerializationError(e.to_string()))?;

    Ok(envelope.to_bytes())
}

/// Deserializes bytes into a Message using the global registry.
fn deserialize_message(data: &[u8]) -> Result<Message> {
    // Reconstruct envelope from bytes
    let envelope = SerializableEnvelope::from_bytes(data)
        .map_err(|e| DistributedError::SerializationError(e.to_string()))?;

    // Get global registry
    let registry = get_global_registry();
    let registry_guard = registry.read().unwrap();

    // Unwrap envelope using registry
    let serializable = envelope
        .unwrap(&registry_guard)
        .map_err(|e| DistributedError::SerializationError(e.to_string()))?;

    // Convert to Message
    Ok(Box::new(serializable))
}

// ============================================================================
// Public API for ActorSystem integration
// ============================================================================

/// Sends a message to a remote actor via the node registry.
///
/// This is called by ActorSystem when sending to a remote Pid.
pub async fn send_remote(
    node_registry: &Arc<NodeRegistry>,
    from: Pid,
    to: Pid,
    msg: Message,
) -> Result<()> {
    // Serialize the message
    let payload = serialize_message(&msg)?;

    // Create NetworkMessage
    let net_msg = NetworkMessage { to, from, payload };

    // Lookup node by node_id
    let conn = node_registry.get_by_node_id(to.node).await?;

    // Send via TCP connection
    conn.send_message(&net_msg).await?;

    debug!("Sent remote message from {} to {}", from, to);
    Ok(())
}

/// Connects to a remote node and returns the connection.
///
/// This is called by ActorSystem::connect_to_node().
pub async fn connect_to_node(
    epmd_client: &EpmdClient,
    node_registry: &Arc<NodeRegistry>,
    remote_node_name: &str,
    _local_node_name: &str,
    _local_node_id: u32,
    _local_listen_address: &str,
) -> Result<()> {
    // Lookup remote node in EPMD
    let node_info = epmd_client
        .lookup(remote_node_name)
        .await?
        .ok_or_else(|| DistributedError::NodeNotFound(remote_node_name.to_string()))?;

    // Establish connection (will send handshake later when we implement it)
    node_registry.get_or_connect(node_info).await?;

    info!("Connected to remote node {}", remote_node_name);
    Ok(())
}

/// Pings a remote process to check if it's alive.
///
/// Returns true if the process responds within the timeout.
pub async fn ping_process(_node_registry: &Arc<NodeRegistry>, _pid: Pid) -> Result<bool> {
    // TODO: Implement ping/pong RPC protocol
    // For now, return false (not implemented)
    warn!("ping_process not yet implemented - returning false");
    Ok(false)
}

/// Accepts incoming connections from remote nodes.
///
/// This runs in a background task and handles incoming TCP connections.
pub async fn accept_connections(
    listener: TcpListener,
    system: Arc<ActorSystem>,
    _node_name: String,
    _node_id: u32,
    _listen_address: String,
) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                debug!("Accepted connection from {}", addr);
                let system_clone = Arc::clone(&system);
                tokio::spawn(async move {
                    if let Err(e) = handle_node_connection(stream, system_clone).await {
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

/// Handles messages from a connected remote node.
async fn handle_node_connection(mut stream: TcpStream, system: Arc<ActorSystem>) -> Result<()> {
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

        // Deserialize NetworkMessage
        let net_msg: NetworkMessage = bincode::deserialize(&msg_buf)
            .map_err(|e| DistributedError::SerializationError(e.to_string()))?;

        debug!(
            "Received remote message from {} to {}",
            net_msg.from, net_msg.to
        );

        // Deserialize payload
        match deserialize_message(&net_msg.payload) {
            Ok(msg) => {
                // Route to target actor
                if let Err(e) = system.send(net_msg.to, msg).await {
                    warn!("Failed to deliver remote message to {}: {}", net_msg.to, e);
                    // TODO: Send error response to sender
                }
            }
            Err(e) => {
                error!(
                    "Failed to deserialize message payload from {}: {}",
                    net_msg.from, e
                );
                // TODO: Send error response to sender
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::{SerializableMessage, SerializationError, register_message_type};
    use std::any::Any;

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
            from: Pid { node: 2, id: 100 },
            payload: vec![1, 2, 3, 4],
        };

        let bytes = bincode::serialize(&msg).unwrap();
        let deserialized: NetworkMessage = bincode::deserialize(&bytes).unwrap();

        assert_eq!(msg.to, deserialized.to);
        assert_eq!(msg.from, deserialized.from);
        assert_eq!(msg.payload, deserialized.payload);
    }

    // Test message type
    #[derive(Debug, Clone, PartialEq)]
    struct TestMsg {
        value: u32,
        text: String,
    }

    impl SerializableMessage for TestMsg {
        fn message_type_id(&self) -> &'static str {
            "test::TestMsg"
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn serialize(&self) -> std::result::Result<Vec<u8>, SerializationError> {
            let mut bytes = self.value.to_le_bytes().to_vec();
            bytes.extend_from_slice(self.text.as_bytes());
            Ok(bytes)
        }
    }

    fn deserialize_test_msg(
        data: &[u8],
    ) -> std::result::Result<Box<dyn SerializableMessage>, SerializationError> {
        if data.len() < 4 {
            return Err(SerializationError::InvalidFormat("Too short".into()));
        }
        let value = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let text = String::from_utf8(data[4..].to_vec())
            .map_err(|e| SerializationError::DeserializeFailed(e.to_string()))?;
        Ok(Box::new(TestMsg { value, text }))
    }

    #[test]
    fn test_serialize_message_roundtrip() {
        // Register the message type
        register_message_type("test::TestMsg", Box::new(deserialize_test_msg));

        // Create a message
        let original = TestMsg {
            value: 42,
            text: "Hello, World!".to_string(),
        };

        // Wrap in Message (Box<dyn Any>)
        let msg: Message = Box::new(Box::new(original.clone()) as Box<dyn SerializableMessage>);

        // Serialize
        let serialized = serialize_message(&msg).expect("Serialization should succeed");

        // Deserialize
        let deserialized =
            deserialize_message(&serialized).expect("Deserialization should succeed");

        // Extract and verify
        let result = deserialized
            .downcast_ref::<Box<dyn SerializableMessage>>()
            .expect("Should downcast to SerializableMessage")
            .as_ref()
            .as_any()
            .downcast_ref::<TestMsg>()
            .expect("Should downcast to TestMsg");

        assert_eq!(result.value, original.value);
        assert_eq!(result.text, original.text);
    }

    #[test]
    fn test_serialize_non_serializable_message() {
        // Create a message that doesn't implement SerializableMessage
        let msg: Message = Box::new("plain string");

        // Try to serialize - should fail
        let result = serialize_message(&msg);
        assert!(result.is_err());

        match result {
            Err(DistributedError::SerializationError(err)) => {
                assert!(err.contains("does not implement SerializableMessage"));
            }
            _ => panic!("Expected SerializationError"),
        }
    }

    #[test]
    fn test_deserialize_invalid_envelope() {
        // Invalid data
        let invalid_data = vec![1, 2, 3];

        let result = deserialize_message(&invalid_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_unknown_message_type() {
        use crate::serialization::SerializableEnvelope;

        // Manually create envelope with different type ID
        struct UnknownMsg;
        impl SerializableMessage for UnknownMsg {
            fn message_type_id(&self) -> &'static str {
                "test::UnknownMsg"
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn serialize(&self) -> std::result::Result<Vec<u8>, SerializationError> {
                Ok(vec![1, 2, 3, 4])
            }
        }

        let envelope = SerializableEnvelope::wrap(&UnknownMsg).unwrap();
        let bytes = envelope.to_bytes();

        // Try to deserialize - should fail with unknown type
        let result = deserialize_message(&bytes);
        assert!(result.is_err());
    }
}
