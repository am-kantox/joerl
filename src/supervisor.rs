//! Supervision trees for fault-tolerant systems.
//!
//! Supervisors monitor child actors and restart them according to
//! configurable strategies when they fail. This implements the
//! Erlang/OTP supervisor behavior.

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
/// These strategies mirror Erlang/OTP supervisor strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartStrategy {
    /// Restart only the failed child (default).
    ///
    /// In Erlang: `one_for_one`
    OneForOne,

    /// Restart all children when one fails.
    ///
    /// In Erlang: `one_for_all`
    OneForAll,

    /// Restart the failed child and all children started after it.
    ///
    /// In Erlang: `rest_for_one`
    RestForOne,
}

/// Restart intensity limits to prevent infinite restart loops.
#[derive(Debug, Clone, Copy)]
pub struct RestartIntensity {
    /// Maximum number of restarts within the time period.
    pub max_restarts: usize,
    /// Time period in seconds.
    pub within_seconds: u64,
}

impl Default for RestartIntensity {
    fn default() -> Self {
        Self {
            max_restarts: 3,
            within_seconds: 5,
        }
    }
}

/// Child specification for supervised actors.
///
/// In Erlang: child_spec()
pub struct ChildSpec {
    /// Unique identifier for the child.
    pub id: String,
    /// Factory function to create the child actor.
    pub start: Box<dyn FnMut() -> Box<dyn Actor> + Send>,
}

impl ChildSpec {
    /// Creates a new child specification.
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
/// In Erlang: supervisor:init/1 return value
pub struct SupervisorSpec {
    /// Restart strategy.
    pub strategy: RestartStrategy,
    /// Restart intensity limits.
    pub intensity: RestartIntensity,
    /// Child specifications.
    pub children: Vec<ChildSpec>,
}

impl SupervisorSpec {
    /// Creates a new supervisor specification.
    pub fn new(strategy: RestartStrategy) -> Self {
        Self {
            strategy,
            intensity: RestartIntensity::default(),
            children: Vec::new(),
        }
    }

    /// Adds a child to the supervisor.
    pub fn child(mut self, spec: ChildSpec) -> Self {
        self.children.push(spec);
        self
    }

    /// Sets the restart intensity.
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
/// # Examples
///
/// # Examples
///
/// Supervisors monitor and restart child actors according to strategies.
/// See the module documentation for complete examples.
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

/// Helper function to spawn a supervisor.
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
