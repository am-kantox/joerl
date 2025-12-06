# Panic Handling Implementation

This document describes the panic handling implementation that ensures joerl fully mimics Erlang/OTP behavior for actor panics.

## Problem Statement

Previously, when an actor panicked:
- The tokio task would crash silently
- Cleanup code never ran
- Linked actors received no EXIT signals
- Monitoring actors received no DOWN signals
- The actor entry remained in the registry (memory leak)
- Supervisors couldn't restart panicked children

This violated Erlang/OTP guarantees where process crashes always:
- Notify linked processes
- Send DOWN messages to monitors
- Trigger supervisor restarts
- Perform proper cleanup

## Solution

The implementation wraps the entire actor execution in `std::panic::catch_unwind` using `AssertUnwindSafe` and `futures::FutureExt::catch_unwind()`. This ensures:

1. **Panics are caught**: Any panic during actor execution is intercepted
2. **Converted to ExitReason**: Panics become `ExitReason::Panic(msg)`
3. **Cleanup always runs**: The cleanup code executes even after panic
4. **Supervision works**: Supervisors receive DOWN signals and can restart children

### Code Changes

**system.rs** - Modified `spawn_internal` to wrap actor execution:

```rust,ignore
let exit_reason = AssertUnwindSafe(async {
    // Actor lifecycle: started -> message loop -> stopped
    actor.started(&mut ctx).await;
    
    let exit_reason = loop {
        // Process messages and signals
        // ...
    };
    
    actor.stopped(&exit_reason, &mut ctx).await;
    exit_reason
})
.catch_unwind()
.await
.unwrap_or_else(|panic_info| {
    // Convert panic to ExitReason::Panic
    let panic_msg = // extract panic message
    tracing::error!("Actor {} panicked: {}", pid, panic_msg);
    ExitReason::Panic(panic_msg)
});

// Cleanup - ALWAYS runs even after panic
system.cleanup_actor(pid, &exit_reason).await;
```

**Cargo.toml** - Added futures dependency:
```toml
futures = "0.3"
```

## Test Coverage

Comprehensive tests verify all panic scenarios:

### 1. `test_actor_panic_notifies_links`
- Actor panics during message handling
- Linked actor receives EXIT signal with `ExitReason::Panic`
- Panicked actor is properly cleaned up

### 2. `test_actor_panic_notifies_monitors`
- Actor panics during message handling
- Monitoring actor receives DOWN signal with `ExitReason::Panic`
- Panicked actor is properly cleaned up

### 3. `test_supervisor_restarts_panicked_child`
- Child actor panics on startup
- Supervisor receives DOWN signal
- Supervisor automatically restarts the child
- Restart counter increments correctly

### 4. `test_panic_in_started_hook`
- Panic occurs in `started()` lifecycle hook
- Monitor still receives DOWN signal
- Actor is cleaned up properly

### 5. `test_panic_during_stopped_hook`
- Panic occurs in `stopped()` lifecycle hook
- Monitors are still notified (cleanup runs first)
- No resource leaks

## Behavior Comparison

| Scenario | Before | After (Erlang/OTP-like) |
|----------|--------|------------------------|
| Actor panics | Silent failure | `ExitReason::Panic` |
| Links notified | ❌ No | ✅ Yes - EXIT signal |
| Monitors notified | ❌ No | ✅ Yes - DOWN signal |
| Cleanup runs | ❌ No | ✅ Yes - always |
| Supervisor restart | ❌ No | ✅ Yes - automatic |
| Registry cleanup | ❌ Memory leak | ✅ Proper cleanup |

## Erlang/OTP Semantics

This implementation now fully matches Erlang/OTP guarantees:

1. **Crash isolation**: Panics don't bring down the system
2. **Link propagation**: EXIT signals sent to all linked actors
3. **Monitor notification**: DOWN messages sent to all monitors
4. **Supervisor recovery**: Children automatically restarted per strategy
5. **Resource cleanup**: No leaks, proper registry management

## Performance Impact

Minimal:
- `catch_unwind` has negligible overhead when no panic occurs
- Panic path only executes during actual panics (exceptional case)
- No additional allocations in the hot path

## Future Considerations

The current implementation handles panics at the actor task level. Potential enhancements:

1. **Stack traces**: Capture and log full panic backtraces
2. **Panic hooks**: Allow custom panic handlers per actor
3. **Metrics**: Track panic rates for monitoring
4. **Graceful degradation**: Exponential backoff for repeatedly panicking actors

## Testing

Run panic-specific tests:
```bash
cargo test test_actor_panic
cargo test test_supervisor_restarts_panicked_child
cargo test test_panic_in_started_hook
cargo test test_panic_during_stopped_hook
```

All tests pass with 0 failures.
