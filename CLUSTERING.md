# Distributed Clustering with EPMD

joerl implements an EPMD (Erlang Port Mapper Daemon) equivalent for node discovery and location transparency in distributed actor systems.

## Overview

Like Erlang/OTP, joerl provides:

- **EPMD Server**: Standalone service maintaining node registry
- **EPMD Client**: Library for node registration and discovery
- **Protocol**: Simple, efficient binary protocol
- **Custom Implementations**: Public protocol allowing custom EPMD servers

## Quick Start

### 1. Start EPMD Server

```bash
cargo run --example epmd_server
```

This starts the EPMD server on `127.0.0.1:4369` (the standard EPMD port used by Erlang).

### 2. Register a Node

```rust
use joerl::epmd::EpmdClient;

#[tokio::main]
async fn main() {
    let epmd = EpmdClient::new("127.0.0.1:4369");
    
    // Register this node
    epmd.register("my_node", "127.0.0.1", 5000).await.unwrap();
    
    // Start keep-alive to prevent timeout
    epmd.start_keep_alive_loop("my_node".to_string(), Duration::from_secs(20)).await;
}
```

### 3. Discover Other Nodes

```rust
// Lookup a specific node
if let Some(node_info) = epmd.lookup("other_node").await.unwrap() {
    println!("Found node at {}:{}", node_info.host, node_info.port);
}

// List all registered nodes
let nodes = epmd.list_nodes().await.unwrap();
for node in nodes {
    println!("Node: {} at {}", node.name, node.address());
}
```

## Complete Example

Run the distributed cluster example:

### Terminal 1: Start EPMD
```bash
cargo run --example epmd_server
```

### Terminal 2: Start Node A
```bash
cargo run --example distributed_cluster -- node_a 5001
```

### Terminal 3: Start Node B
```bash
cargo run --example distributed_cluster -- node_b 5002
```

### Terminal 4: Start Node C
```bash
cargo run --example distributed_cluster -- node_c 5003
```

Each node will:
- Register with EPMD
- Discover other nodes automatically
- Send keep-alive messages
- Demonstrate location-transparent communication

## Architecture

### EPMD Server

The EPMD server is a TCP service that:

- Listens on port 4369 (configurable)
- Maintains an in-memory registry of nodes
- Handles concurrent client connections
- Removes dead nodes after keep-alive timeout (default: 60 seconds)
- Uses a simple length-prefixed binary protocol

### EPMD Protocol

Messages are serialized using `bincode` with the following structure:

```
[4 bytes: message length (big-endian)] [N bytes: bincode-serialized EpmdMessage]
```

#### Protocol Messages

**Registration:**
```rust
Register { name, host, port, metadata } -> RegisterOk | RegisterError
Unregister { name } -> UnregisterOk
```

**Discovery:**
```rust
Lookup { name } -> LookupResult { node: Option<NodeInfo> }
ListNodes -> NodeList { nodes: Vec<NodeInfo> }
```

**Health:**
```rust
Ping -> Pong
KeepAlive { name } -> KeepAliveAck
```

### Node Information

Each registered node stores:

```rust
pub struct NodeInfo {
    pub name: String,           // Node name (e.g., "node1")
    pub host: String,           // Host address
    pub port: u16,              // Port for connections
    pub metadata: NodeMetadata, // Custom metadata
}

pub struct NodeMetadata {
    pub protocol_version: u32,
    pub capabilities: Vec<String>,
    pub attributes: HashMap<String, String>,
}
```

## Custom EPMD Implementations

The EPMD protocol is public, allowing custom implementations:

```rust
use joerl::epmd::{EpmdMessage, NodeInfo};

// Create your own EPMD server
// - Use different storage (Redis, etcd, Consul)
// - Add authentication/authorization
// - Implement custom discovery logic
// - Add monitoring and metrics
```

### Example: Custom Storage Backend

```rust
struct RedisEpmdServer {
    redis: redis::Client,
}

impl RedisEpmdServer {
    async fn handle_register(&self, name: String, info: NodeInfo) {
        // Store in Redis with TTL
        self.redis.set_ex(format!("node:{}", name), info, 60).await;
    }
    
    async fn handle_lookup(&self, name: String) -> Option<NodeInfo> {
        self.redis.get(format!("node:{}", name)).await
    }
}
```

## Comparison with Erlang EPMD

### Similarities
- Same default port (4369)
- Node registration and discovery
- Keep-alive mechanism
- Location transparency goal

### Differences
- **Protocol**: joerl uses bincode instead of Erlang's binary term format
- **Implementation**: Pure Rust vs C
- **Extensibility**: joerl protocol is fully documented for custom implementations
- **Metadata**: joerl supports structured metadata for nodes

## Future Enhancements

### Planned Features

1. **Location Transparency** (In Progress)
   - Extend `Pid` with node information
   - Transparent remote message sending
   - Automatic connection management

2. **Node Transport Layer**
   - TCP connections between nodes
   - Message serialization for network transport
   - Connection pooling
   - Fault tolerance and reconnection

3. **Security**
   - TLS support for EPMD connections
   - Node authentication
   - Message encryption for inter-node communication

4. **Monitoring**
   - Health checks
   - Metrics and statistics
   - Dead node detection

5. **Alternative Discovery**
   - DNS-based discovery
   - Multicast discovery
   - Integration with service meshes (Consul, etcd, etc.)

## Best Practices

### Production Deployment

1. **Run EPMD as a Service**
   ```bash
   # systemd unit file
   [Unit]
   Description=joerl EPMD Server
   After=network.target

   [Service]
   Type=simple
   ExecStart=/path/to/epmd_server 0.0.0.0:4369
   Restart=always

   [Install]
   WantedBy=multi-user.target
   ```

2. **Configure Keep-Alive Timeouts**
   ```rust
   let server = EpmdServer::new("0.0.0.0:4369")
       .with_keep_alive_timeout(Duration::from_secs(30));
   ```

3. **Handle Network Partitions**
   - Implement reconnection logic
   - Use timeouts for discovery operations
   - Monitor EPMD health

4. **Secure EPMD**
   - Use firewall rules to restrict EPMD access
   - Run EPMD on private networks
   - Implement authentication for node registration

### Node Implementation

1. **Always Start Keep-Alive**
   ```rust
   epmd.start_keep_alive_loop(node_name, Duration::from_secs(20)).await;
   ```

2. **Graceful Shutdown**
   ```rust
   // On shutdown
   epmd.unregister(node_name).await.ok();
   ```

3. **Error Handling**
   ```rust
   match epmd.register(name, host, port).await {
       Ok(_) => info!("Registered successfully"),
       Err(EpmdError::NodeAlreadyRegistered(_)) => {
           // Unregister and retry
           epmd.unregister(name).await.ok();
           epmd.register(name, host, port).await?;
       }
       Err(e) => error!("Registration failed: {}", e),
   }
   ```

## API Reference

### EpmdServer

```rust
impl EpmdServer {
    pub fn new(address: impl Into<String>) -> Self;
    pub fn with_keep_alive_timeout(self, timeout: Duration) -> Self;
    pub async fn run(self) -> Result<()>;
}
```

### EpmdClient

```rust
impl EpmdClient {
    pub fn new(epmd_address: impl Into<String>) -> Self;
    
    // Registration
    pub async fn register(&self, name: impl Into<String>, 
                          host: impl Into<String>, 
                          port: u16) -> Result<()>;
    pub async fn unregister(&self, name: impl Into<String>) -> Result<()>;
    
    // Discovery
    pub async fn lookup(&self, name: impl Into<String>) -> Result<Option<NodeInfo>>;
    pub async fn list_nodes(&self) -> Result<Vec<NodeInfo>>;
    
    // Health
    pub async fn ping(&self) -> Result<()>;
    pub async fn keep_alive(&self, name: impl Into<String>) -> Result<()>;
    pub async fn start_keep_alive_loop(&self, name: String, 
                                       interval: Duration) -> JoinHandle<()>;
}
```

## Examples

All examples are in the `examples/` directory:

- `epmd_server.rs` - Standalone EPMD server
- `distributed_cluster.rs` - Multi-node cluster with discovery
- `remote_actors.rs` - Remote actor communication (conceptual)

Run with:
```bash
cargo run --example <example_name>
```

## Testing

Run EPMD tests:
```bash
cargo test epmd
```

Integration tests:
```bash
# Start EPMD in one terminal
cargo run --example epmd_server

# Run tests in another terminal
cargo test --test epmd_integration
```

## Troubleshooting

### "Connection failed" errors
- Ensure EPMD server is running
- Check firewall rules
- Verify correct port (default: 4369)

### "Node already registered" errors
- A node with that name is already registered
- Unregister first, or use a different name
- Check if a previous instance crashed without unregistering

### Keep-alive failures
- Network connectivity issues
- EPMD server restarted
- Keep-alive interval too long (increase frequency)

### Node not found in discovery
- Node hasn't registered yet
- Registration failed silently (check logs)
- EPMD server restarted (re-register)

## Contributing

To add features to EPMD:

1. Protocol changes go in `src/epmd.rs`
2. Server implementation in `src/epmd/server.rs`
3. Client implementation in `src/epmd/client.rs`
4. Add tests for new functionality
5. Update this documentation

## License

Same as joerl - see LICENSE file.
