# Overview
Implement Erlang-style named process registry and scheduled messaging to make joerl more feature-complete for Erlang developers. This includes:
* `whereis/1` - Look up processes by registered name
* `register/2` - Register a process with a name
* `unregister/1` - Unregister a named process
* `send/2` - Send to named process or Pid
* `send_after/3` - Schedule a message to be sent after a delay
# Current State
joerl currently only supports addressing actors by Pid. The ActorSystem maintains a DashMap registry mapping Pid to ActorEntry (joerl/src/system.rs:279). Messages are sent via `ActorSystem::send()` and `ActorRef::send()` methods which only accept Pid as the recipient.
There is no timer/scheduler subsystem, and all message sending is immediate.
# Proposed Changes
## 1. Create Named Registry Module
Create joerl/src/registry.rs for name registration functionality.
### Registry Structure
* Use `DashMap<String, Pid>` for thread-safe name-to-Pid mapping
* Use `DashMap<Pid, String>` for reverse lookup (Pid-to-name)
* Support both local and distributed names (node-scoped)
* Automatic cleanup when actors terminate
### Key Types
* `RegisteredName` - Newtype wrapper for registered names with validation
* `RegistrationError` - Specific errors for name conflicts, invalid names
### Core Functions
* `register(name: String, pid: Pid) -> Result<()>` - Register name (error if already registered)
* `unregister(name: &str) -> Result<Pid>` - Remove registration, return Pid
* `whereis(name: &str) -> Option<Pid>` - Look up Pid by name
* `registered() -> Vec<String>` - List all registered names
* `cleanup_pid(pid: Pid)` - Internal cleanup when actor dies
## 2. Integrate Registry into ActorSystem
Add registry field to ActorSystem (joerl/src/system.rs:278).
### Changes to ActorSystem
* Add `registry: Arc<Registry>` field
* Initialize registry in `new()` and `new_distributed()`
* Expose public API methods that delegate to registry:
    * `register(&self, name: impl Into<String>, pid: Pid) -> Result<()>`
    * `unregister(&self, name: &str) -> Result<Pid>`
    * `whereis(&self, name: &str) -> Option<Pid>`
    * `registered(&self) -> Vec<String>`
* Call `registry.cleanup_pid()` in `cleanup_actor()` (system.rs:717)
### Named Send Support
* Create `Destination` enum: `Pid(Pid) | Name(String)`
* Add `send_to(&self, dest: Destination, msg: Message) -> Result<()>`
* Modify existing `send()` to work through this abstraction
* Update ActorRef to support named sends
## 3. Create Scheduler Module
Create joerl/src/scheduler.rs for delayed message scheduling.
### Scheduler Structure
* Use tokio's time utilities (`tokio::time::sleep`, `Instant`)
* Store scheduled messages as heap-ordered by delivery time
* Each scheduled message spawns a tokio task that sleeps then sends
### Key Types
* `ScheduledMessage` - Contains destination, message, delivery time
* `SchedulerHandle` - Cancellable reference to scheduled message
* `TimerRef` - Erlang-style timer reference for cancellation
### Core Functions
* `send_after(dest: Destination, msg: Message, duration: Duration) -> TimerRef`
* `cancel_timer(timer_ref: TimerRef) -> Result<bool>` - Returns true if cancelled
* `send_interval(dest: Destination, msg: Message, interval: Duration) -> TimerRef` (bonus)
### Implementation Strategy
* Each `send_after` spawns a detached tokio task
* Task sleeps for duration, then calls `ActorSystem::send_to()`
* Store cancellation tokens in `DashMap<TimerRef, CancellationToken>`
* Use `tokio_util::sync::CancellationToken` for cancellation
## 4. Update Message Types
Add timer-related message types to joerl/src/message.rs if needed for timer expiry notifications.
## 5. Extend ActorContext
Add convenience methods to ActorContext (joerl/src/actor.rs:100):
* `send_after(&self, dest: Destination, msg: Message, duration: Duration) -> Result<TimerRef>`
* `whereis(&self, name: &str) -> Option<Pid>`
* Allow actors to easily schedule messages from within their context
## 6. Update Error Types
Add new error variants to ActorError (joerl/src/error.rs):
* `NameAlreadyRegistered(String)`
* `NameNotRegistered(String)`
* `InvalidName(String)`
* `TimerNotFound(u64)`
## 7. Telemetry Integration
Add metrics for registry and scheduler (if telemetry feature enabled):
* `joerl_registry_size` - Current number of registered names
* `joerl_registry_lookups_total` - Registry lookup counter
* `joerl_registry_conflicts_total` - Registration conflict counter
* `joerl_scheduled_messages_total` - Messages scheduled
* `joerl_scheduled_messages_cancelled_total` - Timers cancelled
## 8. Documentation and Examples
### Update README.md
Add section on named processes and scheduled messaging with examples.
### Create Examples
* `examples/named_processes.rs` - Registry demonstration
* `examples/scheduled_messaging.rs` - Timer demonstration
* Update existing examples to show optional use of names
### Update Erlang Terminology Mapping
Add to README.md (line 301):
* `register(Name, Pid)` -> `system.register(name, pid)`
* `unregister(Name)` -> `system.unregister(name)`
* `whereis(Name)` -> `system.whereis(name)`
* `registered()` -> `system.registered()`
* `erlang:send_after(Time, Pid, Msg)` -> `system.send_after(dest, msg, duration)`
* `erlang:cancel_timer(TRef)` -> `system.cancel_timer(tref)`
### Update WARP.md
Document the new registry and scheduler modules in the Architecture section.
## 9. Testing
### Registry Tests
* Test basic register/unregister/whereis
* Test name conflicts (should error)
* Test automatic cleanup on actor death
* Test concurrent registration attempts
* Test distributed name registration (node-scoped)
* Property tests for registry consistency
### Scheduler Tests
* Test basic send_after delivery
* Test timer cancellation before delivery
* Test message ordering with multiple scheduled messages
* Test scheduled message to named destination
* Test scheduled messages survive actor restarts
* Test timer behaviour when destination dies
### Integration Tests
* Named processes with supervision trees
* Scheduled messages in distributed systems
* Registry with monitors and links
## 10. Export Public API
Update joerl/src/lib.rs:
* Add `pub mod registry;`
* Add `pub mod scheduler;`
* Re-export key types: `RegisteredName`, `Destination`, `TimerRef`
* Update documentation with new capabilities
