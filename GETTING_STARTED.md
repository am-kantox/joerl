# Getting Started with joerl

**joerl** is an Erlang-inspired actor model library for Rust, designed to make building concurrent, fault-tolerant systems as natural in Rust as it is in Erlang/OTP.

## Why joerl?

- **Familiar for Erlang developers**: Same concepts, same terminology, seamless mental model transfer
- **Production-ready**: Built-in telemetry, health monitoring, and distributed messaging
- **Well-tested**: Extensive property-based testing ensures correctness
- **Zero-cost abstractions**: Built on tokio for maximum performance

## A Simple Example: Counter Actor

Let's build a simple counter actor to demonstrate the basics:

```rust
use joerl::{Actor, ActorContext, ActorSystem, Message};
use async_trait::async_trait;

// Define your actor
struct Counter {
    count: i32,
}

#[async_trait]
impl Actor for Counter {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("Counter started with pid {}", ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "increment" => {
                    self.count += 1;
                    println!("[{}] Count: {}", ctx.pid(), self.count);
                },
                "get" => {
                    println!("[{}] Current count: {}", ctx.pid(), self.count);
                },
                "stop" => {
                    ctx.stop(joerl::ExitReason::Normal);
                },
                _ => {}
            }
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, ctx: &mut ActorContext) {
        println!("[{}] Counter stopped: {}", ctx.pid(), reason);
    }
}

#[tokio::main]
async fn main() {
    let system = ActorSystem::new();
    let counter = system.spawn(Counter { count: 0 });
    
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("get")).await.unwrap();
    counter.send(Box::new("stop")).await.unwrap();
    
    // Wait for messages to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
```

**Key concepts:**
- **Actor**: Encapsulates state (`count`) and behavior (`handle_message`)
- **ActorSystem**: Runtime that manages all actors
- **Message passing**: Type-erased messages allow any type to be sent
- **Lifecycle hooks**: `started()` and `stopped()` for initialization/cleanup

## Telemetry and Observability

One of joerl's strengths is built-in production monitoring. Enable the `telemetry` feature:

```toml
[dependencies]
joerl = { version = "0.5", features = ["telemetry"] }
metrics-exporter-prometheus = "0.15"
```

Add telemetry to your application:

```rust
use joerl::telemetry;
use metrics_exporter_prometheus::PrometheusBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Start Prometheus exporter
    PrometheusBuilder::new()
        .with_http_listener(([127, 0, 0, 1], 9090))
        .install()?;
    
    telemetry::init();
    
    let system = ActorSystem::new();
    // ... your actors
    
    Ok(())
}
```

Now visit `http://localhost:9090/metrics` to see:
- **Actor lifecycle**: spawns, stops, panics
- **Message throughput**: messages sent/processed per second
- **Mailbox depth**: backpressure indicators
- **Supervisor restarts**: fault recovery statistics

**Why it matters:**
- Zero-configuration monitoring out of the box
- Production debugging without guesswork
- Grafana/Prometheus integration ready
- OpenTelemetry support for distributed tracing

## Transparent Distribution

joerl provides location-transparent messaging: the same API works for local and remote actors.

**Start two nodes:**

```bash
# Terminal 1: Start EPMD server
cargo run --example epmd_server

# Terminal 2: Start node A
cargo run --example distributed_cluster -- node_a 5001

# Terminal 3: Start node B  
cargo run --example distributed_cluster -- node_b 5002
```

Nodes automatically discover each other via EPMD (Erlang Port Mapper Daemon).

**Code example:**

```rust
use joerl::ActorSystem;

#[tokio::main]
async fn main() {
    // Create distributed system
    let system = ActorSystem::new_distributed(
        "mynode@localhost",
        5000,
        "127.0.0.1:4369"  // EPMD address
    ).await.unwrap();
    
    // Spawn actor - works exactly like local
    let actor = system.spawn(MyActor::new());
    
    // Send message - works for local AND remote actors
    actor.send(Box::new("hello")).await.unwrap();
    
    // Connect to another node
    system.connect_to_node("othernode@localhost").await.unwrap();
    
    // Get remote actor pid and send messages transparently
    // ... same API, zero code changes!
}
```

**Key features:**
- **Same API**: `spawn()`, `send()`, `link()` work identically
- **Automatic discovery**: EPMD handles node registration
- **Bidirectional links**: Full Erlang-style connection semantics
- **Serialization**: Trait-based message serialization with global registry

This is **exactly** how Erlang works: write once, deploy anywhere.

## For Erlang/OTP Developers

If you know Erlang, you already know joerl. Here's the mapping:

| Erlang | joerl | Notes |
|--------|-------|-------|
| `spawn/1` | `system.spawn(actor)` | Spawn new process |
| `Pid ! Msg` | `actor.send(msg).await` | Send message |
| `gen_server:call/2` | `server.call(request).await` | Synchronous RPC |
| `gen_server:cast/2` | `server.cast(msg).await` | Async message |
| `link/1` | `system.link(pid1, pid2)` | Bidirectional link |
| `monitor/2` | `actor.monitor(from)` | Unidirectional monitor |
| `process_flag(trap_exit, true)` | `ctx.trap_exit(true)` | Handle failures |
| `{'EXIT', Pid, Reason}` | `Signal::Exit { from, reason }` | Exit signal |
| `supervisor:start_link/2` | `spawn_supervisor(&system, spec)` | Supervision tree |

**Example: Converting Erlang gen_server to joerl:**

Erlang:
```erlang
-module(counter).
-behaviour(gen_server).

init([]) -> {ok, 0}.

handle_call(get, _From, State) ->
    {reply, State, State};
handle_call({add, N}, _From, State) ->
    {reply, State + N, State + N}.

handle_cast(increment, State) ->
    {noreply, State + 1}.
```

joerl:
```rust
use joerl::gen_server::{GenServer, GenServerContext};

struct Counter;

#[async_trait]
impl GenServer for Counter {
    type State = i32;
    type Call = CounterCall;
    type Cast = CounterCast;
    type CallReply = i32;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        0
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) -> Self::CallReply {
        match call {
            CounterCall::Get => *state,
            CounterCall::Add(n) => {
                *state += n;
                *state
            }
        }
    }

    async fn handle_cast(
        &mut self,
        cast: Self::Cast,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        match cast {
            CounterCast::Increment => *state += 1,
        }
    }
}
```

**Migration is straightforward:**
1. Map your gen_server callbacks to trait methods
2. Use `async`/`await` where Erlang would block
3. Keep your mental model: actors, supervision, links, monitors
4. Gain Rust's type safety and performance

## Property-Based Testing: Proof of Correctness

joerl uses extensive property-based testing with QuickCheck to verify correctness. Instead of hand-written test cases, properties are defined that should hold for **all** inputs, then hundreds of random test cases are generated.

**What's tested:**

1. **Pid properties**: Serialization roundtrips, node semantics, equality
2. **Message serialization**: Lossless encoding, determinism, edge cases
3. **EPMD protocol**: All protocol messages, NodeInfo properties
4. **Actor lifecycle**: Spawn, message handling, termination
5. **Supervision**: Restart strategies, failure propagation

**Example property test:**

```rust
use quickcheck_macros::quickcheck;

/// Property: Pid serialization must be lossless
#[quickcheck]
fn prop_pid_serialization_roundtrip(pid: Pid) -> bool {
    let serialized = serde_json::to_string(&pid).unwrap();
    let deserialized: Pid = serde_json::from_str(&serialized).unwrap();
    pid == deserialized
}
```

QuickCheck generates random `Pid` values and verifies the property holds for all of them.

**Running property tests:**

```bash
# Run all property tests
cargo test --tests proptest

# Run 1000 random cases per property
QUICKCHECK_TESTS=1000 cargo test --test proptest_pid

# Run specific test
cargo test --test proptest_serialization prop_message_roundtrip
```

**Why this matters:**
- **Confidence**: Tests cover cases you'd never think of manually
- **Edge cases**: Finds corner cases (empty strings, max values, etc.)
- **Regression prevention**: Failed test cases can be saved and replayed
- **Living documentation**: Properties describe what the code guarantees

For full details, see [PROPERTY_TESTING.md](PROPERTY_TESTING.md).

## Next Steps

1. **Learn by example**: Check the [`examples/`](joerl/examples/) directory
   - `counter.rs` - Basic actor
   - `gen_server_counter.rs` - GenServer pattern
   - `supervision_tree.rs` - Fault tolerance
   - `telemetry_demo.rs` - Monitoring
   - `distributed_cluster.rs` - Multi-node systems

2. **Read the docs**: Comprehensive API documentation at [docs.rs/joerl](https://docs.rs/joerl)

3. **Understand supervision**: Erlang's "let it crash" philosophy is core to joerl

4. **Explore distribution**: See [DISTRIBUTED.md](DISTRIBUTED.md) for clustering details

5. **Monitor in production**: See [TELEMETRY.md](TELEMETRY.md) for observability setup

## Summary

joerl brings Erlang/OTP's proven concurrent programming model to Rust:

- **Simple**: Start with basic actors, add complexity as needed
- **Observable**: Built-in telemetry for production debugging
- **Distributed**: Write once, deploy across multiple nodes
- **Familiar**: Direct mapping from Erlang concepts
- **Verified**: Property-based testing ensures correctness

Whether you're coming from Erlang or new to the actor model, joerl provides a robust foundation for building fault-tolerant, distributed systems in Rust.
