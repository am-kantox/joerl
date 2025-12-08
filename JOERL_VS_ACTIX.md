# joerl vs actix: Feature Comparison

## Quick Reference

| Aspect | joerl 0.4.0 | actix 0.13.5 |
|--------|-------------|--------------|
| **Philosophy** | Erlang/OTP fidelity | Rust-native pragmatism |
| **Message Types** | Dynamic (`Box<dyn Any>`) | Static (typed traits) |
| **Target Users** | Erlang/Elixir developers | General Rust developers |
| **Status** | Active development | Production-ready, passively maintained |
| **Panic Handling** | ✅ Full Erlang/OTP style | ⚠️ Manual implementation |

---

## Core Design Differences

### Message Handling Philosophy

#### joerl: Dynamic Messages (Erlang-style)

```rust
use joerl::{Actor, ActorContext, Message};

#[async_trait]
impl Actor for Counter {
    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "increment" => self.count += 1,
                "get" => println!("{}", self.count),
                _ => {}
            }
        }
    }
}
```

**Pros:**
- Simple, familiar to Erlang developers
- Flexible message handling
- Single handler method

**Cons:**
- Runtime type checking
- Less IDE support
- Potential runtime errors

#### actix: Typed Messages (Rust-native)

```rust
use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "()")]
struct Increment;

impl Handler<Increment> for Counter {
    type Result = ();
    fn handle(&mut self, _msg: Increment, _ctx: &mut Context<Self>) {
        self.count += 1;
    }
}
```

**Pros:**
- Compile-time type safety
- Better tooling/IDE support
- Clear API contracts

**Cons:**
- More boilerplate
- Separate handler per message
- Different from Erlang patterns

---

## Feature Matrix

### Core Features

| Feature | joerl | actix | Winner |
|---------|-------|-------|--------|
| Basic actors | ✅ | ✅ | Tie |
| Async/await | ✅ | ✅ | Tie |
| Bounded mailboxes | ✅ | ✅ | Tie |
| Actor lifecycle | ✅ | ✅ | Tie |
| Message passing | ✅ Dynamic | ✅ Typed | Different approach |

### Erlang/OTP Patterns

| Feature | joerl | actix | Notes |
|---------|-------|-------|-------|
| **gen_server** | ✅ Native | ❌ | Use multiple handlers in actix |
| **gen_statem** | ✅ DSL with Mermaid | ❌ | joerl only |
| **supervisor** | ✅ Structured | ⚠️ Manual | joerl has explicit strategies |
| **Links** | ✅ | ❌ | Erlang concept |
| **Monitors** | ✅ | ❌ | Erlang concept |
| **Trap exits** | ✅ | ❌ | Erlang pattern |

### Fault Tolerance & Panic Handling

| Feature | joerl 0.4.0 | actix 0.13.5 | Winner |
|---------|-------------|--------------|--------|
| **Panic catching** | ✅ Automatic | ❌ Manual | 🏆 **joerl** |
| **Panic to signal** | ✅ `ExitReason::Panic` | ❌ | 🏆 **joerl** |
| **Link notification** | ✅ `Signal::Exit` | ❌ | 🏆 **joerl** |
| **Monitor notification** | ✅ `Signal::Down` | ❌ | 🏆 **joerl** |
| **Supervisor restart** | ✅ Automatic | ⚠️ Manual | 🏆 **joerl** |
| **Cleanup guarantee** | ✅ Always runs | ⚠️ Best effort | 🏆 **joerl** |
| **Erlang/OTP compliance** | ✅ Full | ❌ | 🏆 **joerl** |

---

## Full Erlang/OTP Panic Handling (since 0.3.0)

### What Happens When an Actor Panics?

#### joerl Behavior (Erlang/OTP-style)

```rust
use joerl::{Actor, ActorContext, ActorSystem, ExitReason, Signal};

struct Worker;

#[async_trait]
impl Actor for Worker {
    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        panic!("Unexpected error!"); // This is caught!
    }
}

struct Supervisor;

#[async_trait]
impl Actor for Supervisor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        ctx.trap_exit(true); // Receive EXIT signals instead of dying
    }
    
    async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
        if let Signal::Exit { from, reason } = signal {
            match reason {
                ExitReason::Panic(msg) => {
                    println!("Worker {} panicked: {}", from, msg);
                    // Can restart, log, escalate, etc.
                }
                _ => {}
            }
        }
    }
    
    async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
}
```

**What joerl does automatically:**

1. ✅ **Catches the panic** using `std::panic::catch_unwind`
2. ✅ **Converts to ExitReason::Panic** for structured handling
3. ✅ **Notifies linked actors** via `Signal::Exit`
4. ✅ **Notifies monitors** via `Signal::Down`
5. ✅ **Triggers supervisor** to restart the actor
6. ✅ **Runs cleanup** (stopped hook, registry cleanup)
7. ✅ **No resource leaks** - guaranteed cleanup

#### actix 0.13.5 Behavior

```rust
use actix::prelude::*;

struct Worker;

impl Actor for Worker {
    type Context = Context<Self>;
}

impl Handler<SomeMessage> for Worker {
    type Result = ();
    fn handle(&mut self, _msg: SomeMessage, _ctx: &mut Context<Self>) {
        panic!("Unexpected error!"); // NOT caught!
    }
}
```

**What happens in actix:**

1. ❌ **Panic propagates** - task/thread may crash
2. ❌ **No automatic conversion** to structured error
3. ❌ **No link notification** (no link concept)
4. ❌ **No monitor notification** (no monitor concept)
5. ⚠️ **Manual supervision** required
6. ⚠️ **Cleanup not guaranteed**
7. ⚠️ **Potential resource leaks**

### Comparison: Panic Handling Code

#### joerl: Automatic and Transparent

```rust
// Panic is automatically caught and handled
let system = ActorSystem::new();
let worker = system.spawn(Worker);

// Panic in worker is caught, supervisor notified, worker restarted
// No manual panic handling code needed!
```

#### actix: Manual Implementation Required

```rust
// Must manually implement panic recovery
impl Actor for Worker {
    fn started(&mut self, ctx: &mut Context<Self>) {
        // Set up panic hook manually
        ctx.set_mailbox_capacity(1);
    }
}

impl Supervised for Worker {
    fn restarting(&mut self, ctx: &mut Context<Worker>) {
        // Manual restart logic
    }
}

// Must manually catch panics in message handlers
impl Handler<MyMsg> for Worker {
    fn handle(&mut self, msg: MyMsg, ctx: &mut Context<Self>) -> Self::Result {
        // Wrap in catch_unwind yourself
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Your code here
        }));
        
        if result.is_err() {
            // Manually handle panic
            ctx.stop();
        }
    }
}
```

---

## Performance Comparison

### Benchmarks (from `cargo bench --bench actix_comparison`)

| Benchmark | joerl | actix | Winner | Difference |
|-----------|-------|-------|--------|------------|
| **Actor spawn** | 6.04 µs | 9.56 µs | 🏆 joerl | 37% faster |
| **10 messages** | 3.11 ms | 3.48 ms | 🏆 joerl | 11% faster |
| **100 messages** | 3.13 ms | 3.51 ms | 🏆 joerl | 11% faster |
| **1000 messages** | 3.44 ms | 3.71 ms | 🏆 joerl | 7% faster |
| **Throughput** | 11.86 ms | 11.64 ms | 🏆 actix | 2% faster |
| **10 actors** | 6.18 ms | 6.57 ms | 🏆 joerl | 6% faster |
| **50 actors** | 6.23 ms | 6.64 ms | 🏆 joerl | 6% faster |

**Winner: joerl (6 out of 7 benchmarks)** 🏆

Despite using dynamic dispatch, joerl is faster in most scenarios due to:
- Simpler runtime architecture
- Less per-actor overhead
- Direct message passing
- Efficient async/await design

---

## Use Case Recommendations

### Choose joerl if:

✅ **You are:**
- Coming from Erlang/Elixir
- Building fault-tolerant systems
- Learning actor model/OTP patterns
- Need explicit Erlang semantics
- Prototyping with actors

✅ **You need:**
- Automatic panic recovery
- Supervision trees with restart strategies
- Links and monitors
- gen_server or gen_statem patterns
- Dynamic message handling

❌ **Avoid joerl if:**
- You require strong compile-time type safety
- You're building on actix-web (use actix)
- You need mature ecosystem integration

### Choose actix if:

✅ **You are:**
- Building production web services
- Working with actix-web
- Prefer strong type safety
- Have large team/codebase

✅ **You need:**
- Compile-time message type checking
- Web framework integration
- Streaming support
- Mature, battle-tested library

❌ **Avoid actix if:**
- You want Erlang/OTP patterns
- You need automatic panic handling
- You're porting from Erlang/Elixir

---

## Example: Supervised Worker with Panic Recovery

### joerl (Automatic)

```rust
use joerl::{
    Actor, ActorContext, ActorSystem, ExitReason, Message,
    supervisor::{ChildSpec, RestartStrategy, SupervisorSpec, spawn_supervisor},
};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

struct Worker {
    start_count: Arc<AtomicUsize>,
}

#[async_trait]
impl Actor for Worker {
    async fn started(&mut self, _ctx: &mut ActorContext) {
        let count = self.start_count.fetch_add(1, Ordering::SeqCst);
        if count == 0 {
            panic!("Initial panic"); // Automatically caught!
        }
        println!("Worker started successfully after restart");
    }
    
    async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
}

#[tokio::main]
async fn main() {
    let system = Arc::new(ActorSystem::new());
    let start_count = Arc::new(AtomicUsize::new(0));
    let start_count_clone = Arc::clone(&start_count);
    
    // Create supervisor that will automatically restart on panic
    let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
        .child(ChildSpec::new("worker", move || {
            Box::new(Worker {
                start_count: Arc::clone(&start_count_clone),
            })
        }));
    
    let _supervisor = spawn_supervisor(&system, spec);
    
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    
    // Worker panicked once, was restarted automatically
    assert_eq!(start_count.load(Ordering::SeqCst), 2);
}
```

**Lines of code: ~35**  
**Panic handling: Automatic**  
**Restart logic: Built-in**

### actix (Manual)

```rust
use actix::prelude::*;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

struct Worker {
    start_count: Arc<AtomicUsize>,
}

impl Actor for Worker {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Context<Self>) {
        let count = self.start_count.fetch_add(1, Ordering::SeqCst);
        
        // Must manually catch and handle panic
        if count == 0 {
            ctx.stop(); // Can't panic, must stop manually
            // Need to manually restart
        }
    }
}

impl Supervised for Worker {
    fn restarting(&mut self, _ctx: &mut Context<Worker>) {
        // Manual restart logic here
    }
}

struct Supervisor {
    worker: Option<Addr<Worker>>,
    start_count: Arc<AtomicUsize>,
}

impl Actor for Supervisor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Context<Self>) {
        // Manually spawn and supervise worker
        let worker = Worker {
            start_count: Arc::clone(&self.start_count),
        }.start();
        
        self.worker = Some(worker);
    }
}

// Additional message types and handlers needed for restart logic
// ... (more boilerplate)

#[actix::main]
async fn main() {
    let start_count = Arc::new(AtomicUsize::new(0));
    
    let supervisor = Supervisor {
        worker: None,
        start_count: Arc::clone(&start_count),
    }.start();
    
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    System::current().stop();
}
```

**Lines of code: ~60+**  
**Panic handling: Manual**  
**Restart logic: Custom implementation**

---

## Migration Guide

### From Erlang/OTP → joerl: Easy ⭐⭐

Direct mapping:

| Erlang | joerl |
|--------|-------|
| `spawn/1` | `system.spawn()` |
| `Pid` | `Pid` |
| `!` | `send()` |
| `gen_server:call` | `call()` |
| `gen_server:cast` | `cast()` |
| `supervisor` | `SupervisorSpec` |
| `link/1` | `link()` |
| `monitor/2` | `monitor()` |
| `trap_exit` | `trap_exit(true)` |

### From Erlang/OTP → actix: Moderate ⭐⭐⭐⭐

Requires rethinking:
- Learn typed message pattern
- Convert gen_server to multiple handlers
- Implement supervision manually
- No direct link/monitor equivalent

### From actix → joerl: Easy ⭐⭐

Simplification:
- Replace typed messages with dynamic matching
- Consolidate handlers into `handle_message`
- Use `GenServer` for call/cast patterns
- Use `SupervisorSpec` for supervision

### From joerl → actix: Moderate ⭐⭐⭐

Requires refactoring:
- Create typed message structs
- Split into multiple `Handler` implementations
- Implement supervision manually
- Remove link/monitor dependencies

---

## Conclusion

### joerl 0.4.0 Strengths

1. ✅ **Full Erlang/OTP panic handling** - unique to joerl
2. ✅ **Faster in most benchmarks** - simpler architecture wins
3. ✅ **Easier for Erlang developers** - familiar patterns
4. ✅ **Built-in fault tolerance** - supervision trees, links, monitors
5. ✅ **Less boilerplate** - dynamic messages, single handler

### actix 0.13.5 Strengths

1. ✅ **Production-proven** - years of battle-testing
2. ✅ **Type safety** - compile-time message checking
3. ✅ **Ecosystem** - actix-web integration
4. ✅ **Maturity** - extensive documentation, large community
5. ✅ **Slight throughput edge** - for sustained high-volume

### Final Recommendation

**Use joerl if:**
- You value Erlang/OTP patterns and automatic fault tolerance
- You're building fault-tolerant systems
- You prioritize panic handling and supervision
- You're coming from Erlang/Elixir

**Use actix if:**
- You need production maturity and ecosystem
- You value compile-time type safety
- You're building on actix-web
- You prefer Rust-native patterns

**Both are excellent choices** - the decision depends on your priorities: Erlang fidelity (joerl) vs. Rust ecosystem integration (actix).

---

## References

- **joerl**: https://github.com/am-kantox/joerl | https://crates.io/crates/joerl
- **actix**: https://github.com/actix/actix | https://docs.rs/actix
- **Erlang/OTP**: https://www.erlang.org/doc/

**Version Info:**
- joerl: 0.4.0 (2024-12-08)
- actix: 0.13.5
- Comparison last updated: 2024-12-08
