# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - 2025-12-10

### Added

- **Deferred Reply Pattern for GenServer** - Full Erlang-style deferred reply support
  - Added `CallResponse<T>` enum with `Reply(T)` and `NoReply` variants
  - Implemented `ReplyHandle<T>` for sending deferred replies asynchronously
  - Server can now accept requests, return immediately, and reply later when work completes
  - Multiple concurrent deferred calls supported
  - Matches Erlang's `{noreply, State}` and `gen_server:reply/2` semantics exactly
  - Breaking change: `GenServer::handle_call` now returns `CallResponse<Self::CallReply>` instead of `Self::CallReply`
  
- **Selective Receive** - Erlang-style pattern matching for messages
  - Added pending queue (`Mutex<VecDeque<Envelope>>`) to `Mailbox` for non-matching messages
  - Implemented `recv_matching()` with timeout support
  - Implemented `try_recv_matching()` for non-blocking selective receive
  - Added three public methods to `ActorContext`:
    - `receive(predicate)` - Wait indefinitely for matching message
    - `receive_timeout(predicate, timeout)` - Wait with timeout
    - `try_receive(predicate)` - Non-blocking check
  - Non-matching messages saved to pending queue and checked first on subsequent receives
  - Maintains message ordering through `VecDeque`
  - Perfect for RPC patterns with correlation IDs
  
- **Examples**
  - `gen_server_deferred_reply.rs` - Job processor demonstrating async work with deferred replies
  - `actor_selective_receive.rs` - RPC client/server with correlation IDs using selective receive
  
- **Documentation**
  - Updated `GETTING_STARTED.md` with comprehensive deferred reply and selective receive sections
  - Updated `GETTING_STARTED_RU.md` with full Russian translations
  - Added code examples, benefits, use cases, and Erlang mappings
  - Updated example lists in both English and Russian guides

### Fixed

- Resolved clippy warnings in `actor_selective_receive` example
- Removed unused `monitored` field from `MonitorActor` struct
- Collapsed nested if statements using let-chain patterns

### Changed

- **Breaking**: `GenServer::handle_call` signature changed to return `CallResponse<Self::CallReply>`
  - Migration: wrap existing returns in `CallResponse::Reply(value)`
  - Example: `state.value` → `CallResponse::Reply(state.value)`

### Technical Details

- Deferred reply uses `tokio::spawn` for async work without blocking the server
- Selective receive uses `Mutex<VecDeque<Envelope>>` for thread-safe pending queue
- Predicates use `FnMut(&Message) -> Option<T>` for flexible pattern matching
- Timeout support via `tokio::time::timeout_at`
- All new features include comprehensive test suites (12 new tests total)
- All tests passing, code formatted, clippy clean

## [0.5.0] - 2024-12-09

### Added

- Comprehensive Getting Started guides in English and Russian
- Property-based testing with QuickCheck for improved reliability
- Distributed system metrics for telemetry
- Unified `ActorSystem` with transparent distributed support
- Location-transparent messaging (same API for local and remote actors)
- Remote messaging support with full RPC capabilities
- Ping/pong RPC for remote `is_process_alive()`
- Handshake protocol for bidirectional connections
- EPMD (Erlang Port Mapper Daemon) client and server implementation
- Custom telemetry providers support
- Memory tracking for actors and mailboxes

### Changed

- Deprecated `DistributedSystem` in favor of `ActorSystem::new_distributed()`
- Cleaned up CI verbosity and deprecated struct documentation
- Migrated to unified ActorSystem architecture

### Fixed

- Resolved clippy warnings in distributed.rs
- Various documentation consistency improvements

## Earlier Versions

For earlier version history, see the git commit log.

[0.6.0]: https://github.com/am-kantox/joerl/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/am-kantox/joerl/compare/v0.4.0...v0.5.0
