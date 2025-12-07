//! EPMD server implementation.
//!
//! A standalone TCP server that maintains a registry of nodes in the cluster.

use super::{EpmdError, EpmdMessage, NodeInfo, Result};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time;
use tracing::{debug, error, info, warn};

/// EPMD server that maintains node registry
pub struct EpmdServer {
    /// Bind address
    address: String,

    /// Registry of nodes
    nodes: Arc<DashMap<String, NodeEntry>>,

    /// Keep-alive timeout in seconds
    keep_alive_timeout: Duration,
}

/// Internal node entry with keep-alive tracking
struct NodeEntry {
    info: NodeInfo,
    last_seen: SystemTime,
}

impl EpmdServer {
    /// Creates a new EPMD server
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            nodes: Arc::new(DashMap::new()),
            keep_alive_timeout: Duration::from_secs(60),
        }
    }

    /// Sets the keep-alive timeout
    pub fn with_keep_alive_timeout(mut self, timeout: Duration) -> Self {
        self.keep_alive_timeout = timeout;
        self
    }

    /// Runs the EPMD server
    pub async fn run(self) -> Result<()> {
        let listener = TcpListener::bind(&self.address).await?;
        info!("EPMD server listening on {}", self.address);

        // Start keep-alive checker
        let nodes = Arc::clone(&self.nodes);
        let timeout = self.keep_alive_timeout;
        tokio::spawn(async move {
            Self::keep_alive_checker(nodes, timeout).await;
        });

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("New connection from {}", addr);
                    let nodes = Arc::clone(&self.nodes);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, nodes).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Handles a client connection
    async fn handle_connection(
        mut stream: TcpStream,
        nodes: Arc<DashMap<String, NodeEntry>>,
    ) -> Result<()> {
        loop {
            // Read message length (4 bytes)
            let mut len_buf = [0u8; 4];
            match stream.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Client disconnected
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            }

            let len = u32::from_be_bytes(len_buf) as usize;
            if len > 1024 * 1024 {
                // 1MB max message size
                return Err(EpmdError::ProtocolError("Message too large".to_string()));
            }

            // Read message body
            let mut msg_buf = vec![0u8; len];
            stream.read_exact(&mut msg_buf).await?;

            // Parse message
            let msg = EpmdMessage::from_bytes(&msg_buf)?;
            debug!("Received message: {:?}", msg);

            // Handle message
            let response = Self::handle_message(msg, &nodes);

            // Send response
            let response_bytes = response.to_bytes()?;
            let response_len = (response_bytes.len() as u32).to_be_bytes();

            stream.write_all(&response_len).await?;
            stream.write_all(&response_bytes).await?;
            stream.flush().await?;
        }
    }

    /// Handles an EPMD protocol message
    fn handle_message(msg: EpmdMessage, nodes: &Arc<DashMap<String, NodeEntry>>) -> EpmdMessage {
        match msg {
            EpmdMessage::Register {
                name,
                host,
                port,
                metadata,
            } => {
                if nodes.contains_key(&name) {
                    warn!("Node {} already registered", name);
                    EpmdMessage::RegisterError {
                        reason: format!("Node {} already registered", name),
                    }
                } else {
                    let info = NodeInfo::with_metadata(name.clone(), host, port, metadata);
                    let entry = NodeEntry {
                        info,
                        last_seen: SystemTime::now(),
                    };
                    nodes.insert(name.clone(), entry);
                    info!("Registered node: {}", name);
                    EpmdMessage::RegisterOk
                }
            }

            EpmdMessage::Unregister { name } => {
                if nodes.remove(&name).is_some() {
                    info!("Unregistered node: {}", name);
                    EpmdMessage::UnregisterOk
                } else {
                    warn!("Attempted to unregister unknown node: {}", name);
                    EpmdMessage::UnregisterOk // Idempotent
                }
            }

            EpmdMessage::Lookup { name } => {
                let node = nodes.get(&name).map(|entry| entry.info.clone());
                if let Some(ref info) = node {
                    debug!("Lookup {} -> {}", name, info);
                } else {
                    debug!("Lookup {} -> not found", name);
                }
                EpmdMessage::LookupResult { node }
            }

            EpmdMessage::ListNodes => {
                let node_list: Vec<NodeInfo> =
                    nodes.iter().map(|entry| entry.info.clone()).collect();
                debug!("Listing {} nodes", node_list.len());
                EpmdMessage::NodeList { nodes: node_list }
            }

            EpmdMessage::Ping => {
                debug!("Ping received");
                EpmdMessage::Pong
            }

            EpmdMessage::KeepAlive { name } => {
                if let Some(mut entry) = nodes.get_mut(&name) {
                    entry.last_seen = SystemTime::now();
                    debug!("Keep-alive from {}", name);
                    EpmdMessage::KeepAliveAck
                } else {
                    warn!("Keep-alive from unregistered node: {}", name);
                    EpmdMessage::RegisterError {
                        reason: format!("Node {} not registered", name),
                    }
                }
            }

            // These are response messages, shouldn't be received by server
            _ => EpmdMessage::RegisterError {
                reason: "Unexpected message type".to_string(),
            },
        }
    }

    /// Background task to check for dead nodes
    async fn keep_alive_checker(nodes: Arc<DashMap<String, NodeEntry>>, timeout: Duration) {
        let mut interval = time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;

            let now = SystemTime::now();
            let mut dead_nodes = Vec::new();

            for entry in nodes.iter() {
                if let Ok(elapsed) = now.duration_since(entry.last_seen)
                    && elapsed > timeout
                {
                    dead_nodes.push(entry.key().clone());
                }
            }

            for name in dead_nodes {
                nodes.remove(&name);
                warn!("Removed dead node: {} (keep-alive timeout)", name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_epmd_server_start() {
        let server = EpmdServer::new("127.0.0.1:0");
        // Just test that we can create it
        assert_eq!(server.nodes.len(), 0);
    }

    #[test]
    fn test_handle_register() {
        let nodes = Arc::new(DashMap::new());

        let msg = EpmdMessage::Register {
            name: "test_node".to_string(),
            host: "127.0.0.1".to_string(),
            port: 5000,
            metadata: Default::default(),
        };

        let response = EpmdServer::handle_message(msg, &nodes);
        assert!(matches!(response, EpmdMessage::RegisterOk));
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn test_handle_lookup() {
        let nodes = Arc::new(DashMap::new());

        // Register first
        let register_msg = EpmdMessage::Register {
            name: "test_node".to_string(),
            host: "127.0.0.1".to_string(),
            port: 5000,
            metadata: Default::default(),
        };
        EpmdServer::handle_message(register_msg, &nodes);

        // Then lookup
        let lookup_msg = EpmdMessage::Lookup {
            name: "test_node".to_string(),
        };
        let response = EpmdServer::handle_message(lookup_msg, &nodes);

        match response {
            EpmdMessage::LookupResult { node: Some(info) } => {
                assert_eq!(info.name, "test_node");
                assert_eq!(info.port, 5000);
            }
            _ => panic!("Expected LookupResult with node"),
        }
    }

    #[test]
    fn test_handle_list_nodes() {
        let nodes = Arc::new(DashMap::new());

        // Register two nodes
        for i in 0..2 {
            let msg = EpmdMessage::Register {
                name: format!("node{}", i),
                host: "127.0.0.1".to_string(),
                port: 5000 + i,
                metadata: Default::default(),
            };
            EpmdServer::handle_message(msg, &nodes);
        }

        let list_msg = EpmdMessage::ListNodes;
        let response = EpmdServer::handle_message(list_msg, &nodes);

        match response {
            EpmdMessage::NodeList { nodes: node_list } => {
                assert_eq!(node_list.len(), 2);
            }
            _ => panic!("Expected NodeList"),
        }
    }
}
