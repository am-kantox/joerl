# Quick Start: Distributed Clustering

Get your distributed joerl cluster running in 5 minutes!

## 1. Start EPMD Server

In one terminal:

```bash
cd /path/to/joerl
cargo run --example epmd_server
```

You should see:
```
========================================
      joerl EPMD Server v0.3.0
========================================

 INFO  EPMD server listening on 127.0.0.1:4369
```

## 2. Start First Node

In a second terminal:

```bash
cargo run --example distributed_cluster -- node_a 5001
```

Output:
```
========================================
     Distributed Cluster Example
========================================

 INFO  [node_a] Registered with EPMD on port 5001
 INFO  [node_a] Spawning local actors...
 INFO  [node_a] Worker-1 started (pid: ...)
 INFO  [node_a] Worker-2 started (pid: ...)
 INFO  [node_a] Coordinator started (pid: ...)
```

## 3. Start Second Node

In a third terminal:

```bash
cargo run --example distributed_cluster -- node_b 5002
```

Node B will discover Node A automatically!

```
 INFO  [node_b] Registered with EPMD on port 5002
 INFO  [node_b] Discovered 1 peer(s): ["node_a"]
 INFO  [node_b] Active cluster: node_b + 1 peer(s)
```

## 4. Add More Nodes

Keep adding nodes:

```bash
# Terminal 4
cargo run --example distributed_cluster -- node_c 5003

# Terminal 5
cargo run --example distributed_cluster -- node_d 5004
```

Each new node automatically discovers all existing nodes!

## What's Happening?

1. **EPMD Server** (Terminal 1)
   - Maintains registry of all nodes
   - Listens on port 4369 (Erlang standard)
   - Removes dead nodes after timeout

2. **Each Node** (Terminals 2+)
   - Registers with EPMD on startup
   - Sends keep-alive every 20 seconds
   - Queries EPMD every 5 seconds for peers
   - Spawns local actors

3. **Discovery Flow**
   ```
   Node → Register with EPMD
   Node → Start keep-alive loop
   Node → Query EPMD for peers
   EPMD → Returns list of all nodes
   Node → Discovers peers automatically
   ```

## Next Steps

### Custom Node Implementation

Create your own distributed node:

```rust
use joerl::epmd::{EpmdClient, DEFAULT_EPMD_PORT};
use joerl::ActorSystem;

#[tokio::main]
async fn main() {
    // Create EPMD client
    let epmd = EpmdClient::new(format!("127.0.0.1:{}", DEFAULT_EPMD_PORT));
    
    // Register this node
    let node_name = "my_custom_node";
    let port = 6000;
    epmd.register(node_name, "127.0.0.1", port)
        .await
        .expect("Failed to register");
    
    // Start keep-alive
    epmd.start_keep_alive_loop(
        node_name.to_string(), 
        Duration::from_secs(20)
    ).await;
    
    // Create actor system
    let system = ActorSystem::new();
    
    // Spawn actors...
    // Discover peers...
    // Send messages...
}
```

### Production Deployment

1. **Run EPMD as systemd service**
   ```bash
   sudo systemctl enable joerl-epmd
   sudo systemctl start joerl-epmd
   ```

2. **Use environment variables for configuration**
   ```rust
   let epmd_addr = env::var("EPMD_ADDRESS")
       .unwrap_or_else(|_| "127.0.0.1:4369".to_string());
   ```

3. **Handle graceful shutdown**
   ```rust
   // On Ctrl+C
   epmd.unregister(node_name).await.ok();
   ```

## Troubleshooting

### Node can't connect to EPMD

```bash
# Check EPMD is running
netstat -an | grep 4369

# Try pinging EPMD
cargo run --example epmd_server &
# In Rust code:
epmd.ping().await.expect("EPMD not responding");
```

### Node not discovering peers

Check registration worked:
```rust
let nodes = epmd.list_nodes().await?;
println!("Registered nodes: {:?}", nodes);
```

### Keep-alive failures

Increase frequency:
```rust
// From 20s to 10s
epmd.start_keep_alive_loop(name, Duration::from_secs(10)).await;
```

## Architecture Overview

```
┌─────────────────┐
│  EPMD Server    │
│  (Port 4369)    │
│                 │
│  Registry:      │
│  - node_a:5001  │
│  - node_b:5002  │
│  - node_c:5003  │
└────────┬────────┘
         │
    ┌────┴────┐
    │  TCP    │
    │ Proto   │
    └────┬────┘
         │
    ┌────┴────────────────────────┐
    │                             │
┌───▼────┐  ┌────────┐  ┌────────▼──┐
│ node_a │  │ node_b │  │  node_c   │
│:5001   │  │:5002   │  │  :5003    │
│        │  │        │  │           │
│ Actors │  │ Actors │  │  Actors   │
└────────┘  └────────┘  └───────────┘
```

## What You Have Now

✅ **EPMD Server** - Node registry and discovery  
✅ **EPMD Client** - Registration and lookup API  
✅ **Protocol** - Binary protocol (bincode)  
✅ **Keep-Alive** - Automatic dead node removal  
✅ **Examples** - Working cluster demo  

## What's Next

The foundation for distributed clustering is complete! Next steps for full location transparency:

1. **Node Transport** - TCP connections between nodes
2. **Remote Pids** - Extend Pid with node information
3. **Message Routing** - Transparent local/remote sending
4. **Serialization** - Network-safe message format
5. **Connection Pooling** - Reuse connections efficiently

See [CLUSTERING.md](./CLUSTERING.md) for the full roadmap!

## Learn More

- **Full Documentation**: [CLUSTERING.md](./CLUSTERING.md)
- **Examples**: `examples/distributed_cluster.rs`
- **API Reference**: Run `cargo doc --open`
- **Tests**: `cargo test epmd`

Happy clustering! 🚀
