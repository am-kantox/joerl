# Telemetry Enhancement TODO

This document tracks planned enhancements to joerl's telemetry system.

## 🔴 Critical Missing Features

### 1. Actor-Level Labels/Metadata ✅ COMPLETED
**Status**: Completed  
**Priority**: Immediate  
**Value**: Debug specific actor types, identify which actors consume most resources

**Implementation**:
- [x] Add `actor_type` tracking to ActorEntry
- [x] Add `actor_type_name<T>()` helper function to extract type names
- [x] Add `*_typed()` variants to ActorMetrics for type-labeled metrics
- [x] Add labels to all actor lifecycle metrics
- [x] Update `ActorSystem::spawn_internal()` to capture and store actor type
- [x] Update `cleanup_actor()` and panic handler to use actor type
- [x] Update documentation with new labels and example queries

**Result**: All actor lifecycle metrics now include `type` label enabling per-actor-type observability.

**Metrics to label**:
- `joerl_actors_spawned_total` → add `type` label
- `joerl_actors_stopped_total` → add `type` label (already has `reason`)
- `joerl_actors_active` → add `type` label
- `joerl_actors_panicked_total` → add `type` label

---

### 2. Distributed System Metrics
**Status**: Not started  
**Priority**: Short-term  
**Value**: Essential for debugging distributed systems, network issues, partition detection

**Metrics to add**:
- `joerl_remote_messages_sent_total{target_node}`
- `joerl_remote_messages_failed_total{target_node, reason}`
- `joerl_node_connections_active{node}`
- `joerl_node_connection_established_total{node}`
- `joerl_node_connection_lost_total{node}`
- `joerl_network_latency_seconds{node}` (histogram)
- `joerl_serialization_errors_total`

**Files to modify**:
- `joerl/src/distributed.rs`
- `joerl/src/epmd/client.rs`
- `joerl/src/epmd/server.rs`

---

### 3. GenServer & GenStatem Metrics ✅ COMPLETED
**Status**: Completed  
**Priority**: Immediate  
**Value**: Track FSM behavior, identify stuck states, measure synchronous call latency

**Implementation**:
- [x] Add `GenServerMetrics` and `GenStatemMetrics` structs to telemetry module
- [x] Add `GenServerCallSpan` and `GenStatemStateSpan` for duration tracking
- [x] Add `server_type` field to `GenServerActor` to track server type
- [x] Instrument GenServer call operations with duration histogram and in-flight gauge
- [x] Instrument GenServer cast operations with counter
- [x] Add `fsm_type` and `state_span` fields to `GenStatemActor`
- [x] Track state transitions with from/to labels
- [x] Track state duration with automatic span management
- [x] Track current state with gauge metric
- [x] Update TELEMETRY.md with new metrics tables and PromQL queries
- [x] Update telemetry module documentation

**Result**: GenServer and GenStatem now have comprehensive observability for calls, casts, state transitions, and state durations.

**Metrics added**:

**GenServer**:
- `joerl_gen_server_call_duration_seconds{type}` (histogram)
- `joerl_gen_server_casts_total{type}` (counter)
- `joerl_gen_server_call_timeouts_total{type}` (counter, infrastructure ready)
- `joerl_gen_server_calls_in_flight{type}` (gauge)

**GenStatem**:
- `joerl_gen_statem_transitions_total{type, from, to}` (counter)
- `joerl_gen_statem_invalid_transitions_total{type, state, event}` (counter, infrastructure ready)
- `joerl_gen_statem_state_duration_seconds{type, state}` (histogram)
- `joerl_gen_statem_current_state{type, state}` (gauge)

**Files modified**:
- `joerl/src/telemetry.rs` - Added metrics structs and span types
- `joerl/src/gen_server.rs` - Instrumented with call/cast tracking
- `joerl/src/gen_statem.rs` - Instrumented with state transition/duration tracking
- `TELEMETRY.md` - Updated with metrics documentation and PromQL queries

---

## 🟡 Important Enhancements

### 4. Per-Actor Mailbox Tracking ✅ COMPLETED
**Status**: Completed  
**Priority**: Immediate  
**Value**: Identify backpressure hotspots, find slow consumers

**Implementation**:
- [x] Add `actor_type` and `capacity` fields to `MailboxSender`
- [x] Create `new_with_type()` method for mailbox creation with type
- [x] Change `mailbox_depth` gauge to include `actor_type` label
- [x] Add `joerl_mailbox_utilization_percent{actor_type}` gauge (depth/capacity * 100)
- [x] Add `actor_type` label to `mailbox_full_total` counter
- [x] Update ActorSystem to pass actor type to mailbox
- [x] Keep backward-compatible non-typed variants
- [x] Update documentation with new metrics and queries

**Result**: All mailbox metrics now track per-actor-type data, enabling identification of specific actors with backpressure issues.

---

### 5. Signal-Specific Metrics ✅ COMPLETED
**Status**: Completed  
**Priority**: Short-term  
**Value**: Debug supervision trees, understand failure propagation patterns

**Implementation**:
- [x] Add `SignalMetrics` struct to telemetry module
- [x] Track signal sending in `send_signal()` with type labels
- [x] Track signal reception in actor message loop
- [x] Detect and track trapped/ignored signals
- [x] Track exit signal reasons separately
- [x] Update TELEMETRY.md with signal metrics and queries

**Result**: Complete signal observability for debugging supervision trees and failure propagation.

**Metrics added**:
- `joerl_signals_sent_total{type}` - exit, down, stop, kill
- `joerl_signals_received_total{type}`
- `joerl_signals_ignored_total{type}` - tracked when trap_exit=true
- `joerl_exit_signals_by_reason_total{reason}` - normal, shutdown, killed, panic, custom

**Files modified**:
- `joerl/src/telemetry.rs` - Added SignalMetrics struct
- `joerl/src/system.rs` - Instrumented send_signal() and signal reception
- `TELEMETRY.md` - Added metrics table and PromQL queries

---

### 6. Actor Lifetime Statistics ✅ COMPLETED
**Status**: Completed  
**Priority**: Short-term  
**Value**: Identify actors that die too quickly (restart loops) or leak (never die)

**Implementation**:
- [x] Add `spawn_time` field to ActorEntry (conditional on telemetry feature)
- [x] Track spawn time on actor creation
- [x] Calculate lifetime duration in cleanup_actor()
- [x] Add `actor_lifetime()` method to ActorMetrics
- [x] Automatically detect short-lived actors (< 1s)
- [x] Update TELEMETRY.md with lifetime metrics and queries

**Result**: Complete actor lifetime tracking for identifying restart loops and performance issues.

**Metrics added**:
- `joerl_actor_lifetime_seconds{type}` - histogram of actor lifetimes
- `joerl_short_lived_actors_total{type}` - counter for actors < 1s (restart loops)

**Files modified**:
- `joerl/src/system.rs` - Added spawn_time tracking and lifetime calculation
- `joerl/src/telemetry.rs` - Added actor_lifetime() method
- `TELEMETRY.md` - Added metrics table and PromQL queries

---

### 7. Message Queue Wait Time ✅ COMPLETED
**Status**: Completed  
**Priority**: Medium-term  
**Value**: Distinguish between slow processing vs. queue backlog

**Implementation**:
- [x] Restructure `Envelope` to include enqueue timestamp (conditional on telemetry)
- [x] Add `EnvelopeContent` enum to wrap Message or Signal
- [x] Calculate wait time when envelope is dequeued
- [x] Add `message_queue_wait()` method to MessageMetrics
- [x] Track `joerl_message_queue_wait_seconds` histogram
- [x] Update TELEMETRY.md with PromQL queries for queue analysis

**Result**: Can now distinguish between queue backlog and slow message processing, identifying backpressure hotspots.

**Metric added**:
- `joerl_message_queue_wait_seconds` - histogram of time messages spend in queue before processing

**Files modified**:
- `joerl/src/message.rs` - Restructured Envelope with timestamp and content
- `joerl/src/system.rs` - Calculate and record queue wait time on dequeue
- `joerl/src/telemetry.rs` - Added message_queue_wait() method
- `TELEMETRY.md` - Added metric table and PromQL queries

---

### 8. Sampling for High-Frequency Operations ✅ COMPLETED
**Status**: Completed  
**Priority**: Medium-term  
**Value**: Reduce overhead in high-throughput scenarios

**Implementation**:
- [x] Add `TelemetryConfig` struct with sampling rates per metric type
- [x] Implement deterministic counter-based sampling (consistent distribution)
- [x] Add `should_sample()` helper with counter modulo 100
- [x] Apply sampling to message processing metrics
- [x] Apply sampling to queue wait time metrics
- [x] Apply sampling to signal metrics
- [x] Add `set_config()` and `get_config()` API
- [x] Update `TelemetrySpan` to support sampling
- [x] Document sampling configuration and best practices
- [x] Add performance guidelines to TELEMETRY.md

**Result**: Configurable sampling for high-throughput systems, reducing overhead by up to 100x while maintaining accurate percentiles.

**Features added**:
- `TelemetryConfig` with per-metric-type sampling rates
- `set_config(config)` - Configure sampling before spawning actors
- `get_config()` - Retrieve current configuration
- Deterministic sampling using atomic counter (better distribution than random)
- Default 100% sampling (backward compatible)
- Zero overhead when sampling is disabled

**Files modified**:
- `joerl/src/telemetry.rs` - Added config structs, sampling logic, updated metrics
- `TELEMETRY.md` - Added sampling section with examples and best practices

---

## 🟢 Nice-to-Have Enhancements

### 9. Health Check Endpoint ✅ COMPLETED
**Status**: Completed  
**Priority**: Long-term  
**Value**: Kubernetes liveness/readiness probes, automated alerting

**Implementation**:
- [x] Add `SystemHealth` struct with health indicators
- [x] Add `HealthConfig` with configurable thresholds
- [x] Implement `is_healthy()`, `is_ready()`, and `get_status()` methods
- [x] Add structured `HealthIssue` reporting with severity levels
- [x] Add `get_report()` for formatted health summaries
- [x] Add `actor_count()` method to ActorSystem
- [x] Export health module and types from lib.rs
- [x] Add comprehensive unit tests
- [ ] Add HTTP endpoint (optional feature) for `/health` and `/ready` - Future enhancement
- [ ] Document integration with Kubernetes - Future enhancement

**Result**: Complete health checking system with configurable thresholds for monitoring actor system health.

**Features added**:
- `HealthConfig` - Configurable thresholds (max_actors, min_actors, panic_rate, etc.)
- `HealthStatus` - Enum with Healthy/Degraded/Unhealthy states
- `HealthIssue` - Structured issue reporting with severity
- `SystemHealth` - Main health checker with multiple query methods
- Health checks based on actor count, panic rate, restart rate, backpressure

**Files modified**:
- `joerl/src/health.rs` - NEW: Complete health checking module (272 lines)
- `joerl/src/system.rs` - Added actor_count() method
- `joerl/src/lib.rs` - Added health module and exports

---

### 10. Tracing Integration with OpenTelemetry Spans
**Status**: Not started  
**Priority**: Long-term  
**Value**: Distributed tracing across actor boundaries, see full request flow

**Implementation**:
- [ ] Add `#[instrument]` attribute support for actor methods
- [ ] Propagate trace context through messages
- [ ] Add span IDs to log messages
- [ ] Create example with Jaeger/Zipkin

---

### 11. Memory Usage Tracking
**Status**: Not started  
**Priority**: Long-term  
**Value**: Identify memory leaks, oversized actors

**Metrics to add**:
- `joerl_actor_memory_bytes{actor_type}` (gauge)
- `joerl_system_memory_bytes` (gauge)
- `joerl_mailbox_memory_bytes` (gauge)

**Note**: Requires memory profiling, may need external tools.

---

### 12. Custom Metric Registry
**Status**: Not started  
**Priority**: Long-term  
**Value**: Allow users to inject custom telemetry logic

**Implementation**:
- [ ] Define `TelemetryProvider` trait
- [ ] Add `set_telemetry_provider()` function
- [ ] Call provider hooks at key points
- [ ] Document custom provider implementation

---

## Progress Summary

- **Completed**: 8/12 (66.7%)
- **In Progress**: 0/12
- **Not Started**: 4/12

---

## Priority Order for Implementation

### Phase 1: Core Observability (Immediate) ✅ COMPLETED
1. ✅ **Actor-Level Labels/Metadata** (#1) - COMPLETED
2. ✅ **Per-Actor Mailbox Tracking** (#4) - COMPLETED
3. ✅ **GenServer/GenStatem Metrics** (#3) - COMPLETED

### Phase 2: Production Readiness (Short-term) ✅ COMPLETED (except #2)
4. **Distributed System Metrics** (#2) - Requires distributed module implementation
5. ✅ **Signal-Specific Metrics** (#5) - COMPLETED
6. ✅ **Actor Lifetime Statistics** (#6) - COMPLETED

### Phase 3: Performance & Optimization (Medium-term) ✅ COMPLETED
7. ✅ **Sampling Configuration** (#8) - COMPLETED
8. ✅ **Message Queue Wait Time** (#7) - COMPLETED

### Phase 4: Advanced Features (Long-term)
9. ✅ **Health Check Endpoint** (#9) - COMPLETED
10. **OpenTelemetry Span Integration** (#10)
11. **Memory Usage Tracking** (#11)
12. **Custom Metric Registry** (#12)

---

## Notes

- All enhancements should maintain backward compatibility
- Feature flags should be considered for optional heavy features
- Performance impact should be measured for each enhancement
- Documentation and examples should be updated with each implementation
- Tests should cover both enabled and disabled telemetry scenarios
