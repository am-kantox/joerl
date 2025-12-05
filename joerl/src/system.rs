//! Actor system for managing the lifecycle of actors.
//!
//! The ActorSystem is responsible for:
//! - Spawning new actors
//! - Maintaining a registry of active actors
//! - Routing messages between actors
//! - Managing actor lifecycle (links, monitors)

use crate::Pid;
use crate::actor::{Actor, ActorContext};
use crate::error::{ActorError, Result};
use crate::mailbox::{DEFAULT_MAILBOX_CAPACITY, Mailbox, MailboxSender};
use crate::message::{Envelope, ExitReason, Message, MonitorRef, Signal};
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::task::JoinHandle;

static MONITOR_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Handle to a spawned actor.
///
/// The ActorRef allows you to:
/// - Send messages to the actor
/// - Link to or monitor the actor
/// - Wait for the actor to terminate
pub struct ActorRef {
    pid: Pid,
    system: Arc<ActorSystem>,
    join_handle: Option<JoinHandle<ExitReason>>,
}

impl ActorRef {
    /// Returns the Pid of the actor.
    ///
    /// The Pid can be used to identify the actor, send messages via the system,
    /// or establish links and monitors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct TestActor;
    ///
    /// #[async_trait]
    /// impl Actor for TestActor {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let actor_ref = system.spawn(TestActor);
    /// let pid = actor_ref.pid();
    /// println!("Actor PID: {}", pid);
    /// # });
    /// ```
    pub fn pid(&self) -> Pid {
        self.pid
    }

    /// Sends a message to the actor.
    ///
    /// Returns an error if the actor is no longer alive or the mailbox is full.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use joerl::{ActorSystem, ActorRef};
    /// # async fn example(actor_ref: ActorRef) {
    /// actor_ref.send(Box::new("Hello".to_string())).await;
    /// # }
    /// ```
    pub async fn send(&self, msg: Message) -> Result<()> {
        self.system.send(self.pid, msg).await
    }

    /// Attempts to send a message without blocking.
    pub fn try_send(&self, msg: Message) -> Result<()> {
        self.system.try_send(self.pid, msg)
    }

    /// Links this actor to another actor (bidirectional).
    ///
    /// Creates a bidirectional link between two actors. If either actor terminates
    /// abnormally, the other will receive an EXIT signal (and terminate unless
    /// trapping exits).
    ///
    /// In Erlang: `link(Pid)`
    ///
    /// # Arguments
    ///
    /// * `from` - The Pid of the actor to link from
    ///
    /// # Errors
    ///
    /// Returns `ActorError::ActorNotFound` if either actor doesn't exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let worker1 = system.spawn(Worker);
    /// let worker2 = system.spawn(Worker);
    ///
    /// // Link worker1 to worker2
    /// worker2.link(worker1.pid()).expect("Failed to link");
    /// // Now if either fails, both will be notified
    /// # });
    /// ```
    pub fn link(&self, from: Pid) -> Result<()> {
        self.system.link(from, self.pid)
    }

    /// Monitors this actor (unidirectional).
    ///
    /// Creates a unidirectional monitor. When this actor terminates, the
    /// monitoring actor receives a DOWN signal but is not affected.
    ///
    /// Returns a MonitorRef that will be included in the DOWN message,
    /// allowing the monitor to identify which actor terminated.
    ///
    /// In Erlang: `monitor(process, Pid)`
    ///
    /// # Arguments
    ///
    /// * `from` - The Pid of the monitoring actor
    ///
    /// # Errors
    ///
    /// Returns `ActorError::ActorNotFound` if either actor doesn't exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message, Signal};
    /// use async_trait::async_trait;
    ///
    /// struct Observer;
    ///
    /// #[async_trait]
    /// impl Actor for Observer {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    ///     
    ///     async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
    ///         if let Signal::Down { pid, reason, .. } = signal {
    ///             println!("Monitored actor {} terminated: {}", pid, reason);
    ///         }
    ///     }
    /// }
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let observer = system.spawn(Observer);
    /// let worker = system.spawn(Worker);
    ///
    /// // Observer monitors worker
    /// let monitor_ref = worker.monitor(observer.pid()).expect("Failed to monitor");
    /// # });
    /// ```
    pub fn monitor(&self, from: Pid) -> Result<MonitorRef> {
        self.system.monitor(from, self.pid)
    }

    /// Waits for the actor to terminate and returns the exit reason.
    ///
    /// This consumes the ActorRef and blocks until the actor has fully
    /// terminated, returning the reason it stopped.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use joerl::{ActorSystem, Actor, ActorContext, Message, ExitReason};
    /// use async_trait::async_trait;
    ///
    /// struct ShortLived;
    ///
    /// #[async_trait]
    /// impl Actor for ShortLived {
    ///     async fn started(&mut self, ctx: &mut ActorContext) {
    ///         ctx.stop(ExitReason::Normal);
    ///     }
    ///     
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # async fn example() {
    /// let system = ActorSystem::new();
    /// let actor = system.spawn(ShortLived);
    ///
    /// // Wait for the actor to terminate
    /// let exit_reason = actor.join().await;
    /// assert_eq!(exit_reason, ExitReason::Normal);
    /// # }
    /// ```
    pub async fn join(mut self) -> ExitReason {
        if let Some(handle) = self.join_handle.take() {
            handle
                .await
                .unwrap_or(ExitReason::Panic("Actor task panicked".to_string()))
        } else {
            ExitReason::Normal
        }
    }
}

/// Internal actor entry in the registry.
struct ActorEntry {
    sender: MailboxSender,
    links: HashSet<Pid>,
    monitors: HashSet<(Pid, MonitorRef)>, // (monitoring_pid, ref)
}

/// The actor system runtime.
///
/// This is the main entry point for creating and managing actors.
///
/// # Examples
///
/// ```
/// use joerl::ActorSystem;
///
/// #[tokio::main]
/// async fn main() {
///     let system = ActorSystem::new();
///     // Spawn actors...
/// }
/// ```
pub struct ActorSystem {
    actors: Arc<DashMap<Pid, ActorEntry>>,
}

impl ActorSystem {
    /// Creates a new actor system.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            actors: Arc::new(DashMap::new()),
        })
    }

    /// Spawns a new actor with default mailbox capacity.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use joerl::{Actor, ActorSystem, ActorContext, Message};
    /// # use async_trait::async_trait;
    /// # struct MyActor;
    /// # #[async_trait]
    /// # impl Actor for MyActor {
    /// #     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {}
    /// # }
    /// # async fn example() {
    /// let system = ActorSystem::new();
    /// let actor_ref = system.spawn(MyActor);
    /// # }
    /// ```
    pub fn spawn<A: Actor>(self: &Arc<Self>, actor: A) -> ActorRef {
        self.spawn_with_capacity(actor, DEFAULT_MAILBOX_CAPACITY)
    }

    /// Spawns a boxed actor with default mailbox capacity.
    ///
    /// This is useful for spawning trait objects or when you need dynamic
    /// dispatch for actors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let boxed_actor: Box<dyn Actor> = Box::new(Worker);
    /// let actor_ref = system.spawn_boxed(boxed_actor);
    /// # });
    /// ```
    pub fn spawn_boxed(self: &Arc<Self>, actor: Box<dyn Actor>) -> ActorRef {
        self.spawn_boxed_with_capacity(actor, DEFAULT_MAILBOX_CAPACITY)
    }

    /// Spawns a new actor with a specific mailbox capacity.
    ///
    /// Allows customization of the mailbox size. Larger mailboxes can buffer
    /// more messages but use more memory. Smaller mailboxes provide stronger
    /// backpressure.
    ///
    /// # Arguments
    ///
    /// * `actor` - The actor to spawn
    /// * `capacity` - The mailbox capacity (number of messages)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct HighThroughputActor;
    ///
    /// #[async_trait]
    /// impl Actor for HighThroughputActor {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// // Spawn with larger mailbox for high-throughput scenarios
    /// let actor_ref = system.spawn_with_capacity(HighThroughputActor, 1000);
    /// # });
    /// ```
    pub fn spawn_with_capacity<A: Actor>(self: &Arc<Self>, actor: A, capacity: usize) -> ActorRef {
        self.spawn_internal(Box::new(actor), capacity)
    }

    /// Spawns a boxed actor with a specific mailbox capacity.
    ///
    /// Combines the flexibility of boxed actors with custom mailbox sizing.
    ///
    /// # Arguments
    ///
    /// * `actor` - The boxed actor to spawn
    /// * `capacity` - The mailbox capacity (number of messages)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let boxed: Box<dyn Actor> = Box::new(Worker);
    /// let actor_ref = system.spawn_boxed_with_capacity(boxed, 500);
    /// # });
    /// ```
    pub fn spawn_boxed_with_capacity(
        self: &Arc<Self>,
        actor: Box<dyn Actor>,
        capacity: usize,
    ) -> ActorRef {
        self.spawn_internal(actor, capacity)
    }

    /// Internal spawn implementation.
    fn spawn_internal(self: &Arc<Self>, mut actor: Box<dyn Actor>, capacity: usize) -> ActorRef {
        let pid = Pid::new();
        let (mailbox, sender) = Mailbox::new(capacity);
        let mut ctx = ActorContext::new(pid, mailbox);

        // Register actor
        let entry = ActorEntry {
            sender: sender.clone(),
            links: HashSet::new(),
            monitors: HashSet::new(),
        };
        self.actors.insert(pid, entry);

        let system = Arc::clone(self);
        let join_handle = tokio::spawn(async move {
            // Call started hook
            actor.started(&mut ctx).await;

            // Main message loop
            let exit_reason = loop {
                match ctx.recv().await {
                    Some(Envelope::Message(msg)) => {
                        actor.handle_message(msg, &mut ctx).await;
                    }
                    Some(Envelope::Signal(signal)) => {
                        actor.handle_signal(signal, &mut ctx).await;
                    }
                    None => {
                        // Mailbox closed, exit normally
                        break ExitReason::Normal;
                    }
                }

                // Check if actor should stop
                if ctx.should_stop() {
                    break ctx.stop_reason().cloned().unwrap_or(ExitReason::Normal);
                }
            };

            // Call stopped hook
            actor.stopped(&exit_reason, &mut ctx).await;

            // Cleanup
            system.cleanup_actor(pid, &exit_reason).await;

            exit_reason
        });

        ActorRef {
            pid,
            system: Arc::clone(self),
            join_handle: Some(join_handle),
        }
    }

    /// Sends a message to an actor.
    pub async fn send(&self, to: Pid, msg: Message) -> Result<()> {
        if let Some(entry) = self.actors.get(&to) {
            entry
                .sender
                .send(Envelope::message(msg))
                .await
                .map_err(|_| ActorError::SendFailed(to))
        } else {
            Err(ActorError::ActorNotFound(to))
        }
    }

    /// Attempts to send a message without blocking.
    pub fn try_send(&self, to: Pid, msg: Message) -> Result<()> {
        if let Some(entry) = self.actors.get(&to) {
            entry
                .sender
                .try_send(Envelope::message(msg))
                .map_err(|_| ActorError::SendFailed(to))
        } else {
            Err(ActorError::ActorNotFound(to))
        }
    }

    /// Sends a signal to an actor.
    pub(crate) async fn send_signal(&self, to: Pid, signal: Signal) -> Result<()> {
        if let Some(entry) = self.actors.get(&to) {
            entry
                .sender
                .send(Envelope::signal(signal))
                .await
                .map_err(|_| ActorError::SendFailed(to))
        } else {
            Err(ActorError::ActorNotFound(to))
        }
    }

    /// Links two actors bidirectionally.
    ///
    /// Creates a bidirectional link between two actors. When either actor
    /// terminates abnormally, the other receives an EXIT signal and will
    /// terminate unless it's trapping exits.
    ///
    /// In Erlang: `link(Pid)`
    ///
    /// # Arguments
    ///
    /// * `pid1` - The first actor's Pid
    /// * `pid2` - The second actor's Pid
    ///
    /// # Errors
    ///
    /// Returns `ActorError::ActorNotFound` if either actor doesn't exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let worker1 = system.spawn(Worker);
    /// let worker2 = system.spawn(Worker);
    ///
    /// system.link(worker1.pid(), worker2.pid()).expect("Failed to link");
    /// # });
    /// ```
    pub fn link(&self, pid1: Pid, pid2: Pid) -> Result<()> {
        // Add link in both directions
        if let Some(mut entry) = self.actors.get_mut(&pid1) {
            entry.links.insert(pid2);
        } else {
            return Err(ActorError::ActorNotFound(pid1));
        }

        if let Some(mut entry) = self.actors.get_mut(&pid2) {
            entry.links.insert(pid1);
        } else {
            return Err(ActorError::ActorNotFound(pid2));
        }

        Ok(())
    }

    /// Monitors an actor (unidirectional).
    ///
    /// Creates a unidirectional monitor from one actor to another. When the
    /// monitored actor terminates, the monitoring actor receives a DOWN signal
    /// with the monitor reference, but is not affected by the termination.
    ///
    /// In Erlang: `monitor(process, Pid)`
    ///
    /// # Arguments
    ///
    /// * `from` - The Pid of the monitoring actor
    /// * `to` - The Pid of the actor to monitor
    ///
    /// # Returns
    ///
    /// A `MonitorRef` that will be included in the DOWN signal
    ///
    /// # Errors
    ///
    /// Returns `ActorError::ActorNotFound` if either actor doesn't exist.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let observer = system.spawn(Worker);
    /// let worker = system.spawn(Worker);
    ///
    /// let monitor_ref = system.monitor(observer.pid(), worker.pid())
    ///     .expect("Failed to monitor");
    /// # });
    /// ```
    pub fn monitor(&self, from: Pid, to: Pid) -> Result<MonitorRef> {
        let monitor_ref = MonitorRef::new(MONITOR_COUNTER.fetch_add(1, Ordering::Relaxed));

        if let Some(mut entry) = self.actors.get_mut(&to) {
            entry.monitors.insert((from, monitor_ref));
            Ok(monitor_ref)
        } else {
            Err(ActorError::ActorNotFound(to))
        }
    }

    /// Returns true if an actor with the given Pid exists and is running.
    ///
    /// This checks if an actor is still registered in the system. Returns false
    /// if the actor never existed or has already terminated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{ActorSystem, Actor, ActorContext, Message, Pid};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    ///
    /// # tokio_test::block_on(async {
    /// let system = ActorSystem::new();
    /// let worker = system.spawn(Worker);
    ///
    /// assert!(system.is_alive(worker.pid()));
    ///
    /// // Non-existent actor
    /// let fake_pid = Pid::new();
    /// assert!(!system.is_alive(fake_pid));
    /// # });
    /// ```
    pub fn is_alive(&self, pid: Pid) -> bool {
        self.actors.contains_key(&pid)
    }

    /// Cleans up an actor after termination.
    async fn cleanup_actor(&self, pid: Pid, reason: &ExitReason) {
        let (links, monitors) = if let Some((_, entry)) = self.actors.remove(&pid) {
            (entry.links, entry.monitors)
        } else {
            return;
        };

        // Send EXIT signals to linked actors
        for linked_pid in links {
            let _ = self
                .send_signal(
                    linked_pid,
                    Signal::Exit {
                        from: pid,
                        reason: reason.clone(),
                    },
                )
                .await;
        }

        // Send DOWN signals to monitoring actors
        for (monitoring_pid, monitor_ref) in monitors {
            let _ = self
                .send_signal(
                    monitoring_pid,
                    Signal::Down {
                        reference: monitor_ref,
                        pid,
                        reason: reason.clone(),
                    },
                )
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Actor;
    use async_trait::async_trait;

    struct TestActor;

    #[async_trait]
    impl Actor for TestActor {
        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    }

    #[tokio::test]
    async fn test_system_new() {
        let system = ActorSystem::new();
        assert!(system.actors.is_empty());
    }

    #[tokio::test]
    async fn test_spawn_actor() {
        let system = ActorSystem::new();
        let actor_ref = system.spawn(TestActor);
        assert!(system.is_alive(actor_ref.pid()));
    }

    #[tokio::test]
    async fn test_send_message() {
        let system = ActorSystem::new();
        let actor_ref = system.spawn(TestActor);

        let result = actor_ref.send(Box::new(42i32)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_to_dead_actor() {
        let system = ActorSystem::new();
        let pid = Pid::new();

        let result = system.send(pid, Box::new(42i32)).await;
        assert!(matches!(result, Err(ActorError::ActorNotFound(_))));
    }

    #[tokio::test]
    async fn test_link_actors() {
        let system = ActorSystem::new();
        let actor1 = system.spawn(TestActor);
        let actor2 = system.spawn(TestActor);

        let result = system.link(actor1.pid(), actor2.pid());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_monitor_actor() {
        let system = ActorSystem::new();
        let actor1 = system.spawn(TestActor);
        let actor2 = system.spawn(TestActor);

        let result = system.monitor(actor1.pid(), actor2.pid());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_is_alive() {
        let system = ActorSystem::new();
        let actor_ref = system.spawn(TestActor);

        assert!(system.is_alive(actor_ref.pid()));

        let dead_pid = Pid::new();
        assert!(!system.is_alive(dead_pid));
    }
}
