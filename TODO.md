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

### 3. GenServer & GenStatem Metrics
**Status**: Not started  
**Priority**: Immediate  
**Value**: Track FSM behavior, identify stuck states, measure synchronous call latency

**Metrics to add**:

**GenServer**:
- `joerl_genserver_call_duration_seconds{server_type}` (histogram)
- `joerl_genserver_cast_sent_total{server_type}`
- `joerl_genserver_call_timeout_total{server_type}`
- `joerl_genserver_calls_in_flight{server_type}` (gauge)

**GenStatem**:
- `joerl_genstatem_state_transitions_total{fsm_type, from_state, to_state}`
- `joerl_genstatem_invalid_transitions_total{fsm_type, state, event}`
- `joerl_genstatem_state_duration_seconds{fsm_type, state}` (histogram)
- `joerl_genstatem_current_state{fsm_type, state}` (gauge)

**Files to modify**:
- `joerl/src/gen_server.rs`
- `joerl/src/gen_statem.rs`

---

## 🟡 Important Enhancements

### 4. Per-Actor Mailbox Tracking
**Status**: Not started  
**Priority**: Immediate  
**Value**: Identify backpressure hotspots, find slow consumers

**Implementation**:
- [ ] Change `mailbox_depth` gauge to include `actor_type` label
- [ ] Track max mailbox depth per actor type
- [ ] Add `joerl_mailbox_depth_max{actor_type}` gauge
- [ ] Add `joerl_mailbox_utilization{actor_type}` gauge (depth/capacity)

---

### 5. Signal-Specific Metrics
**Status**: Not started  
**Priority**: Short-term  
**Value**: Debug supervision trees, understand failure propagation patterns

**Metrics to add**:
- `joerl_signals_sent_total{signal_type}` (exit, down, stop, kill)
- `joerl_signals_received_total{signal_type}`
- `joerl_signals_ignored_total{signal_type}` (when trap_exit=true)
- `joerl_exit_signals_by_reason{reason}` (breakdown of exit reasons)

---

### 6. Actor Lifetime Statistics
**Status**: Not started  
**Priority**: Short-term  
**Value**: Identify actors that die too quickly (restart loops) or leak (never die)

**Metrics to add**:
- `joerl_actor_lifetime_seconds{actor_type}` (histogram)
- `joerl_actor_lifetime_total_seconds{actor_type}` (cumulative)
- `joerl_short_lived_actors_total{actor_type}` (lived < 1s)

---

### 7. Message Queue Wait Time
**Status**: Not started  
**Priority**: Medium-term  
**Value**: Distinguish between slow processing vs. queue backlog

**Implementation**:
- [ ] Add timestamp to `Envelope` when enqueued
- [ ] Calculate wait time when dequeued
- [ ] Add `joerl_message_queue_wait_seconds` histogram
- [ ] Compare with `joerl_message_processing_duration_seconds`

---

### 8. Sampling for High-Frequency Operations
**Status**: Not started  
**Priority**: Medium-term  
**Value**: Reduce overhead in high-throughput scenarios

**Implementation**:
- [ ] Add `TelemetryConfig` struct with sampling rate
- [ ] Implement probabilistic sampling (e.g., 1% of messages)
- [ ] Add configuration for which metrics to sample
- [ ] Document performance characteristics with/without sampling

---

## 🟢 Nice-to-Have Enhancements

### 9. Health Check Endpoint
**Status**: Not started  
**Priority**: Long-term  
**Value**: Kubernetes liveness/readiness probes, automated alerting

**Implementation**:
- [ ] Add `SystemHealth` struct with health indicators
- [ ] Implement `is_healthy()` based on metrics thresholds
- [ ] Add HTTP endpoint (optional feature) for `/health` and `/ready`
- [ ] Document integration with Kubernetes

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

- **Completed**: 1/12 (8.3%)
- **In Progress**: 0/12
- **Not Started**: 11/12

---

## Priority Order for Implementation

### Phase 1: Core Observability (Immediate)
1. ✅ **Actor-Level Labels/Metadata** (#1) - IN PROGRESS
2. **Per-Actor Mailbox Tracking** (#4)
3. **GenServer/GenStatem Metrics** (#3)

### Phase 2: Production Readiness (Short-term)
4. **Distributed System Metrics** (#2)
5. **Signal-Specific Metrics** (#5)
6. **Actor Lifetime Statistics** (#6)

### Phase 3: Performance & Optimization (Medium-term)
7. **Sampling Configuration** (#8)
8. **Message Queue Wait Time** (#7)

### Phase 4: Advanced Features (Long-term)
9. **OpenTelemetry Span Integration** (#10)
10. **Health Check Endpoint** (#9)
11. **Memory Usage Tracking** (#11)
12. **Custom Metric Registry** (#12)

---

## Notes

- All enhancements should maintain backward compatibility
- Feature flags should be considered for optional heavy features
- Performance impact should be measured for each enhancement
- Documentation and examples should be updated with each implementation
- Tests should cover both enabled and disabled telemetry scenarios
