//! Distributed chat example.
//!
//! Demonstrates remote actors communicating across network nodes.
//! This example simulates a distributed chat system where actors on different
//! "nodes" can send messages to each other over TCP connections.
//!
//! In a real Erlang/OTP system, this would use the built-in distribution protocol.
//! Here we demonstrate the concept using TCP and JSON serialization.
//!
//! Run multiple instances:
//! ```bash
//! # Terminal 1 - Node A (server)
//! cargo run --example distributed_chat -- --node node_a --port 8001
//!
//! # Terminal 2 - Node B (client connecting to A)
//! cargo run --example distributed_chat -- --node node_b --port 8002 --connect 127.0.0.1:8001
//! ```

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, Message, Pid};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

// ============================================================================
// Message Types
// ============================================================================

/// Messages that can be sent between chat actors
#[derive(Debug, Clone)]
enum ChatMessage {
    /// User joins the chat
    Join { username: String, pid: Pid },
    /// User leaves the chat
    Leave { username: String },
    /// Broadcast a text message
    Text { from: String, content: String },
    /// List all active users
    ListUsers,
}

/// Network protocol message (serializable)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum NetworkMessage {
    Join {
        node: String,
        username: String,
        pid_str: String,
    },
    Leave {
        node: String,
        username: String,
    },
    Text {
        node: String,
        from: String,
        content: String,
    },
    Heartbeat,
}

// ============================================================================
// Connection Registry
// ============================================================================

/// Maintains connections to remote nodes
#[derive(Clone)]
struct ConnectionRegistry {
    connections: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<NetworkMessage>>>>,
}

impl ConnectionRegistry {
    fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn add_connection(
        &self,
        node: String,
        sender: tokio::sync::mpsc::Sender<NetworkMessage>,
    ) {
        self.connections.write().await.insert(node, sender);
    }

    async fn remove_connection(&self, node: &str) {
        self.connections.write().await.remove(node);
    }

    async fn send_to_all(&self, msg: NetworkMessage) {
        let connections = self.connections.read().await;
        for (node, sender) in connections.iter() {
            if let Err(e) = sender.send(msg.clone()).await {
                eprintln!("Failed to send to node {}: {}", node, e);
            }
        }
    }
}

// ============================================================================
// Chat Room Actor
// ============================================================================

/// Central chat room that manages users and broadcasts messages
struct ChatRoomActor {
    node_name: String,
    users: HashMap<String, Pid>,
    registry: ConnectionRegistry,
}

impl ChatRoomActor {
    fn new(node_name: String, registry: ConnectionRegistry) -> Self {
        Self {
            node_name,
            users: HashMap::new(),
            registry,
        }
    }

    async fn handle_join(&mut self, username: String, pid: Pid) {
        println!(
            "[{}] User '{}' joined (pid: {})",
            self.node_name, username, pid
        );
        self.users.insert(username.clone(), pid);

        // Notify remote nodes
        self.registry
            .send_to_all(NetworkMessage::Join {
                node: self.node_name.clone(),
                username: username.clone(),
                pid_str: pid.to_string(),
            })
            .await;

        // Send welcome message
        let welcome = format!(
            "Welcome to the chat, {}! Type /users to see who's online.",
            username
        );
        println!("[{}] {}", self.node_name, welcome);
    }

    async fn handle_leave(&mut self, username: String) {
        if self.users.remove(&username).is_some() {
            println!("[{}] User '{}' left", self.node_name, username);

            // Notify remote nodes
            self.registry
                .send_to_all(NetworkMessage::Leave {
                    node: self.node_name.clone(),
                    username: username.clone(),
                })
                .await;
        }
    }

    async fn handle_text(&mut self, from: String, content: String) {
        println!("[{}] {}: {}", self.node_name, from, content);

        // Broadcast to remote nodes
        self.registry
            .send_to_all(NetworkMessage::Text {
                node: self.node_name.clone(),
                from: from.clone(),
                content: content.clone(),
            })
            .await;
    }

    fn handle_list_users(&self) -> Vec<String> {
        self.users.keys().cloned().collect()
    }
}

#[async_trait]
impl Actor for ChatRoomActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!(
            "[{}] Chat room started with pid {}",
            self.node_name,
            ctx.pid()
        );
    }

    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(chat_msg) = msg.downcast_ref::<ChatMessage>() {
            match chat_msg {
                ChatMessage::Join { username, pid } => {
                    self.handle_join(username.clone(), *pid).await;
                }
                ChatMessage::Leave { username } => {
                    self.handle_leave(username.clone()).await;
                }
                ChatMessage::Text { from, content } => {
                    self.handle_text(from.clone(), content.clone()).await;
                }
                ChatMessage::ListUsers => {
                    let users = self.handle_list_users();
                    println!("[{}] Active users: {:?}", self.node_name, users);
                }
            }
        } else if let Some(net_msg) = msg.downcast_ref::<NetworkMessage>() {
            // Handle messages from remote nodes
            match net_msg {
                NetworkMessage::Join { node, username, .. } => {
                    println!(
                        "[{}] Remote user '{}' joined from node '{}'",
                        self.node_name, username, node
                    );
                }
                NetworkMessage::Leave { node, username } => {
                    println!(
                        "[{}] Remote user '{}' left from node '{}'",
                        self.node_name, username, node
                    );
                }
                NetworkMessage::Text {
                    node,
                    from,
                    content,
                } => {
                    println!("[{}@{}] {}", from, node, content);
                }
                NetworkMessage::Heartbeat => {
                    // Keep-alive, do nothing
                }
            }
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, ctx: &mut ActorContext) {
        println!(
            "[{}] Chat room {} stopped: {}",
            self.node_name,
            ctx.pid(),
            reason
        );
    }
}

// ============================================================================
// Network Layer
// ============================================================================

/// Handle incoming TCP connection from remote node
async fn handle_connection(
    stream: TcpStream,
    chat_room_pid: Pid,
    system: Arc<ActorSystem>,
    registry: ConnectionRegistry,
    local_node: String,
) {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = writer;
    let mut line = String::new();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<NetworkMessage>(100);

    // Task to send outgoing messages
    let writer_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg)
                && (writer.write_all(json.as_bytes()).await.is_err()
                    || writer.write_all(b"\n").await.is_err()
                    || writer.flush().await.is_err())
            {
                break;
            }
        }
    });

    // Read incoming messages
    let remote_node = format!("remote_{}", uuid::Uuid::new_v4().as_simple());
    registry.add_connection(remote_node.clone(), tx).await;

    println!(
        "[{}] Connection established with {}",
        local_node, remote_node
    );

    while let Ok(n) = reader.read_line(&mut line).await {
        if n == 0 {
            break;
        }

        if let Ok(msg) = serde_json::from_str::<NetworkMessage>(&line) {
            // Forward network message to chat room
            if let Err(e) = system.send(chat_room_pid, Box::new(msg)).await {
                eprintln!("[{}] Failed to forward message: {}", local_node, e);
            }
        }

        line.clear();
    }

    registry.remove_connection(&remote_node).await;
    writer_task.abort();
    println!("[{}] Connection closed with {}", local_node, remote_node);
}

/// Start TCP listener for incoming connections
async fn start_server(
    port: u16,
    chat_room_pid: Pid,
    system: Arc<ActorSystem>,
    registry: ConnectionRegistry,
    node_name: String,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    println!("[{}] Listening on port {}", node_name, port);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("[{}] New connection from {}", node_name, addr);
                let pid = chat_room_pid;
                let sys = system.clone();
                let reg = registry.clone();
                let node = node_name.clone();
                tokio::spawn(async move {
                    handle_connection(stream, pid, sys, reg, node).await;
                });
            }
            Err(e) => {
                eprintln!("[{}] Connection error: {}", node_name, e);
            }
        }
    }
}

/// Connect to a remote node
async fn connect_to_node(
    address: String,
    chat_room_pid: Pid,
    system: Arc<ActorSystem>,
    registry: ConnectionRegistry,
    node_name: String,
) {
    println!("[{}] Connecting to {}", node_name, address);

    match TcpStream::connect(&address).await {
        Ok(stream) => {
            println!("[{}] Connected to {}", node_name, address);
            handle_connection(stream, chat_room_pid, system, registry, node_name).await;
        }
        Err(e) => {
            eprintln!("[{}] Failed to connect to {}: {}", node_name, address, e);
        }
    }
}

// ============================================================================
// Simulation Runner
// ============================================================================

async fn run_simulation(node_name: String, port: u16, connect_to: Option<String>) {
    println!("=== Distributed Chat Example ===");
    println!("Node: {}", node_name);
    println!();

    let system = ActorSystem::new();
    let registry = ConnectionRegistry::new();

    // Start chat room
    let chat_room = system.spawn(ChatRoomActor::new(node_name.clone(), registry.clone()));
    let chat_room_pid = chat_room.pid();

    // Start server
    let server_system = system.clone();
    let server_registry = registry.clone();
    let server_node = node_name.clone();
    tokio::spawn(async move {
        if let Err(e) = start_server(
            port,
            chat_room_pid,
            server_system,
            server_registry,
            server_node,
        )
        .await
        {
            eprintln!("Server error: {}", e);
        }
    });

    // Connect to remote node if specified
    if let Some(addr) = connect_to {
        let client_system = system.clone();
        let client_registry = registry.clone();
        let client_node = node_name.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            connect_to_node(
                addr,
                chat_room_pid,
                client_system,
                client_registry,
                client_node,
            )
            .await;
        });
    }

    // Simulate some users joining and chatting
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let user_pid = Pid::new();
    chat_room
        .send(Box::new(ChatMessage::Join {
            username: format!("user_{}", node_name),
            pid: user_pid,
        }))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    chat_room
        .send(Box::new(ChatMessage::Text {
            from: format!("user_{}", node_name),
            content: format!("Hello from {}!", node_name),
        }))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    chat_room
        .send(Box::new(ChatMessage::ListUsers))
        .await
        .unwrap();

    // Keep running for a while
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    chat_room
        .send(Box::new(ChatMessage::Leave {
            username: format!("user_{}", node_name),
        }))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    println!("\n[{}] Example completed!", node_name);
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() {
    // Parse command line arguments (simplified)
    let args: Vec<String> = std::env::args().collect();

    let mut node_name = "node_a".to_string();
    let mut port = 8001;
    let mut connect_to = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--node" => {
                if i + 1 < args.len() {
                    node_name = args[i + 1].clone();
                    i += 1;
                }
            }
            "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(8001);
                    i += 1;
                }
            }
            "--connect" => {
                if i + 1 < args.len() {
                    connect_to = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    run_simulation(node_name, port, connect_to).await;
}
