//! EPMD client implementation.
//!
//! Used by nodes to register with EPMD and query for other nodes.

use super::{EpmdError, EpmdMessage, NodeInfo, NodeMetadata, Result};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info, warn};

/// EPMD client for node registration and discovery
#[derive(Clone)]
pub struct EpmdClient {
    /// EPMD server address
    epmd_address: String,

    /// Connection pool (shared across clones)
    connection: std::sync::Arc<RwLock<Option<TcpStream>>>,
}

impl EpmdClient {
    /// Creates a new EPMD client
    pub fn new(epmd_address: impl Into<String>) -> Self {
        Self {
            epmd_address: epmd_address.into(),
            connection: std::sync::Arc::new(RwLock::new(None)),
        }
    }

    /// Registers a node with EPMD
    pub async fn register(
        &self,
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
    ) -> Result<()> {
        self.register_with_metadata(name, host, port, NodeMetadata::default())
            .await
    }

    /// Registers a node with metadata
    pub async fn register_with_metadata(
        &self,
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        metadata: NodeMetadata,
    ) -> Result<()> {
        let msg = EpmdMessage::Register {
            name: name.into(),
            host: host.into(),
            port,
            metadata,
        };

        let response = self.send_message(msg).await?;

        match response {
            EpmdMessage::RegisterOk => Ok(()),
            EpmdMessage::RegisterError { reason } => Err(EpmdError::NodeAlreadyRegistered(reason)),
            _ => Err(EpmdError::ProtocolError(
                "Unexpected response to Register".to_string(),
            )),
        }
    }

    /// Unregisters a node from EPMD
    pub async fn unregister(&self, name: impl Into<String>) -> Result<()> {
        let msg = EpmdMessage::Unregister { name: name.into() };

        let response = self.send_message(msg).await?;

        match response {
            EpmdMessage::UnregisterOk => Ok(()),
            _ => Err(EpmdError::ProtocolError(
                "Unexpected response to Unregister".to_string(),
            )),
        }
    }

    /// Looks up a node by name
    pub async fn lookup(&self, name: impl Into<String>) -> Result<Option<NodeInfo>> {
        let name = name.into();
        let msg = EpmdMessage::Lookup { name: name.clone() };

        let response = self.send_message(msg).await?;

        match response {
            EpmdMessage::LookupResult { node } => {
                if let Some(ref info) = node {
                    debug!("Lookup {} -> {}", name, info);
                } else {
                    debug!("Lookup {} -> not found", name);
                }
                Ok(node)
            }
            _ => Err(EpmdError::ProtocolError(
                "Unexpected response to Lookup".to_string(),
            )),
        }
    }

    /// Lists all registered nodes
    pub async fn list_nodes(&self) -> Result<Vec<NodeInfo>> {
        let msg = EpmdMessage::ListNodes;
        let response = self.send_message(msg).await?;

        match response {
            EpmdMessage::NodeList { nodes } => {
                debug!("Listed {} nodes", nodes.len());
                Ok(nodes)
            }
            _ => Err(EpmdError::ProtocolError(
                "Unexpected response to ListNodes".to_string(),
            )),
        }
    }

    /// Pings EPMD to check if it's alive
    pub async fn ping(&self) -> Result<()> {
        let msg = EpmdMessage::Ping;
        let response = self.send_message(msg).await?;

        match response {
            EpmdMessage::Pong => Ok(()),
            _ => Err(EpmdError::ProtocolError(
                "Unexpected response to Ping".to_string(),
            )),
        }
    }

    /// Sends a keep-alive message for a registered node
    pub async fn keep_alive(&self, name: impl Into<String>) -> Result<()> {
        let msg = EpmdMessage::KeepAlive { name: name.into() };
        let response = self.send_message(msg).await?;

        match response {
            EpmdMessage::KeepAliveAck => Ok(()),
            _ => Err(EpmdError::ProtocolError(
                "Unexpected response to KeepAlive".to_string(),
            )),
        }
    }

    /// Starts a keep-alive loop for a node
    pub async fn start_keep_alive_loop(
        &self,
        name: String,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        tokio::spawn(async move {
            let mut ticker = time::interval(interval);
            loop {
                ticker.tick().await;
                match client.keep_alive(&name).await {
                    Ok(_) => debug!("Keep-alive sent for {}", name),
                    Err(e) => {
                        error!("Keep-alive failed for {}: {}", name, e);
                        break;
                    }
                }
            }
        })
    }

    /// Sends a message to EPMD and returns the response
    async fn send_message(&self, msg: EpmdMessage) -> Result<EpmdMessage> {
        // Try to reuse existing connection
        let mut connection_guard = self.connection.write().await;

        if connection_guard.is_none() {
            // Establish new connection
            match TcpStream::connect(&self.epmd_address).await {
                Ok(stream) => {
                    info!("Connected to EPMD at {}", self.epmd_address);
                    *connection_guard = Some(stream);
                }
                Err(e) => {
                    return Err(EpmdError::ConnectionFailed(format!(
                        "Failed to connect to EPMD at {}: {}",
                        self.epmd_address, e
                    )));
                }
            }
        }

        let stream = connection_guard.as_mut().unwrap();

        // Send message
        let msg_bytes = msg.to_bytes()?;
        let msg_len = (msg_bytes.len() as u32).to_be_bytes();

        if let Err(e) = stream.write_all(&msg_len).await {
            warn!("Failed to send message length: {}", e);
            *connection_guard = None;
            return Err(e.into());
        }

        if let Err(e) = stream.write_all(&msg_bytes).await {
            warn!("Failed to send message: {}", e);
            *connection_guard = None;
            return Err(e.into());
        }

        if let Err(e) = stream.flush().await {
            warn!("Failed to flush: {}", e);
            *connection_guard = None;
            return Err(e.into());
        }

        // Read response
        let mut len_buf = [0u8; 4];
        if let Err(e) = stream.read_exact(&mut len_buf).await {
            warn!("Failed to read response length: {}", e);
            *connection_guard = None;
            return Err(e.into());
        }

        let len = u32::from_be_bytes(len_buf) as usize;
        let mut response_buf = vec![0u8; len];

        if let Err(e) = stream.read_exact(&mut response_buf).await {
            warn!("Failed to read response: {}", e);
            *connection_guard = None;
            return Err(e.into());
        }

        EpmdMessage::from_bytes(&response_buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = EpmdClient::new("127.0.0.1:4369");
        assert_eq!(client.epmd_address, "127.0.0.1:4369");
    }

    #[test]
    fn test_client_clone() {
        let client1 = EpmdClient::new("127.0.0.1:4369");
        let client2 = client1.clone();

        // They should share the same connection pool
        assert!(std::sync::Arc::ptr_eq(
            &client1.connection,
            &client2.connection
        ));
    }
}
