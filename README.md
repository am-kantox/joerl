# joerl 🦀

[![Crates.io](https://img.shields.io/crates/v/joerl.svg)](https://crates.io/crates/joerl)
[![Documentation](https://docs.rs/joerl/badge.svg)](https://docs.rs/joerl)
[![License](https://img.shields.io/crates/l/joerl.svg)](LICENSE)

An Erlang-inspired actor model library for Rust, named in tribute to [Joe Armstrong](https://en.wikipedia.org/wiki/Joe_Armstrong_(programmer)), the creator of Erlang.

## Features

- 🎭 **Actor Model**: Lightweight actors that communicate via message passing
- 🤖 **GenServer**: Erlang's gen_server behavior with call/cast semantics
- 🔄 **GenStatem DSL**: Mermaid-based state machine definition with compile-time validation
- 🌳 **Supervision Trees**: Robust error handling with configurable restart strategies  
- 🔗 **Links & Monitors**: Actor relationships for failure detection and propagation
- 📬 **Bounded Mailboxes**: Backpressure support to prevent resource exhaustion
- 🌐 **Distributed Messaging**: Full location-transparent remote messaging with EPMD discovery
- ⚡ **Async/Await**: Built on tokio for excellent performance
- 📊 **Telemetry & Observability**: Comprehensive metrics and tracing with Prometheus/OpenTelemetry support
- 🦀 **Erlang Conventions**: Familiar API for Erlang/OTP developers

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
joerl = "0.4"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
```

## Quick Start

```rust
use joerl::{Actor, ActorContext, ActorSystem, Message};
use async_trait::async_trait;

// Define your actor
struct Counter {
    count: i32,
}

#[async_trait]
impl Actor for Counter {
    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "increment" => {
                    self.count += 1;
                    println!("Count: {}", self.count);
                },
                "get" => println!("Current count: {}", self.count),
                _ => {}
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let system = ActorSystem::new();
    let counter = system.spawn(Counter { count: 0 });
    
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("increment")).await.unwrap();
    counter.send(Box::new("get")).await.unwrap();
    
    // Give actors time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
```

## Core Concepts

### Actors

Actors are the fundamental unit of computation. Each actor:
- Has a unique `Pid` (Process ID)
- Processes messages sequentially from its mailbox
- Can spawn other actors
- Can send messages to other actors
- Can link to and monitor other actors

### Message Passing

Actors communicate by sending messages. Messages are type-erased using `Box<dyn Any>`:

```rust
actor_ref.send(Box::new("hello")).await?;
actor_ref.send(Box::new(42i32)).await?;
```

### Links and Monitors

**Links** create bidirectional relationships between actors. If one fails, both fail:

```rust
system.link(actor1.pid(), actor2.pid())?;
```

**Monitors** create unidirectional observation. The monitoring actor receives a `DOWN` signal when the monitored actor terminates:

```rust
let monitor_ref = actor_ref.monitor(my_pid)?;
```

### Supervision Trees

Supervisors monitor child actors and restart them according to strategies:

```rust
use joerl::{SupervisorSpec, RestartStrategy, ChildSpec};

let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
    .child(ChildSpec::new("worker1", || Box::new(Worker::new())))
    .child(ChildSpec::new("worker2", || Box::new(Worker::new())));

let supervisor = spawn_supervisor(&system, spec);
```

**Restart Strategies:**
- `OneForOne`: Restart only the failed child
- `OneForAll`: Restart all children when one fails
- `RestForOne`: Restart the failed child and all children started after it

### GenServer (Generic Server Behavior)

For structured stateful actors with synchronous call/reply and asynchronous cast semantics:

```rust
use joerl::gen_server::{GenServer, GenServerContext};

struct Counter;

#[derive(Debug)]
enum CounterCall {
    Get,
    Add(i32),
}

#[derive(Debug)]
enum CounterCast {
    Increment,
}

#[async_trait]
impl GenServer for Counter {
    type State = i32;
    type Call = CounterCall;
    type Cast = CounterCast;
    type CallReply = i32;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        0  // Initial state
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

// Usage
let counter = gen_server::spawn(&system, Counter);
let value = counter.call(CounterCall::Get).await?;  // Synchronous
counter.cast(CounterCast::Increment).await?;         // Asynchronous
```

### GenStatem (Generic State Machine with DSL)

For finite state machines, joerl provides a powerful DSL using Mermaid state diagrams with compile-time validation:

```rust
use joerl::{gen_statem, ActorSystem, ExitReason};
use std::sync::Arc;

#[gen_statem(fsm = r#"
    [*] --> locked
    locked --> |coin| unlocked
    locked --> |push| locked
    unlocked --> |push| locked
    unlocked --> |coin| unlocked
    unlocked --> |off| [*]
"#)]
#[derive(Debug, Clone)]
struct Turnstile {
    donations: u32,
    pushes: u32,
}

impl Turnstile {
    /// Called on every state transition
    fn on_transition(
        &mut self,
        event: TurnstileEvent,
        state: TurnstileState,
    ) -> TurnstileTransitionResult {
        match (state.clone(), event.clone()) {
            (TurnstileState::Locked, TurnstileEvent::Coin) => {
                self.donations += 1;
                TurnstileTransitionResult::Next(TurnstileState::Unlocked, self.clone())
            }
            (TurnstileState::Unlocked, TurnstileEvent::Push) => {
                self.pushes += 1;
                TurnstileTransitionResult::Next(TurnstileState::Locked, self.clone())
            }
            (TurnstileState::Unlocked, TurnstileEvent::Off) => {
                // FSM will auto-terminate on this transition
                TurnstileTransitionResult::Keep(self.clone())
            }
            _ => TurnstileTransitionResult::Keep(self.clone()),
        }
    }

    /// Called when entering a new state
    fn on_enter(
        &self,
        old_state: &TurnstileState,
        new_state: &TurnstileState,
        _data: &TurnstileData,
    ) {
        println!("Transition: {:?} -> {:?}", old_state, new_state);
    }

    /// Called on termination
    fn on_terminate(
        &self,
        reason: &ExitReason,
        state: &TurnstileState,
        data: &TurnstileData,
    ) {
        println!("Terminated in {:?}: {:?}", state, reason);
    }
}

// Usage
let system = Arc::new(ActorSystem::new());
let initial_data = Turnstile { donations: 0, pushes: 0 };
let turnstile = Turnstile(&system, initial_data);

turnstile.send(Box::new(TurnstileEvent::Coin)).await.unwrap();
turnstile.send(Box::new(TurnstileEvent::Push)).await.unwrap();
```

**Features:**
- **Mermaid Syntax**: Define FSM using standard Mermaid state diagram syntax
- **Compile-Time Validation**: FSM structure validated at compile time
- **Auto-Generated Types**: State and Event enums generated from the diagram
- **Transition Validation**: Invalid transitions detected and logged at runtime
- **Terminal States**: Automatic termination when reaching `[*]` end state
- **Callbacks**: `on_transition`, `on_enter`, and `on_terminate` hooks

The macro generates:
- `{Name}State` enum with all states
- `{Name}Event` enum with all events
- `TransitionResult` enum for transition outcomes
- Boilerplate Actor implementation with validation

### Trapping Exits

Actors can trap exit signals to handle failures gracefully:

```rust
#[async_trait]
impl Actor for MyActor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        ctx.trap_exit(true);
    }
    
    async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
        if let Signal::Exit { from, reason } = signal {
            println!("Actor {} exited: {}", from, reason);
        }
    }
}
```

## Erlang Terminology Mapping
|| Erlang | joerl | Description |
|--------|-------|-------------|
| `spawn/1` | `system.spawn(actor)` | Spawn a new actor |
| `gen_server:start_link/3` | `gen_server::spawn(&system, server)` | Spawn a gen_server |
| `gen_server:call/2` | `server_ref.call(request)` | Synchronous call |
| `gen_server:cast/2` | `server_ref.cast(message)` | Asynchronous cast |
| `gen_statem:start_link/3` | `#[gen_statem(fsm = "...")]` | Define state machine with DSL |
| `Pid` | `Pid` | Process identifier |
| `!` (send) | `actor_ref.send(msg)` | Send a message |
| `link/1` | `system.link(pid1, pid2)` | Link two actors |
| `monitor/2` | `actor_ref.monitor(from)` | Monitor an actor |
| `process_flag(trap_exit, true)` | `ctx.trap_exit(true)` | Trap exit signals |
| `{'EXIT', Pid, Reason}` | `Signal::Exit { from, reason }` | Exit signal |
| `{'DOWN', Ref, process, Pid, Reason}` | `Signal::Down { reference, pid, reason }` | Down signal |

## Examples

See the [`examples/`](examples/) directory for more examples:

- `counter.rs` - Simple counter actor
- `gen_server_counter.rs` - GenServer (gen_server behavior) example
- `turnstile.rs` - GenStatem DSL with Mermaid state diagram
- `gen_statem_turnstile.rs` - Alternative GenStatem example
- `document_workflow.rs` - Complex FSM with approval workflow and revision cycle
- `ping_pong.rs` - Two actors communicating
- `supervision_tree.rs` - Supervision tree example
- `link_monitor.rs` - Links and monitors demonstration
- **`panic_handling.rs` - Comprehensive panic handling demonstration (Erlang/OTP-style)**
- **`telemetry_demo.rs` - Telemetry and metrics with Prometheus export**
- `serialization_example.rs` - Trait-based message serialization
- `remote_actors.rs` - Distributed actors conceptual foundation
- `remote_ping_pong.rs` - Remote messaging between nodes
- `distributed_chat.rs` - Multi-node chat system over TCP
- `distributed_cluster.rs` - Multi-node cluster with EPMD discovery
- `distributed_system_example.rs` - DistributedSystem API demo
- `epmd_server.rs` - Standalone EPMD server

Run examples with:

```bash
cargo run --example counter
```

### Distributed Actors Examples

joerl now features a **unified ActorSystem** with true location transparency! The same API works for both local and distributed scenarios - just like Erlang/OTP.

**Quick Start with EPMD:**

```bash
# Terminal 1: Start EPMD server
cargo run --example epmd_server

# Terminal 2: Start first node
cargo run --example distributed_system_example -- node_a 5001

# Terminal 3: Start second node
cargo run --example distributed_system_example -- node_b 5002

# Nodes automatically discover and connect!
```

**Remote Ping-Pong Example:**

```bash
# Terminal 1: Start server node
cargo run --example remote_ping_pong -- server

# Terminal 2: Start client node
cargo run --example remote_ping_pong -- client
```

This demonstrates:
- **Unified ActorSystem**: `ActorSystem::new_distributed()` - same API as local!
- **Location Transparency**: `ctx.send()` works for both local and remote actors
- **Erlang-Style Helpers**: `nodes()`, `node()`, `is_process_alive()`, `connect_to_node()`
- **Bidirectional Connections**: Handshake protocol establishes auto-bidirectional links
- **Ping/Pong RPC**: Remote process liveness checking

**Conceptual Examples:**

The `remote_actors` example demonstrates distributed concepts using multiple local systems:

```bash
cargo run --example remote_actors
```

The `distributed_chat` example shows a real TCP-based distributed chat:

```bash
# Terminal 1
cargo run --example distributed_chat -- --node alice --port 8001

# Terminal 2
cargo run --example distributed_chat -- --node bob --port 8002 --connect 127.0.0.1:8001
```

For detailed documentation on building distributed systems with joerl, see [DISTRIBUTED.md](DISTRIBUTED.md).

## Telemetry and Observability

joerl provides comprehensive telemetry support for production monitoring:

```toml
[dependencies]
joerl = { version = "0.4", features = ["telemetry"] }
```

```rust
use joerl::telemetry;
use metrics_exporter_prometheus::PrometheusBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up Prometheus exporter
    PrometheusBuilder::new()
        .with_http_listener(([127, 0, 0, 1], 9090))
        .install()?;
    
    telemetry::init();
    
    // Metrics automatically tracked:
    // - Actor spawns, stops, panics
    // - Message throughput and latency
    // - Mailbox depth and backpressure
    // - Supervisor restarts
    
    let system = ActorSystem::new();
    // ... your code
    
    Ok(())
}
```

**Available Metrics:**
- `joerl_actors_spawned_total` - Total actors spawned
- `joerl_actors_active` - Current active actors
- `joerl_messages_sent_total` - Message throughput
- `joerl_message_processing_duration_seconds` - Processing latency
- `joerl_supervisor_restarts_total` - Supervisor restart events
- And more...

See [TELEMETRY.md](TELEMETRY.md) for comprehensive documentation and integration examples with Prometheus, Grafana, OpenTelemetry, and Datadog.

## Architecture

The library is organized into several modules:

- `actor` - Core actor trait and context
- `system` - Actor system runtime and registry
- `message` - Message types and signals
- `mailbox` - Bounded mailbox implementation
- `supervisor` - Supervision trees and restart strategies
- `telemetry` - Metrics and observability (optional)
- `error` - Error types and results
- `pid` - Process identifier

## Testing

Run the test suite:

```bash
cargo test
```

Check code coverage:

```bash
cargo tarpaulin --out Html
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Acknowledgments

This library is dedicated to the memory of [Joe Armstrong](https://en.wikipedia.org/wiki/Joe_Armstrong_(programmer)) (1950-2019), whose work on Erlang has inspired generations of developers to build robust, concurrent systems.

## See Also

- [Erlang/OTP Documentation](https://www.erlang.org/doc/)
- [The Erlang Rationale](http://www.erlang.org/doc/system_principles/system_principles.html)
- [Actors with Tokio](https://ryhl.io/blog/actors-with-tokio/)
