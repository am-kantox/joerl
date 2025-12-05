# joerl vs actix: Comprehensive Comparison

## Executive Summary

This document provides a detailed comparison between **joerl** (an Erlang/OTP-inspired actor library) and **actix** (the established Rust actor framework). Both frameworks provide actor model implementations but with different philosophies, APIs, and target use-cases.

## Table of Contents

- [Overview](#overview)
- [Design Philosophy](#design-philosophy)
- [API Comparison](#api-comparison)
- [Feature Matrix](#feature-matrix)
- [Performance Benchmarks](#performance-benchmarks)
- [Use Case Recommendations](#use-case-recommendations)
- [Migration Guide](#migration-guide)

---

## Overview

### joerl

- **Version**: 0.1.0 (early development)
- **Philosophy**: Faithful Erlang/OTP semantics in Rust
- **Inspiration**: Named after Joe Armstrong, creator of Erlang
- **Target**: Developers familiar with Erlang/OTP patterns
- **Status**: New library, experimental

### actix

- **Version**: 0.13.5 (mature)
- **Philosophy**: Pragmatic Rust-native actor framework
- **Inspiration**: Akka (Scala/JVM ecosystem)
- **Target**: General Rust developers, web applications
- **Status**: Production-ready, passively maintained

---

## Design Philosophy

### joerl: Erlang/OTP Fidelity

**Core Principles:**
- Mirror Erlang/OTP semantics as closely as possible
- Provide familiar terminology (`Pid`, `gen_server`, `gen_statem`, `supervisor`)
- Support dynamic message types via `Box<dyn Any>`
- Focus on fault tolerance with supervision trees
- Explicit state management patterns

**Trade-offs:**
- Less type safety (dynamic messages via `Any`)
- Requires understanding of Erlang patterns
- Simpler mental model for Erlang developers
- More verbose API

### actix: Rust-Native Pragmatism

**Core Principles:**
- Type-safe messages via trait implementation
- Zero-cost abstractions where possible
- Minimal runtime overhead
- Integrate naturally with Rust ecosystem
- Optimize for common use cases

**Trade-offs:**
- More type annotations required
- Different mental model from Erlang
- Steeper learning curve for Erlang developers
- More efficient for statically-typed workflows

---

## API Comparison

### Basic Actor Definition

#### joerl (Erlang-style)

```rust
use joerl::{Actor, ActorContext, ActorSystem, Message};
use async_trait::async_trait;

struct Counter {
    count: i32,
}

#[async_trait]
impl Actor for Counter {
    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "increment" => self.count += 1,
                "decrement" => self.count -= 1,
                _ => {}
            }
        }
    }
}

// Spawn and send
let system = ActorSystem::new();
let counter = system.spawn(Counter { count: 0 });
counter.send(Box::new("increment")).await?;
```

**Characteristics:**
- Dynamic message type (`Box<dyn Any>`)
- Runtime type checking via downcasting
- Single message handler method
- Familiar to Erlang developers

#### actix (Typed messages)

```rust
use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "()")]
struct Increment;

struct Counter {
    count: i32,
}

impl Actor for Counter {
    type Context = Context<Self>;
}

impl Handler<Increment> for Counter {
    type Result = ();
    
    fn handle(&mut self, _msg: Increment, _ctx: &mut Context<Self>) {
        self.count += 1;
    }
}

// Spawn and send
let addr = Counter { count: 0 }.start();
addr.do_send(Increment);
```

**Characteristics:**
- Statically typed messages
- Compile-time type checking
- Separate handler per message type
- Better tooling support

### GenServer / Typed Handlers

#### joerl (gen_server)

```rust
use joerl::gen_server::{GenServer, GenServerContext};

struct CounterServer;

#[derive(Debug)]
enum Call { Get, Add(i32) }

#[derive(Debug)]
enum Cast { Increment }

#[async_trait]
impl GenServer for CounterServer {
    type State = i32;
    type Call = Call;
    type Cast = Cast;
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
            Call::Get => *state,
            Call::Add(n) => {
                *state += n;
                *state
            }
        }
    }

    async fn handle_cast(
        &mut self,
        _cast: Self::Cast,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        *state += 1;
    }
}

// Usage
let server = joerl::gen_server::spawn(&system, CounterServer);
let value = server.call(Call::Get).await?;
server.cast(Cast::Increment).await?;
```

**Characteristics:**
- Explicit call/cast separation (Erlang convention)
- Type-safe within the gen_server abstraction
- State managed explicitly
- Direct Erlang gen_server translation

#### actix (Multiple handlers)

```rust
use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "i32")]
struct Get;

#[derive(Message)]
#[rtype(result = "i32")]
struct Add(i32);

#[derive(Message)]
#[rtype(result = "()")]
struct Increment;

struct Counter {
    count: i32,
}

impl Actor for Counter {
    type Context = Context<Self>;
}

impl Handler<Get> for Counter {
    type Result = i32;
    fn handle(&mut self, _msg: Get, _ctx: &mut Context<Self>) -> Self::Result {
        self.count
    }
}

impl Handler<Add> for Counter {
    type Result = i32;
    fn handle(&mut self, msg: Add, _ctx: &mut Context<Self>) -> Self::Result {
        self.count += msg.0;
        self.count
    }
}

impl Handler<Increment> for Counter {
    type Result = ();
    fn handle(&mut self, _msg: Increment, _ctx: &mut Context<Self>) {
        self.count += 1;
    }
}

// Usage
let addr = Counter { count: 0 }.start();
let value = addr.send(Get).await?;
addr.do_send(Increment);
```

**Characteristics:**
- No call/cast distinction (unified via return type)
- Each message type is a separate handler
- More boilerplate per message type
- Better IDE support and refactoring

### Supervision

#### joerl (Erlang supervisor)

```rust
use joerl::supervisor::{ChildSpec, RestartStrategy, SupervisorSpec, spawn_supervisor};

let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
    .child(ChildSpec::new("worker1", || Box::new(Worker { id: 1 })))
    .child(ChildSpec::new("worker2", || Box::new(Worker { id: 2 })));

let supervisor = spawn_supervisor(&system, spec);
```

**Characteristics:**
- Explicit restart strategies (OneForOne, OneForAll, RestForOne)
- Named children
- Factory functions for child creation
- Direct Erlang supervisor translation

#### actix (Supervisor trait)

```rust
use actix::prelude::*;

impl Supervised for MyActor {}

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Supervision logic
    }
}
```

**Characteristics:**
- Supervision via trait implementation
- Less explicit structure
- More flexible but requires more manual setup
- Per-actor supervision configuration

---

## Feature Matrix

| Feature | joerl | actix | Notes |
|---------|-------|-------|-------|
| **Core Features** |
| Basic actors | ✅ | ✅ | Both support async actors |
| Message passing | ✅ | ✅ | joerl: dynamic, actix: typed |
| Actor lifecycle hooks | ✅ | ✅ | started, stopping, stopped |
| Actor addressing | ✅ (Pid) | ✅ (Addr) | Different abstractions |
| **Advanced Patterns** |
| GenServer behavior | ✅ | ❌ | joerl: explicit, actix: via handlers |
| GenStatem (FSM) | ✅ | ❌ | joerl only (Erlang pattern) |
| Supervision trees | ✅ | ✅ | joerl: more structured |
| Links | ✅ | ❌ | joerl only (Erlang concept) |
| Monitors | ✅ | ❌ | joerl only (Erlang concept) |
| **Message Handling** |
| Typed messages | ⚠️ | ✅ | joerl: dynamic, actix: compile-time |
| Generic messages | ✅ | ✅ | Both support |
| Async message handlers | ✅ | ✅ | Both use async/await |
| Synchronous calls | ✅ | ✅ | Both support request/reply |
| Fire-and-forget | ✅ | ✅ | cast/do_send |
| **Fault Tolerance** |
| Automatic restarts | ✅ | ✅ | Via supervisors |
| Restart strategies | ✅ | ⚠️ | joerl: explicit, actix: manual |
| Trap exit signals | ✅ | ❌ | joerl only (Erlang pattern) |
| Escalation | ⚠️ | ⚠️ | Both: manual implementation |
| **Distribution** |
| Multi-node support | ⚠️ | ❌ | joerl: examples only |
| Remote actors | ⚠️ | ❌ | Not production-ready in either |
| Serialization | ⚠️ | ❌ | Custom implementation needed |
| **Performance** |
| Bounded mailboxes | ✅ | ✅ | Both support backpressure |
| Zero-copy messages | ❌ | ⚠️ | actix: better optimization |
| Compile-time optimization | ⚠️ | ✅ | actix: more inlining |
| **Ecosystem** |
| Web framework integration | ❌ | ✅ | actix-web |
| Streaming | ❌ | ✅ | actix: StreamHandler |
| Timers | ⚠️ | ✅ | actix: built-in timers |
| Registry | ❌ | ✅ | actix: SystemService |
| **Maturity** |
| Production ready | ❌ | ✅ | actix is mature |
| Documentation | ⚠️ | ✅ | actix has extensive docs |
| Community size | Small | Large | actix is established |
| Active maintenance | ✅ | ⚠️ | actix: passively maintained |

**Legend:**
- ✅ Full support
- ⚠️ Partial support or requires workarounds
- ❌ Not supported

---

## Performance Benchmarks

### Methodology

All benchmarks performed on:
- CPU: (your system specs)
- Rust: 1.75+
- Criterion 0.8
- Tokio 1.48
- Quick mode (fewer samples for faster comparison)

### Results Summary

#### Actor Spawn Time

| Framework | Time per spawn | Relative |
|-----------|----------------|----------|
| joerl | ~6.15 µs | 1.0x |
| actix | ~TBD µs | TBD |

**Analysis**: TBD after running benchmarks

#### Message Throughput (1000 messages)

| Framework | Total Time | Msgs/sec | Relative |
|-----------|-----------|----------|----------|
| joerl | ~7 ms | ~143k | 1.0x |
| actix | TBD | TBD | TBD |

**Analysis**: TBD after running benchmarks

#### Multiple Actors (50 actors, 1 msg each)

| Framework | Total Time | Relative |
|-----------|-----------|----------|
| joerl | ~11 ms | 1.0x |
| actix | TBD | TBD |

**Analysis**: TBD after running benchmarks

### Performance Insights

#### joerl Strengths

1. **Predictable overhead**: Consistent performance characteristics
2. **Simple runtime**: Minimal abstraction layers
3. **Async-first**: Native async/await throughout

#### joerl Weaknesses

1. **Dynamic dispatch**: `Box<dyn Any>` has runtime cost
2. **Type checking**: Runtime downcasting overhead
3. **Less optimization**: Newer codebase, less compiler optimization

#### actix Strengths

1. **Static typing**: Zero-cost message dispatch
2. **Mature optimizations**: Years of performance tuning
3. **Inlining**: Better compiler optimization opportunities
4. **Specialized paths**: Optimized for common patterns

#### actix Weaknesses

1. **System overhead**: More complex runtime system
2. **Arbiter management**: Additional thread management
3. **Context switching**: More scheduling overhead

---

## Use Case Recommendations

### When to Choose joerl

✅ **Ideal for:**

1. **Erlang Developers**: If you're coming from Erlang/Elixir and want familiar patterns
2. **Learning OTP**: Understanding Erlang/OTP concepts in Rust
3. **Prototyping**: Quick actor-based prototypes with Erlang patterns
4. **Educational**: Teaching actor model with Erlang semantics
5. **Supervision Focus**: Heavy use of supervision trees with complex restart logic

❌ **Not recommended for:**

1. **Production systems**: Too new, not battle-tested
2. **High-performance**: actix has better optimization
3. **Type safety critical**: Dynamic messages reduce compile-time checking
4. **Large teams**: Less tooling support, smaller community

### When to Choose actix

✅ **Ideal for:**

1. **Production Systems**: Mature, battle-tested in production
2. **Web Applications**: Integrates with actix-web framework
3. **High Performance**: Optimized for throughput and latency
4. **Type Safety**: Compile-time message type checking
5. **Large Codebases**: Better refactoring and tooling support
6. **Team Projects**: Larger community, more resources

❌ **Not recommended for:**

1. **Erlang Porting**: API is very different from Erlang
2. **Simple Patterns**: May be overkill for basic actor needs
3. **Learning Actors**: Steeper learning curve than joerl

---

## Conceptual Mapping

### Erlang/OTP → joerl → actix

| Erlang/OTP | joerl | actix | Notes |
|------------|-------|-------|-------|
| `spawn/1` | `system.spawn()` | `.start()` | Create actor |
| `Pid` | `Pid` | `Addr` | Actor reference |
| `!` (send) | `send()` | `do_send()` | Fire-and-forget |
| `gen_server:call` | `call()` | `send().await` | Request/reply |
| `gen_server:cast` | `cast()` | `do_send()` | Async send |
| `gen_server` | `GenServer` | Multiple `Handler` | Server behavior |
| `gen_statem` | `GenStatem` | Manual FSM | State machine |
| `supervisor` | `Supervisor` | `Supervised` | Fault tolerance |
| `link/1` | `link()` | Manual | Process linking |
| `monitor/2` | `monitor()` | Manual | Process monitoring |
| `{'EXIT', Pid, Reason}` | `Signal::Exit` | N/A | Exit signal |
| `{'DOWN', Ref, ...}` | `Signal::Down` | N/A | Monitor signal |

---

## Code Examples: Side by Side

### Example 1: Counter Actor

<details>
<summary>joerl Implementation</summary>

```rust
use joerl::{Actor, ActorContext, ActorSystem, Message};
use async_trait::async_trait;

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
                }
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
    counter.send(Box::new("get")).await.unwrap();
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
```

</details>

<details>
<summary>actix Implementation</summary>

```rust
use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "()")]
struct Increment;

#[derive(Message)]
#[rtype(result = "i32")]
struct Get;

struct Counter {
    count: i32,
}

impl Actor for Counter {
    type Context = Context<Self>;
}

impl Handler<Increment> for Counter {
    type Result = ();
    
    fn handle(&mut self, _msg: Increment, _ctx: &mut Context<Self>) {
        self.count += 1;
        println!("Count: {}", self.count);
    }
}

impl Handler<Get> for Counter {
    type Result = i32;
    
    fn handle(&mut self, _msg: Get, _ctx: &mut Context<Self>) -> i32 {
        println!("Current count: {}", self.count);
        self.count
    }
}

#[actix::main]
async fn main() {
    let addr = Counter { count: 0 }.start();
    
    addr.do_send(Increment);
    let count = addr.send(Get).await.unwrap();
    
    System::current().stop();
}
```

</details>

### Example 2: Supervised Workers

<details>
<summary>joerl Implementation</summary>

```rust
use joerl::supervisor::{ChildSpec, RestartStrategy, SupervisorSpec, spawn_supervisor};
use joerl::{Actor, ActorContext, ActorSystem, ExitReason, Message};

struct Worker {
    id: usize,
}

#[async_trait]
impl Actor for Worker {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            if *cmd == "crash" {
                ctx.stop(ExitReason::Panic("Simulated crash".into()));
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let system = Arc::new(ActorSystem::new());
    
    let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
        .child(ChildSpec::new("worker1", || Box::new(Worker { id: 1 })))
        .child(ChildSpec::new("worker2", || Box::new(Worker { id: 2 })));
    
    let supervisor = spawn_supervisor(&system, spec);
    
    // Supervisor will automatically restart crashed workers
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
}
```

</details>

<details>
<summary>actix Implementation</summary>

```rust
use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "()")]
struct Crash;

struct Worker {
    id: usize,
}

impl Actor for Worker {
    type Context = Context<Self>;
}

impl Supervised for Worker {}

impl Handler<Crash> for Worker {
    type Result = ();
    
    fn handle(&mut self, _msg: Crash, ctx: &mut Context<Self>) {
        ctx.stop();
    }
}

#[actix::main]
async fn main() {
    let addr1 = Worker { id: 1 }.start();
    let addr2 = Worker { id: 2 }.start();
    
    // Manual supervision logic needed
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    
    System::current().stop();
}
```

</details>

---

## Migration Guide

### From Erlang/OTP to joerl

**Difficulty**: Easy ⭐⭐

joerl is designed for this transition:

1. **Terminology is identical**: `Pid`, `gen_server`, `supervisor`
2. **Patterns map directly**: Call/cast, supervision strategies
3. **Mental model preserved**: Process-oriented thinking

**Key Differences:**
- Async/await instead of message receive loops
- Type annotations for trait implementations
- Explicit error handling with `Result`

### From Erlang/OTP to actix

**Difficulty**: Moderate ⭐⭐⭐⭐

Requires conceptual shift:

1. **Different terminology**: `Addr` instead of `Pid`
2. **Typed messages**: Each message is a type, not a term
3. **Handler pattern**: Separate impl per message type
4. **No gen_server**: Use multiple `Handler` implementations

### From actix to joerl

**Difficulty**: Easy ⭐⭐

More conceptual than technical:

1. Replace typed messages with dynamic messages
2. Convert `Handler` impls to match statements in `handle_message`
3. Use `GenServer` for call/cast patterns
4. Adapt supervision to use `SupervisorSpec`

### From joerl to actix

**Difficulty**: Moderate ⭐⭐⭐

Requires refactoring:

1. Convert dynamic messages to typed message structs
2. Split `handle_message` into separate `Handler` implementations
3. Replace `GenServer` with multiple handlers
4. Manually implement supervision logic

---

## Conclusion

### Summary

- **joerl**: Faithful Erlang/OTP semantics, learning-friendly, experimental
- **actix**: Production-ready, performance-optimized, Rust-native

### Final Recommendations

**Choose joerl if:**
- You're porting Erlang/Elixir code
- You want to learn OTP patterns in Rust
- You need explicit Erlang semantics
- You're prototyping actor-based systems

**Choose actix if:**
- You need production reliability
- Performance is critical
- You want strong type safety
- You're building on actix-web

**Use both if:**
- Learning: Study joerl for concepts, actix for production patterns
- Transition: Prototype in joerl, implement in actix

---

## Appendix: Running the Benchmarks

### Prerequisites

```bash
cd /path/to/joerl
cargo bench --bench actix_comparison
```

### Benchmark Suite

The comparison benchmarks measure:

1. **Actor spawn time**: Overhead of creating new actors
2. **Message send**: Latency and throughput of message passing
3. **Multiple actors**: Scalability with concurrent actors
4. **Throughput**: Peak message processing rate

### Interpreting Results

- **Lower is better**: For spawn time and latency
- **Higher is better**: For throughput (msgs/sec)
- **Relative performance**: Compare within same workload
- **System dependent**: Results vary by hardware

---

## Contributing

Contributions to improve this comparison are welcome! Please:

1. Run benchmarks on your system and share results
2. Add missing feature comparisons
3. Provide real-world use case examples
4. Correct any inaccuracies

---

## References

### joerl

- GitHub: https://github.com/am-kantox/joerl
- Documentation: (TBD - docs.rs once published)

### actix

- GitHub: https://github.com/actix/actix
- Documentation: https://docs.rs/actix
- User Guide: https://actix.rs/docs/actix

### Erlang/OTP

- Official Documentation: https://www.erlang.org/doc/
- Design Principles: https://www.erlang.org/doc/design_principles/des_princ.html
- gen_server: https://www.erlang.org/doc/man/gen_server.html

---

**Last Updated**: 2025-12-05  
**joerl Version**: 0.1.0  
**actix Version**: 0.13.5
