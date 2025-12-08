# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

**joerl** is an Erlang-inspired actor model library for Rust, named in tribute to Joe Armstrong, the creator of Erlang. It implements the actor model with supervision trees, links, monitors, and bounded mailboxes using Rust's async/await built on tokio.

## Key Features

### GenServer
Erlang-style generic server with call (synchronous) and cast (asynchronous) operations.

### GenStatem
Finite state machines defined with Mermaid diagrams, compiled to Rust code with validation.

### Distributed Clustering
- EPMD server and client for node discovery
- Location-transparent messaging between nodes
- Unified ActorSystem API (same for local and distributed)
- TCP transport with automatic reconnection
- Message serialization with global registry

### Panic Handling
Full Erlang/OTP-style panic recovery:
- Panics caught automatically
- Links and monitors notified
- Supervisors restart panicked actors
- Guaranteed cleanup (no leaks)

### Health Monitoring
System health checks with configurable thresholds for:
- Active actor count
- Message processing rates
- Supervisor restart rates

### Telemetry (Optional)
Metrics and observability with Prometheus/OpenTelemetry support.

## Common Commands

### Building
```bash
cargo build                    # Build the library
cargo build --release          # Build optimized release version
```

### Testing
```bash
cargo test                     # Run all tests (unit + integration + doc tests)
cargo test test_name           # Run specific test by name
cargo test -- --nocapture      # Run tests with output visible
cargo test --doc               # Run only doc tests
cargo test --test integration_test  # Run integration tests only
```

### Code Quality
```bash
cargo fmt                      # Format code (required before commits)
cargo clippy --all-targets --all-features -- -D warnings  # Run linter (CI enforces this)
```

### Coverage
```bash
cargo install cargo-tarpaulin  # Install coverage tool (once)
cargo tarpaulin --out Html     # Generate HTML coverage report
cargo tarpaulin --verbose --all-features --workspace --timeout 120 --out Xml  # CI-style coverage
```

### Examples
```bash
cargo run --example counter    # Run counter example
cargo run --example ping_pong  # Run ping-pong example
```

### Benchmarks
```bash
cargo bench                    # Run benchmarks (uses Criterion)
```

## Architecture

### Core Module Structure

The library follows a modular design organized around core actor system concepts:

- **`actor`**: Defines the `Actor` trait and `ActorContext`. All actors implement the `Actor` trait with `handle_message()` as the main entry point. The context provides access to the actor's Pid, mailbox operations, and lifecycle control (trap_exit, stop).

- **`system`**: The `ActorSystem` is the runtime that manages all actors. It maintains a `DashMap` registry mapping `Pid` to `ActorEntry` (containing mailbox sender, links, monitors). Spawning returns an `ActorRef` which is used to send messages and interact with actors.

- **`pid`**: Process identifiers using UUID v7 for unique, time-ordered actor IDs. Now includes node support for distributed systems.

- **`mailbox`**: Bounded MPSC channels (using tokio) that provide backpressure. Messages are wrapped in `Envelope` which can contain either user messages or system signals.

- **`message`**: Defines `Message` (type-erased `Box<dyn Any>`), `Signal` enum (Exit, Down, Stop, Kill), and `ExitReason` for actor termination.

- **`gen_server`**: Implements Erlang's gen_server behavior with call/cast semantics for structured request-reply patterns.

- **`gen_statem`**: Implements Erlang's gen_statem with Mermaid DSL for defining finite state machines with compile-time validation.

- **`supervisor`**: Implements Erlang/OTP supervision trees. Supervisors monitor children and restart them according to strategies (`OneForOne`, `OneForAll`, `RestForOne`). Includes restart intensity limits to prevent infinite loops.

- **`distributed`**: Distributed actor system with location transparency. Wraps ActorSystem with EPMD integration, node discovery, and TCP transport.

- **`epmd`**: Erlang Port Mapper Daemon implementation for node discovery and registration (client and server).

- **`serialization`**: Trait-based message serialization system with global registry for remote messaging.

- **`health`**: System health monitoring with configurable checks for actor counts, message rates, and resource usage.

- **`telemetry`**: Optional metrics and observability support (requires "telemetry" feature).

- **`error`**: Error types and Result alias using `thiserror`.

### Actor Lifecycle

1. **Spawn**: Actor registered in system, given Pid and mailbox, tokio task spawned
2. **Started**: `Actor::started()` hook called for initialization  
3. **Message Loop**: Actor processes messages/signals from mailbox sequentially
4. **Stopped**: `Actor::stopped()` hook called, links/monitors notified, cleanup performed

### Key Design Patterns

- **Message Passing**: All communication via type-erased messages (`Box<dyn Any + Send>`). Actors downcast to expected types.

- **Links vs Monitors**: Links are bidirectional (both actors fail together), monitors are unidirectional (observer receives DOWN signal). Actors can trap exits to handle failures gracefully.

- **Supervision**: Supervisors use factory functions (`Box<dyn FnMut() -> Box<dyn Actor>>`) to recreate children. They track restart times to enforce intensity limits.

## Erlang/OTP Mapping

This library intentionally mirrors Erlang/OTP concepts and terminology. When implementing features or fixing bugs:

- Use Erlang terminology in APIs and documentation (e.g., `Pid`, `trap_exit`, `monitor`)
- Follow Erlang semantics (e.g., normal exit doesn't trigger restarts, links are bidirectional)
- Reference Erlang/OTP documentation for expected behavior
- See README.md "Erlang Terminology Mapping" table for exact equivalents

## Development Guidelines

### Testing Requirements
- All new features require tests (unit and/or integration)
- Target >80% code coverage
- Test edge cases, error conditions, and concurrent scenarios
- Use descriptive test names explaining what is being tested

### Async Considerations
- All actor operations are async using tokio
- Use `#[async_trait]` for the `Actor` trait
- Be careful with blocking operations—use `tokio::task::spawn_blocking` if needed
- Actors process messages sequentially but the system is concurrent

### Error Handling
- Use `ActorError` and `Result<T>` types consistently
- Send operations can fail if actor dead or mailbox full
- Link/monitor operations can fail if actors don't exist

### Documentation
- Document all public APIs with rustdoc comments (`///`)
- Include examples in documentation when helpful
- Reference Erlang equivalents in doc comments
- Keep README.md synchronized with API changes

## CI/CD

GitHub Actions runs on push/PR to main:
- **test**: All tests including doc tests
- **fmt**: Checks code formatting with `cargo fmt --check`
- **clippy**: Lints with warnings as errors
- **coverage**: Generates coverage report using tarpaulin, uploads to Codecov

All checks must pass before merging.

## Commit Conventions

Follow conventional commits format:
- `feat:` - New features
- `fix:` - Bug fixes  
- `docs:` - Documentation changes
- `test:` - Test additions/changes
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `chore:` - Maintenance tasks

Example: `feat: add support for named actors in registry`
