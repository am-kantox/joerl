//! Supervision trees for fault-tolerant systems.
//!
//! Supervisors monitor child actors and restart them according to
//! configurable strategies when they fail. This implements the
//! Erlang/OTP supervisor behavior.
//!
//! # Overview
//!
//! Supervisors are special actors that monitor their children and restart them
//! when they fail. This provides fault tolerance by automatically recovering
//! from errors without human intervention.
//!
//! # Restart Strategies
//!
//! - **OneForOne**: Only the failed child is restarted
//! - **OneForAll**: All children are restarted when one fails
//! - **RestForOne**: The failed child and all children started after it are restarted
//!
//! # Example
//!
//! ```rust
//! use joerl::{
//!     Actor, ActorContext, ActorSystem, Message,
//!     supervisor::{ChildSpec, RestartStrategy, SupervisorSpec, spawn_supervisor},
//! };
//! use async_trait::async_trait;
//! use std::sync::Arc;
//!
//! struct Worker { id: String }
//!
//! #[async_trait]
//! impl Actor for Worker {
//!     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {
//!         // Worker logic here
//!     }
//! }
//!
//! # tokio_test::block_on(async {
//! let system = Arc::new(ActorSystem::new());
//!
//! let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
//!     .child(ChildSpec::new("worker1", || Box::new(Worker { id: "1".into() })))
//!     .child(ChildSpec::new("worker2", || Box::new(Worker { id: "2".into() })));
//!
//! let supervisor = spawn_supervisor(&system, spec);
//! # });
//! ```

use crate::Pid;
use crate::actor::{Actor, ActorContext};
use crate::message::{ExitReason, Message, Signal};
use crate::system::{ActorRef, ActorSystem};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Restart strategy for supervised actors.
///
/// These strategies mirror Erlang/OTP supervisor strategies and determine
/// which children should be restarted when a child terminates abnormally.
///
/// # Examples
///
/// ```rust
/// use joerl::supervisor::RestartStrategy;
///
/// let strategy = RestartStrategy::OneForOne;
/// assert_eq!(strategy, RestartStrategy::OneForOne);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartStrategy {
    /// Restart only the failed child (default).
    ///
    /// This is the most common strategy. When a child terminates abnormally,
    /// only that child is restarted. Other children continue running normally.
    ///
    /// **Use when**: Children are independent and don't depend on each other.
    ///
    /// In Erlang: `one_for_one`
    OneForOne,

    /// Restart all children when one fails.
    ///
    /// When any child terminates abnormally, all children are terminated
    /// and then all are restarted. This ensures a consistent state across
    /// all children.
    ///
    /// **Use when**: Children are tightly coupled and must maintain consistency.
    ///
    /// In Erlang: `one_for_all`
    OneForAll,

    /// Restart the failed child and all children started after it.
    ///
    /// When a child terminates abnormally, that child and all children that
    /// were started after it (in the child list order) are terminated and
    /// restarted. Children started before the failed child continue running.
    ///
    /// **Use when**: Children have dependencies where later children depend on earlier ones.
    ///
    /// In Erlang: `rest_for_one`
    RestForOne,
}

/// Restart intensity limits to prevent infinite restart loops.
///
/// The supervisor will terminate itself if a child is restarted more than
/// `max_restarts` times within `within_seconds` seconds. This prevents
/// resource exhaustion from repeatedly failing children.
///
/// # Examples
///
/// ```rust
/// use joerl::supervisor::RestartIntensity;
///
/// // Default: 3 restarts within 5 seconds
/// let default = RestartIntensity::default();
/// assert_eq!(default.max_restarts, 3);
/// assert_eq!(default.within_seconds, 5);
///
/// // Custom: more permissive
/// let custom = RestartIntensity {
///     max_restarts: 10,
///     within_seconds: 60,
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct RestartIntensity {
    /// Maximum number of restarts within the time period.
    pub max_restarts: usize,
    /// Time period in seconds.
    pub within_seconds: u64,
}

impl Default for RestartIntensity {
    /// Creates default restart intensity limits.
    ///
    /// Default is 3 restarts within 5 seconds, which is a reasonable
    /// balance for most applications.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::supervisor::RestartIntensity;
    ///
    /// let intensity = RestartIntensity::default();
    /// assert_eq!(intensity.max_restarts, 3);
    /// assert_eq!(intensity.within_seconds, 5);
    /// ```
    fn default() -> Self {
        Self {
            max_restarts: 3,
            within_seconds: 5,
        }
    }
}

/// Child specification for supervised actors.
///
/// A child spec describes how to start a child actor. The `start` function
/// is a factory that creates a new instance of the actor each time it needs
/// to be started or restarted.
///
/// In Erlang: child_spec()
///
/// # Examples
///
/// ```rust
/// use joerl::{Actor, ActorContext, Message, supervisor::ChildSpec};
/// use async_trait::async_trait;
///
/// struct MyWorker;
///
/// #[async_trait]
/// impl Actor for MyWorker {
///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
/// }
///
/// let spec = ChildSpec::new("my_worker", || Box::new(MyWorker));
/// ```
pub struct ChildSpec {
    /// Unique identifier for the child.
    pub id: String,
    /// Factory function to create the child actor.
    pub start: Box<dyn FnMut() -> Box<dyn Actor> + Send>,
}

impl ChildSpec {
    /// Creates a new child specification.
    ///
    /// # Arguments
    ///
    /// * `id` - A unique identifier for the child within this supervisor
    /// * `start` - A factory function that creates a new actor instance
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message, supervisor::ChildSpec};
    /// use async_trait::async_trait;
    ///
    /// struct Counter { count: i32 }
    ///
    /// #[async_trait]
    /// impl Actor for Counter {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// // Factory function creates a fresh instance each time
    /// let spec = ChildSpec::new("counter", || Box::new(Counter { count: 0 }));
    /// ```
    pub fn new(
        id: impl Into<String>,
        start: impl FnMut() -> Box<dyn Actor> + Send + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            start: Box::new(start),
        }
    }
}

/// Supervisor specification.
///
/// Describes how a supervisor should behave, including its restart strategy,
/// restart intensity limits, and the children it should supervise.
///
/// In Erlang: supervisor:init/1 return value
///
/// # Examples
///
/// ```rust
/// use joerl::{
///     Actor, ActorContext, Message,
///     supervisor::{ChildSpec, RestartIntensity, RestartStrategy, SupervisorSpec},
/// };
/// use async_trait::async_trait;
///
/// struct Worker;
///
/// #[async_trait]
/// impl Actor for Worker {
///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
/// }
///
/// let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
///     .intensity(RestartIntensity {
///         max_restarts: 5,
///         within_seconds: 10,
///     })
///     .child(ChildSpec::new("worker1", || Box::new(Worker)))
///     .child(ChildSpec::new("worker2", || Box::new(Worker)));
/// ```
pub struct SupervisorSpec {
    /// Restart strategy.
    pub strategy: RestartStrategy,
    /// Restart intensity limits.
    pub intensity: RestartIntensity,
    /// Child specifications.
    pub children: Vec<ChildSpec>,
}

impl SupervisorSpec {
    /// Creates a new supervisor specification with the given restart strategy.
    ///
    /// The supervisor starts with default restart intensity (3 restarts within 5 seconds)
    /// and no children. Use the builder methods [`child`](Self::child) and
    /// [`intensity`](Self::intensity) to configure it further.
    ///
    /// # Arguments
    ///
    /// * `strategy` - The restart strategy to use (OneForOne, OneForAll, or RestForOne)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::supervisor::{RestartStrategy, SupervisorSpec};
    ///
    /// let spec = SupervisorSpec::new(RestartStrategy::OneForOne);
    /// assert_eq!(spec.strategy, RestartStrategy::OneForOne);
    /// assert_eq!(spec.children.len(), 0);
    /// ```
    pub fn new(strategy: RestartStrategy) -> Self {
        Self {
            strategy,
            intensity: RestartIntensity::default(),
            children: Vec::new(),
        }
    }

    /// Adds a child to the supervisor.
    ///
    /// Children are started in the order they are added. For `RestForOne` strategy,
    /// this order determines which children are restarted together.
    ///
    /// # Arguments
    ///
    /// * `spec` - The child specification
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{
    ///     Actor, ActorContext, Message,
    ///     supervisor::{ChildSpec, RestartStrategy, SupervisorSpec},
    /// };
    /// use async_trait::async_trait;
    ///
    /// struct Worker { id: String }
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
    ///     .child(ChildSpec::new("worker1", || Box::new(Worker { id: "1".into() })))
    ///     .child(ChildSpec::new("worker2", || Box::new(Worker { id: "2".into() })));
    ///
    /// assert_eq!(spec.children.len(), 2);
    /// ```
    pub fn child(mut self, spec: ChildSpec) -> Self {
        self.children.push(spec);
        self
    }

    /// Sets the restart intensity limits.
    ///
    /// If a child exceeds these limits, the supervisor will terminate itself
    /// rather than continuing to restart the failing child.
    ///
    /// # Arguments
    ///
    /// * `intensity` - The restart intensity limits
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::supervisor::{RestartIntensity, RestartStrategy, SupervisorSpec};
    ///
    /// // Allow more restarts for a less stable environment
    /// let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
    ///     .intensity(RestartIntensity {
    ///         max_restarts: 10,
    ///         within_seconds: 60,
    ///     });
    ///
    /// assert_eq!(spec.intensity.max_restarts, 10);
    /// ```
    pub fn intensity(mut self, intensity: RestartIntensity) -> Self {
        self.intensity = intensity;
        self
    }
}

/// Internal child state.
struct Child {
    #[allow(dead_code)]
    id: String,
    pid: Pid,
    start_factory: Box<dyn FnMut() -> Box<dyn Actor> + Send>,
    restart_times: Vec<Instant>,
}

/// Supervisor actor that monitors and restarts child actors.
///
/// Supervisors are typically created using [`spawn_supervisor`] rather than
/// being spawned directly. They automatically start their children when they
/// are started and monitor them for failures.
///
/// # Lifecycle
///
/// 1. Supervisor is spawned with a [`SupervisorSpec`]
/// 2. In `started()`, all children are spawned and monitored
/// 3. When a child exits abnormally, supervisor applies its restart strategy
/// 4. If restart intensity is exceeded, supervisor terminates
///
/// See the module documentation and examples for complete usage patterns.
pub struct Supervisor {
    strategy: RestartStrategy,
    intensity: RestartIntensity,
    children: HashMap<String, Child>,
    child_order: Vec<String>,            // Maintain insertion order
    child_specs: Option<Vec<ChildSpec>>, // Stored until started
    system: Arc<ActorSystem>,
}

impl Supervisor {
    /// Creates a new supervisor from a specification.
    ///
    /// This is typically called internally by [`spawn_supervisor`]. The supervisor
    /// will start its children when its `started()` hook is called by the actor system.
    ///
    /// # Arguments
    ///
    /// * `spec` - The supervisor specification
    /// * `system` - The actor system to spawn children in
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, supervisor::{RestartStrategy, SupervisorSpec, Supervisor}};
    /// use std::sync::Arc;
    ///
    /// let system = Arc::new(ActorSystem::new());
    /// let spec = SupervisorSpec::new(RestartStrategy::OneForOne);
    /// let supervisor = Supervisor::from_spec(spec, Arc::clone(&system));
    /// ```
    pub fn from_spec(spec: SupervisorSpec, system: Arc<ActorSystem>) -> Self {
        Self {
            strategy: spec.strategy,
            intensity: spec.intensity,
            children: HashMap::new(),
            child_order: Vec::new(),
            child_specs: Some(spec.children),
            system,
        }
    }

    /// Starts all children.
    fn start_children(&mut self, mut specs: Vec<ChildSpec>, ctx: &mut ActorContext) {
        for mut spec in specs.drain(..) {
            let child_actor = (spec.start)();
            let actor_ref = self.system.spawn_boxed(child_actor);
            let pid = actor_ref.pid();

            // Monitor the child
            let _ = actor_ref.monitor(ctx.pid());

            let child = Child {
                id: spec.id.clone(),
                pid,
                start_factory: spec.start,
                restart_times: Vec::new(),
            };

            self.children.insert(spec.id.clone(), child);
            self.child_order.push(spec.id);
        }
    }

    /// Restarts a child according to the strategy.
    async fn handle_child_exit(
        &mut self,
        child_pid: Pid,
        reason: &ExitReason,
        ctx: &mut ActorContext,
    ) {
        // Find which child exited
        let child_id = self
            .children
            .iter()
            .find(|(_, child)| child.pid == child_pid)
            .map(|(id, _)| id.clone());

        if let Some(child_id) = child_id {
            tracing::warn!("Child {} (pid {}) exited: {}", child_id, child_pid, reason);

            if reason.is_normal() {
                // Normal exit, don't restart
                return;
            }

            match self.strategy {
                RestartStrategy::OneForOne => {
                    self.restart_child(&child_id, ctx).await;
                }
                RestartStrategy::OneForAll => {
                    self.restart_all_children(ctx).await;
                }
                RestartStrategy::RestForOne => {
                    self.restart_from_child(&child_id, ctx).await;
                }
            }
        }
    }

    /// Restarts a single child.
    async fn restart_child(&mut self, child_id: &str, ctx: &mut ActorContext) {
        if let Some(child) = self.children.get_mut(child_id) {
            // Check restart intensity
            let now = Instant::now();
            let cutoff = now - Duration::from_secs(self.intensity.within_seconds);
            child.restart_times.retain(|&t| t > cutoff);

            if child.restart_times.len() >= self.intensity.max_restarts {
                tracing::error!(
                    "Child {} exceeded restart intensity, stopping supervisor",
                    child_id
                );
                ctx.stop(ExitReason::Custom(format!(
                    "restart intensity exceeded for {}",
                    child_id
                )));
                return;
            }

            child.restart_times.push(now);

            // Spawn new child
            let child_actor = (child.start_factory)();
            let actor_ref = self.system.spawn_boxed(child_actor);
            let new_pid = actor_ref.pid();

            // Monitor the new child
            let _ = actor_ref.monitor(ctx.pid());

            child.pid = new_pid;
            tracing::info!("Restarted child {} with new pid {}", child_id, new_pid);
        }
    }

    /// Restarts all children.
    async fn restart_all_children(&mut self, ctx: &mut ActorContext) {
        for child_id in self.child_order.clone() {
            self.restart_child(&child_id, ctx).await;
        }
    }

    /// Restarts a child and all children started after it.
    async fn restart_from_child(&mut self, from_id: &str, ctx: &mut ActorContext) {
        let mut should_restart = false;
        for child_id in self.child_order.clone() {
            if child_id == from_id {
                should_restart = true;
            }
            if should_restart {
                self.restart_child(&child_id, ctx).await;
            }
        }
    }
}

#[async_trait]
impl Actor for Supervisor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        ctx.trap_exit(true);
        tracing::info!("Supervisor {} started", ctx.pid());

        // Start all children
        if let Some(specs) = self.child_specs.take() {
            self.start_children(specs, ctx);
        }
    }

    async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {
        // Supervisors typically don't handle user messages
    }

    async fn handle_signal(&mut self, signal: Signal, ctx: &mut ActorContext) {
        match signal {
            Signal::Down { pid, reason, .. } => {
                self.handle_child_exit(pid, &reason, ctx).await;
            }
            Signal::Exit { from, reason } => {
                // Child exited (if we're linked)
                self.handle_child_exit(from, &reason, ctx).await;
            }
            Signal::Stop => {
                ctx.stop(ExitReason::Shutdown);
            }
            Signal::Kill => {
                ctx.stop(ExitReason::Killed);
            }
        }
    }

    async fn stopped(&mut self, _reason: &ExitReason, ctx: &mut ActorContext) {
        tracing::info!("Supervisor {} stopping, terminating children", ctx.pid());
        // Children will be cleaned up automatically when the system detects
        // the supervisor has stopped
    }
}

/// Spawns a supervisor with the given specification.
///
/// This is the recommended way to create and start a supervisor. The supervisor
/// will automatically start all its children when it is initialized.
///
/// # Arguments
///
/// * `system` - The actor system to spawn the supervisor in
/// * `spec` - The supervisor specification defining behavior and children
///
/// # Returns
///
/// An [`ActorRef`] to the spawned supervisor
///
/// # Examples
///
/// ```rust
/// use joerl::{
///     Actor, ActorContext, ActorSystem, Message,
///     supervisor::{ChildSpec, RestartStrategy, SupervisorSpec, spawn_supervisor},
/// };
/// use async_trait::async_trait;
/// use std::sync::Arc;
///
/// struct Worker { id: String }
///
/// #[async_trait]
/// impl Actor for Worker {
///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
/// }
///
/// # tokio_test::block_on(async {
/// let system = Arc::new(ActorSystem::new());
///
/// // Create supervisor with two workers
/// let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
///     .child(ChildSpec::new("worker1", || Box::new(Worker { id: "1".into() })))
///     .child(ChildSpec::new("worker2", || Box::new(Worker { id: "2".into() })));
///
/// let supervisor = spawn_supervisor(&system, spec);
///
/// // Supervisor is now running and monitoring its children
/// assert!(system.is_alive(supervisor.pid()));
/// # });
/// ```
pub fn spawn_supervisor(system: &Arc<ActorSystem>, spec: SupervisorSpec) -> ActorRef {
    let system_clone = Arc::clone(system);
    system.spawn(Supervisor::from_spec(spec, system_clone))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restart_strategy() {
        assert_eq!(RestartStrategy::OneForOne, RestartStrategy::OneForOne);
        assert_ne!(RestartStrategy::OneForOne, RestartStrategy::OneForAll);
    }

    #[test]
    fn test_restart_intensity_default() {
        let intensity = RestartIntensity::default();
        assert_eq!(intensity.max_restarts, 3);
        assert_eq!(intensity.within_seconds, 5);
    }

    #[test]
    fn test_supervisor_spec() {
        let spec = SupervisorSpec::new(RestartStrategy::OneForOne).intensity(RestartIntensity {
            max_restarts: 5,
            within_seconds: 10,
        });

        assert_eq!(spec.strategy, RestartStrategy::OneForOne);
        assert_eq!(spec.intensity.max_restarts, 5);
    }

    #[tokio::test]
    async fn test_child_spec() {
        struct TestActor;

        #[async_trait]
        impl Actor for TestActor {
            async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
        }

        let _spec = ChildSpec::new("test", || Box::new(TestActor) as Box<dyn Actor>);
    }
}
