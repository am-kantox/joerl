# Distributed Actors in joerl

This document explains the distributed actor system examples in joerl and how they relate to Erlang/OTP's distributed computing model.

## Overview

Erlang/OTP has built-in support for distributed computing where actors (processes) can transparently communicate across different nodes in a cluster. While joerl doesn't have built-in distribution (yet!), the examples demonstrate the foundational concepts needed to build such a system.

## Examples

### 1. `remote_actors.rs` - Conceptual Foundation

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

### 2. `distributed_chat.rs` - Real Network Implementation

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

## Building a Production Distributed System

To build a production-ready distributed actor system with joerl, you would need:

### 1. Transport Layer

- **Multiple Protocols**: Support TCP, UDP, Unix sockets, or custom protocols
- **Connection Pooling**: Reuse connections efficiently
- **Heartbeat/Keepalive**: Detect and handle network failures
- **Encryption**: TLS/SSL for secure communication

### 2. Serialization

- **Efficient Format**: Consider bincode, MessagePack, or Protocol Buffers instead of JSON
- **Schema Evolution**: Handle version compatibility
- **Type Safety**: Ensure message types match across nodes

### 3. Node Discovery

- **DNS-based**: Use SRV records for node discovery
- **Multicast**: UDP multicast for local network discovery  
- **Consensus**: Use Raft or similar for cluster membership
- **Static Configuration**: Simple file-based node lists

### 4. Location Transparency

- **Extended Pid Format**: Include node information in PIDs
  ```rust
  struct Pid {
      node: NodeId,
      local_id: LocalPid,
  }
  ```
- **Automatic Routing**: ActorSystem routes based on Pid's node
- **Proxy Pattern**: Local proxies represent remote actors

### 5. Fault Tolerance

- **Network Partitions**: Detect and handle split-brain scenarios
- **Node Monitoring**: Detect when nodes go offline
- **Automatic Reconnection**: Retry connections with exponential backoff
- **Message Guarantees**: At-least-once or exactly-once delivery

### 6. Performance Optimizations

- **Message Batching**: Combine multiple messages into fewer network packets
- **Compression**: Compress large messages
- **Zero-copy**: Use techniques to minimize data copying
- **Backpressure**: Propagate backpressure across network boundaries

## Example: Extended Pid Format

```rust
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct DistributedPid {
    node_id: u64,      // Identifies which node
    local_pid: Pid,    // Local process ID on that node
}

impl DistributedPid {
    pub fn is_local(&self, current_node: u64) -> bool {
        self.node_id == current_node
    }
}
```

## Example: Transparent Routing

```rust
impl ActorSystem {
    pub async fn send_distributed(&self, pid: DistributedPid, msg: Message) -> Result<()> {
        if pid.is_local(self.node_id) {
            // Local send
            self.send(pid.local_pid, msg).await
        } else {
            // Remote send - serialize and forward to appropriate node
            let node = self.cluster.get_node(pid.node_id)?;
            let serialized = serialize_message(msg)?;
            node.send_bytes(pid.local_pid, serialized).await
        }
    }
}
```

## Security Considerations

When building a distributed system:

1. **Authentication**: Verify node identity before allowing connections
2. **Authorization**: Control which nodes can communicate with each other
3. **Encryption**: Use TLS to encrypt all network traffic
4. **Input Validation**: Carefully validate all incoming messages
5. **DoS Protection**: Rate limit message processing and connections

## Further Reading

- [Erlang Distribution Protocol](https://www.erlang.org/doc/apps/erts/erl_dist_protocol.html)
- [Distributed Erlang](https://learnyousomeerlang.com/distribunomicon)
- [Building Distributed Applications with Tokio](https://tokio.rs/tokio/topics/bridging)
- [Raft Consensus Algorithm](https://raft.github.io/)
- [CAP Theorem](https://en.wikipedia.org/wiki/CAP_theorem)

## Contributing

If you're interested in contributing built-in distributed actor support to joerl, please open an issue to discuss the design approach!
