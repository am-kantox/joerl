# joerl 🦀

[![Crates.io](https://img.shields.io/crates/v/joerl.svg)](https://crates.io/crates/joerl)
[![Documentation](https://docs.rs/joerl/badge.svg)](https://docs.rs/joerl)
[![License](https://img.shields.io/crates/l/joerl.svg)](LICENSE)

An Erlang-inspired actor model library for Rust, named in tribute to [Joe Armstrong](https://en.wikipedia.org/wiki/Joe_Armstrong_(programmer)), the creator of Erlang.

## Features

- 🎭 **Actor Model**: Lightweight actors that communicate via message passing
- 🌳 **Supervision Trees**: Robust error handling with configurable restart strategies  
- 🔗 **Links & Monitors**: Actor relationships for failure detection and propagation
- 📬 **Bounded Mailboxes**: Backpressure support to prevent resource exhaustion
- ⚡ **Async/Await**: Built on tokio for excellent performance
- 🦀 **Erlang Conventions**: Familiar API for Erlang/OTP developers

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
joerl = "0.1"
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

| Erlang | joerl | Description |
|--------|-------|-------------|
| `spawn/1` | `system.spawn(actor)` | Spawn a new actor |
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
- `ping_pong.rs` - Two actors communicating
- `supervision_tree.rs` - Supervision tree example
- `link_monitor.rs` - Links and monitors demonstration

Run examples with:

```bash
cargo run --example counter
```

## Architecture

The library is organized into several modules:

- `actor` - Core actor trait and context
- `system` - Actor system runtime and registry
- `message` - Message types and signals
- `mailbox` - Bounded mailbox implementation
- `supervisor` - Supervision trees and restart strategies
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
