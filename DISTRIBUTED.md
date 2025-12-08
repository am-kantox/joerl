# Distributed Actors in joerl

This document explains the distributed actor system examples in joerl and how they relate to Erlang/OTP's distributed computing model.

## Overview

Erlang/OTP has built-in support for distributed computing where actors (processes) can transparently communicate across different nodes in a cluster. **joerl now includes EPMD (Erlang Port Mapper Daemon) support for node discovery and the foundation for location-transparent distributed computing!**

### ✅ What's Implemented

- **EPMD Server**: Standalone port mapper daemon for node registry (port 4369)
- **EPMD Client**: Library for node registration, discovery, and keep-alive
- **DistributedSystem**: Node-aware actor system with EPMD integration
- **Remote Messaging**: Full location-transparent message passing between nodes ✨ NEW
- **Message Serialization**: Global registry with trait-based serialization
- **TCP Transport**: Node-to-node connection management with auto-reconnect
- **Production-Ready Examples**: Working multi-node cluster demonstrations

See [CLUSTERING.md](./CLUSTERING.md) for complete documentation and [QUICKSTART_CLUSTERING.md](./QUICKSTART_CLUSTERING.md) for getting started.

## Remote Messaging ✨ NEW

joerl now supports full remote messaging between distributed nodes with location transparency!

### Key Features

- **Trait-Based Serialization**: Messages implement `SerializableMessage` trait
- **Global Message Registry**: Register deserializers once, use everywhere
- **Automatic Sender Tracking**: Sender Pid automatically included in remote messages
- **Location Transparent**: Same `ctx.send()` API for local and remote messages
- **Type-Safe**: Compile-time message type checking
- **Lazy Registration**: Register message types in actor constructors

### Quick Example

```rust
use joerl::serialization::{SerializableMessage, SerializationError, register_message_type};
use joerl::distributed::DistributedSystem;
use joerl::{Actor, ActorContext, Message};
use async_trait::async_trait;
use std::any::Any;

// 1. Define your message
struct PingMsg { count: u32, reply_to: Pid }

// 2. Implement SerializableMessage
impl SerializableMessage for PingMsg {
    fn message_type_id(&self) -> &'static str {
        "myapp::PingMsg"  // Must be unique!
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        // Your serialization logic
        let mut bytes = self.count.to_le_bytes().to_vec();
        bytes.extend_from_slice(&self.reply_to.node().to_le_bytes());
        bytes.extend_from_slice(&self.reply_to.id().to_le_bytes());
        Ok(bytes)
    }
}

// 3. Define deserializer
fn deserialize_ping(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    // Your deserialization logic
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let node = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let id = u64::from_le_bytes([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]);
    Ok(Box::new(PingMsg { count, reply_to: Pid::with_node(node, id) }))
}

// 4. Register message type (do this once, typically in actor constructor)
register_message_type("myapp::PingMsg", Box::new(deserialize_ping));

// 5. Send messages!
struct MyActor { dist_system: Arc<DistributedSystem> }

#[async_trait]
impl Actor for MyActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        // Wrap message for serialization
        let ping = PingMsg { count: 1, reply_to: ctx.pid() };
        let msg: Message = Box::new(Box::new(ping) as Box<dyn SerializableMessage>);
        
        // Send to remote actor
        let remote_pid = Pid::with_node(remote_node_id, remote_actor_id);
        self.dist_system.send(ctx.pid(), remote_pid, msg).await.ok();
    }
}
```

### Using the `impl_serializable!` Macro

For simpler cases, use the convenience macro:

```rust
use joerl::impl_serializable;

#[derive(Debug, Clone)]
struct SimpleMsg { value: i32 }

impl_serializable!(SimpleMsg, "myapp::SimpleMsg", |msg: &SimpleMsg| {
    Ok(msg.value.to_le_bytes().to_vec())
});
```

### Message Type Guidelines

1. **Unique Type IDs**: Use namespaced identifiers like `"myapp::MessageName"`
2. **Stable Format**: Don't change serialization format after deployment
3. **Version Compatibility**: Consider adding version fields for upgrades
4. **Register Once**: Call `register_message_type()` in `Actor::started()` or globally
5. **All Messages Serializable**: All messages sent across nodes must implement `SerializableMessage`

### Complete Example

See `examples/remote_ping_pong.rs` for a complete working example:

```bash
# Terminal 1: Start server
cargo run --example remote_ping_pong -- server

# Terminal 2: Start client
cargo run --example remote_ping_pong -- client
```

## Quick Start with EPMD

```bash
# Terminal 1: Start EPMD server
cargo run --example epmd_server

# Terminal 2: Start first node
cargo run --example distributed_system_example -- node_a 5001

# Terminal 3: Start second node
cargo run --example distributed_system_example -- node_b 5002

# Nodes automatically discover each other!
```

See [CLUSTERING.md](./CLUSTERING.md) for detailed usage.

## Examples

### 1. `epmd_server.rs` - Standalone EPMD Server ✨ NEW

A production-ready EPMD (Erlang Port Mapper Daemon) server for node discovery.

**Key Features:**

- **Node Registry**: Maintains in-memory registry of all cluster nodes
- **Keep-Alive Protocol**: Automatically removes dead nodes after 60s timeout
- **Binary Protocol**: Efficient bincode serialization (compatible with Erlang's port 4369)
- **Concurrent Connections**: Handles multiple clients simultaneously
- **Health Checks**: Ping/pong protocol for monitoring

**Architecture:**

```
┌──────────────────────┐
│   EPMD Server        │
│   Port 4369          │
│                      │
│  Node Registry:      │
│  - node_a:5001      │
│  - node_b:5002      │
│  - node_c:5003      │
└──────┬───────────────┘
       │
  ┌────┴─────┐
  │   TCP    │
  │ Protocol │
  └────┬─────┘
       │
    Clients
    (Nodes)
```

**Protocol Messages:**
- `Register` - Register node with name, host, port, metadata
- `Unregister` - Remove node from registry
- `Lookup` - Find node by name
- `ListNodes` - Get all registered nodes
- `Ping/Pong` - Health check
- `KeepAlive` - Maintain registration

**Running:**

```bash
# Default (127.0.0.1:4369)
cargo run --example epmd_server

# Custom address
cargo run --example epmd_server -- 0.0.0.0:4369
```

### 2. `distributed_system_example.rs` - DistributedSystem API ✨ NEW

Demonstrates the `DistributedSystem` API for building distributed actor systems.

**Key Features:**

- **Automatic EPMD Registration**: Registers with EPMD on startup
- **Node Discovery**: Automatically discovers peer nodes
- **Connection Management**: Establishes and maintains connections to peers
- **Location Transparency**: Same API as ActorSystem
- **Graceful Shutdown**: Unregisters from EPMD on exit

**Usage:**

```rust
use joerl::distributed::DistributedSystem;

// Create distributed system
let system = DistributedSystem::new(
    "my_node",
    "127.0.0.1:5000",
    "127.0.0.1:4369"
).await?;

// Spawn actors (same API as ActorSystem)
let actor = system.system().spawn(MyActor);

// Discover other nodes
let nodes = system.list_nodes().await?;

// Connect to remote node
system.connect_to_node("other_node").await?;
```

**Running:**

```bash
# Start EPMD first
cargo run --example epmd_server

# Start nodes
cargo run --example distributed_system_example -- node_a 5001
cargo run --example distributed_system_example -- node_b 5002
```

### 3. `distributed_cluster.rs` - Multi-Node Cluster ✨ NEW

Complete example of a multi-node cluster with EPMD discovery.

**Key Features:**

- **Peer Discovery**: Nodes automatically find each other via EPMD
- **Keep-Alive Loop**: Maintains registration with 20s interval
- **Connection Pooling**: Reuses connections between nodes
- **Cluster Awareness**: Each node knows about all peers

**Running:**

```bash
# Terminal 1
cargo run --example epmd_server

# Terminals 2, 3, 4
cargo run --example distributed_cluster -- node_a 5001
cargo run --example distributed_cluster -- node_b 5002
cargo run --example distributed_cluster -- node_c 5003
```

### 4. `remote_actors.rs` - Conceptual Foundation

This example demonstrates the core concepts of distributed actors using multiple `ActorSystem` instances to simulate different nodes.

**Key Concepts Demonstrated:**

- **Node Registry**: A discovery mechanism that maps node names to their actor systems and proxy PIDs
- **Node Proxy Actor**: An entry point for each node that receives and routes messages
- **Serializable Messages**: Messages that can cross node boundaries (using `serde`)
- **Location Transparency**: Remote actors behave like local actors from the caller's perspective

**Architecture:**

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│   Node A    │         │   Node B    │         │   Node C    │
│             │         │             │         │             │
│ ┌─────────┐ │         │ ┌─────────┐ │         │ ┌─────────┐ │
│ │ Proxy A │◄├─────────┤►│ Proxy B │◄├─────────┤►│ Proxy C │ │
│ └────┬────┘ │         │ └────┬────┘ │         │ └────┬────┘ │
│      │      │         │      │      │         │      │      │
│ ┌────▼────┐ │         │ ┌────▼────┐ │         │ ┌────▼────┐ │
│ │Workers  │ │         │ │Workers  │ │         │ │Workers  │ │
│ └─────────┘ │         │ └─────────┘ │         │ └─────────┘ │
└─────────────┘         └─────────────┘         └─────────────┘
```

**Running:**

```bash
cargo run --example remote_actors
```

**Output:**

The example shows:
- Three nodes (A, B, C) being created
- Remote message passing between nodes
- Request-reply patterns across nodes
- Echo and compute operations demonstrating different message types

### 5. `distributed_chat.rs` - Real Network Implementation

This example implements a distributed chat system using TCP networking, demonstrating how joerl actors can communicate across actual network boundaries.

**Key Features:**

- **TCP Transport**: Real network communication using tokio's TcpListener and TcpStream
- **JSON Serialization**: Messages are serialized to JSON for network transmission
- **Connection Management**: Handles incoming and outgoing connections
- **Multi-node Chat**: Multiple nodes can join and exchange messages

**Architecture:**

```
┌──────────────────────────────────────────────────────────┐
│                    Connection Registry                    │
│         (Maps node names to network connections)          │
└────────────┬─────────────────────────────┬───────────────┘
             │                             │
      ┌──────▼──────┐              ┌──────▼──────┐
      │   Node A    │              │   Node B    │
      │  Port 8001  │◄────TCP─────►│  Port 8002  │
      │             │              │             │
      │ ChatRoom    │              │ ChatRoom    │
      │   Actor     │              │   Actor     │
      └─────────────┘              └─────────────┘
           │                             │
      user_alice                    user_bob
```

**Running:**

```bash
# Terminal 1 - Start first node
cargo run --example distributed_chat -- --node alice --port 8001

# Terminal 2 - Start second node (connects to first)
cargo run --example distributed_chat -- --node bob --port 8002 --connect 127.0.0.1:8001
```

**Message Flow:**

1. User joins chat → Local ChatRoom actor handles it
2. ChatRoom actor serializes join event to JSON
3. Event sent to all connected remote nodes via TCP
4. Remote nodes deserialize and display the event
5. Process repeats for text messages and other events

## Erlang/OTP Comparison

### In Erlang/OTP

Distributed communication is transparent:

```erlang
% Send to remote actor
{some_actor, 'node@hostname'} ! {message, Data}.

% Spawn on remote node
RemotePid = spawn('node@hostname', module, function, [Args]).

% Link to remote actor
link(RemotePid).
```

### In joerl (Current Examples)

Distribution requires explicit handling:

```rust
// Get reference to remote node
if let Some((system, proxy_pid)) = registry.get_node("node_b").await {
    // Serialize message
    let msg = RemoteMessage::Echo {
        content: "Hello".to_string(),
        reply_to: "node_a".to_string(),
    };
    
    // Send to remote proxy
    system.send(proxy_pid, Box::new(msg)).await?;
}
```

## EPMD Implementation Details

### Architecture

The distributed system follows a layered architecture:

```
┌────────────────────────────────────────┐
│  Application Layer (Your Actors)      │
├────────────────────────────────────────┤
│  DistributedSystem (Location Trans.)  │
├────────────────────────────────────────┤
│  ActorSystem (Local Runtime)          │
├────────────────────────────────────────┤
│  EPMD Client/Server (Discovery)       │
├────────────────────────────────────────┤
│  TCP Transport (Node-to-Node)         │
└────────────────────────────────────────┘
```

### Node Discovery Flow

1. Node starts → Create `DistributedSystem`
2. Extract host/port from listen address
3. Connect to EPMD server
4. Register node with EPMD (name, host, port, metadata)
5. Start keep-alive loop (20s interval)
6. Start TCP listener for incoming connections
7. Periodically query EPMD for peers
8. Connect to discovered nodes

### Pid with Node Support

Pids now include node information:

```rust
#[derive(Serialize, Deserialize)]
pub struct Pid {
    node: u32,    // Node ID (0 for local, hash of name for remote)
    id: u64,      // Unique ID within node
}

// Create local Pid
let local = Pid::new();  // node=0

// Create remote Pid
let remote = Pid::with_node(42, 100);  // node=42, id=100

// Check if local
assert!(local.is_local());
assert!(!remote.is_local());

// Display format shows node
println!("{}", local);   // <0.123.0>
println!("{}", remote);  // <42.100.0>
```

### Connection Management

```rust
// NodeRegistry maintains connection pool
struct NodeRegistry {
    connections: Arc<DashMap<String, Arc<NodeConnection>>>,
}

// Each connection handles automatic reconnection
struct NodeConnection {
    node_info: NodeInfo,
    stream: RwLock<Option<TcpStream>>,
}

// Reconnects automatically on send failure
if stream_guard.is_none() {
    *stream_guard = Some(TcpStream::connect(&addr).await?);
}
```

## Building on the Foundation

The EPMD implementation provides the infrastructure. To build a complete distributed system, you can extend with:

### 1. Message Serialization Strategy ⚠️ User Choice

The current implementation provides transport but leaves message serialization to users:

**Option 1: User-level serialization**
```rust
let data = serde_json::to_vec(&my_message)?;
remote_actor.send(Box::new(data)).await?;
```

**Option 2: Trait-based approach**
```rust
pub trait RemoteMessage: Any + Send + Serialize + Deserialize {}
```

**Option 3: Message envelope**
```rust
#[derive(Serialize, Deserialize)]
enum RemoteCommand {
    Echo(String),
    Compute(i32, i32),
}
```

### 2. Additional Features You Can Add

- **Remote Actor Spawn**: Spawn actors on specific nodes
- **Global Registry**: Name → Pid mapping across cluster
- **Remote Links/Monitors**: Distributed supervision
- **Security**: TLS, authentication, encryption
- **Alternative Discovery**: DNS, multicast, Consul, etcd

### 3. Advanced Patterns

- **Proxy Pattern**: Local proxies for remote actors
- **Message Batching**: Combine messages for efficiency
- **Compression**: Compress large payloads
- **Backpressure**: Propagate across network

## Current Implementation Status

### ✅ Fully Implemented

- **EPMD Server**: Complete standalone server with node registry
- **EPMD Client**: Registration, lookup, keep-alive
- **DistributedSystem**: Node-aware actor system wrapper
- **Node Discovery**: Automatic peer discovery via EPMD
- **Connection Management**: TCP transport with auto-reconnect
- **Pid Serialization**: Pids with node support
- **Keep-Alive Protocol**: Automatic dead node removal
- **Production Examples**: Working multi-node demonstrations

### 🚧 Infrastructure Ready (User Implementation)

- **Remote Messaging**: Transport ready, serialization strategy is user choice
- **Message Routing**: Can be built on NetworkMessage infrastructure
- **Remote Spawn**: Foundation in place

### 🔮 Future Enhancements

- **Global Registry**: Cluster-wide name → Pid mapping
- **Remote Links/Monitors**: Distributed supervision
- **TLS Support**: Encrypted node-to-node communication
- **Alternative Discovery**: DNS, multicast, Consul, etcd

## Example: Using DistributedSystem

```rust
use joerl::distributed::DistributedSystem;
use joerl::{Actor, ActorContext, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create distributed system
    let system = DistributedSystem::new(
        "my_node",              // Node name
        "127.0.0.1:5000",       // Listen address
        "127.0.0.1:4369"        // EPMD address
    ).await?;
    
    // Spawn actors (same API as ActorSystem)
    let actor = system.system().spawn(MyActor);
    
    // Discover peers
    let nodes = system.list_nodes().await?;
    println!("Cluster nodes: {:?}", nodes);
    
    // Connect to specific node
    system.connect_to_node("other_node").await?;
    
    // Local messaging works as before
    actor.send(Box::new("hello")).await?;
    
    // Graceful shutdown
    system.shutdown().await?;
    
    Ok(())
}
```

## Security Considerations

When building a distributed system:

1. **Authentication**: Verify node identity before allowing connections
2. **Authorization**: Control which nodes can communicate with each other
3. **Encryption**: Use TLS to encrypt all network traffic
4. **Input Validation**: Carefully validate all incoming messages
5. **DoS Protection**: Rate limit message processing and connections

## Documentation

- **[CLUSTERING.md](./CLUSTERING.md)** - Complete clustering guide with architecture, API reference, best practices
- **[QUICKSTART_CLUSTERING.md](./QUICKSTART_CLUSTERING.md)** - 5-minute getting started tutorial
- **[IMPLEMENTATION_SUMMARY.md](./IMPLEMENTATION_SUMMARY.md)** - Technical implementation details and design decisions

## Further Reading

- [Erlang Distribution Protocol](https://www.erlang.org/doc/apps/erts/erl_dist_protocol.html)
- [Distributed Erlang](https://learnyousomeerlang.com/distribunomicon)
- [EPMD - Erlang Port Mapper Daemon](https://www.erlang.org/doc/man/epmd.html)
- [Building Distributed Applications with Tokio](https://tokio.rs/tokio/topics/bridging)
- [Raft Consensus Algorithm](https://raft.github.io/)
- [CAP Theorem](https://en.wikipedia.org/wiki/CAP_theorem)

## Contributing

The distributed clustering foundation is now in place! Contributions welcome for:

- Advanced remote messaging patterns
- Alternative discovery mechanisms (DNS, multicast, Consul)
- Security features (TLS, authentication)
- Global registry implementation
- Performance optimizations
- Additional examples and documentation

Please open an issue to discuss your ideas!
