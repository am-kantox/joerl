//! Core actor trait and context.
//!
//! Actors are the fundamental unit of computation in joerl. Each actor:
//! - Has a unique Pid
//! - Processes messages sequentially from its mailbox
//! - Can spawn other actors
//! - Can send messages to other actors
//! - Can link to and monitor other actors

use crate::Pid;
use crate::mailbox::Mailbox;
use crate::message::{Envelope, ExitReason, Message, Signal};
use async_trait::async_trait;

/// Actor trait that must be implemented by all actors.
///
/// This trait defines the actor's behavior. The `handle_message` method
/// is called for each message received by the actor.
///
/// # Examples
///
/// ```
/// use joerl::{Actor, ActorContext, Message};
/// use async_trait::async_trait;
///
/// struct MyActor;
///
/// #[async_trait]
/// impl Actor for MyActor {
///     async fn started(&mut self, ctx: &mut ActorContext) {
///         // Initialization logic
///     }
///
///     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
///         if let Some(text) = msg.downcast_ref::<String>() {
///             println!("Received: {}", text);
///         }
///     }
/// }
/// ```
#[async_trait]
pub trait Actor: Send + 'static {
    /// Called when the actor is started, before processing any messages.
    ///
    /// Override this method to perform initialization logic.
    async fn started(&mut self, _ctx: &mut ActorContext) {}

    /// Called when a message is received.
    ///
    /// This is the main message handling method that must be implemented.
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext);

    /// Called when a signal is received.
    ///
    /// Override this to handle EXIT, DOWN, and other system signals.
    /// Default behavior is to stop on EXIT signals if not trapping exits.
    async fn handle_signal(&mut self, signal: Signal, ctx: &mut ActorContext) {
        match signal {
            Signal::Exit { from, reason } => {
                if !ctx.is_trapping_exits() && !reason.is_normal() {
                    tracing::warn!(
                        "Actor {} received EXIT from {}: {}",
                        ctx.pid(),
                        from,
                        reason
                    );
                    ctx.stop(reason);
                }
            }
            Signal::Down {
                reference: _,
                pid,
                reason,
            } => {
                tracing::debug!("Actor {} received DOWN for {}: {}", ctx.pid(), pid, reason);
            }
            Signal::Stop => {
                ctx.stop(ExitReason::Normal);
            }
            Signal::Kill => {
                ctx.stop(ExitReason::Killed);
            }
        }
    }

    /// Called when the actor is stopping.
    ///
    /// Override this to perform cleanup logic. The reason parameter
    /// indicates why the actor is stopping.
    async fn stopped(&mut self, _reason: &ExitReason, _ctx: &mut ActorContext) {}
}

/// Context provided to actors for interacting with the actor system.
///
/// The context provides access to:
/// - The actor's own Pid
/// - Methods for sending messages
/// - Methods for spawning, linking, and monitoring other actors
/// - Lifecycle control (stop, restart)
pub struct ActorContext {
    pid: Pid,
    mailbox: Mailbox,
    trap_exit: bool,
    should_stop: bool,
    stop_reason: Option<ExitReason>,
    system: Option<std::sync::Weak<crate::ActorSystem>>,
}

impl ActorContext {
    /// Creates a new actor context.
    pub(crate) fn new(pid: Pid, mailbox: Mailbox) -> Self {
        Self {
            pid,
            mailbox,
            trap_exit: false,
            should_stop: false,
            stop_reason: None,
            system: None,
        }
    }

    /// Sets the system reference (called internally during spawn).
    pub(crate) fn set_system(&mut self, system: std::sync::Weak<crate::ActorSystem>) {
        self.system = Some(system);
    }

    /// Returns the Pid of this actor.
    ///
    /// The Pid uniquely identifies this actor in the system and can be
    /// used to send messages or establish links/monitors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct MyActor;
    ///
    /// #[async_trait]
    /// impl Actor for MyActor {
    ///     async fn started(&mut self, ctx: &mut ActorContext) {
    ///         let my_pid = ctx.pid();
    ///         println!("My PID is: {}", my_pid);
    ///     }
    ///     
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    /// }
    /// ```
    pub fn pid(&self) -> Pid {
        self.pid
    }

    /// Returns true if the actor is trapping exit signals.
    ///
    /// When trapping exits, the actor will receive EXIT signals as
    /// regular signals instead of being terminated automatically.
    pub fn is_trapping_exits(&self) -> bool {
        self.trap_exit
    }

    /// Sets whether to trap exit signals.
    ///
    /// When trapping exits, EXIT signals from linked actors are delivered
    /// to `handle_signal` instead of causing the actor to terminate.
    /// This allows actors to handle failures gracefully.
    ///
    /// In Erlang: `process_flag(trap_exit, true)`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message, Signal, ExitReason};
    /// use async_trait::async_trait;
    ///
    /// struct Supervisor;
    ///
    /// #[async_trait]
    /// impl Actor for Supervisor {
    ///     async fn started(&mut self, ctx: &mut ActorContext) {
    ///         // Enable exit trapping to handle child failures
    ///         ctx.trap_exit(true);
    ///     }
    ///
    ///     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    ///
    ///     async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
    ///         if let Signal::Exit { from, reason } = signal {
    ///             println!("Child {} exited: {}", from, reason);
    ///             // Handle the failure instead of dying
    ///         }
    ///     }
    /// }
    /// ```
    pub fn trap_exit(&mut self, trap: bool) {
        self.trap_exit = trap;
    }

    /// Stops the actor with the given reason.
    ///
    /// Calling this method marks the actor for termination. The actor will
    /// finish processing the current message, then its `stopped` hook will
    /// be called before the actor terminates.
    ///
    /// # Arguments
    ///
    /// * `reason` - The reason for stopping, which will be sent to linked actors
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message, ExitReason};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
    ///         if let Some(cmd) = msg.downcast_ref::<&str>() {
    ///             if *cmd == "shutdown" {
    ///                 ctx.stop(ExitReason::Normal);
    ///             } else if *cmd == "panic" {
    ///                 ctx.stop(ExitReason::Panic("Something went wrong".into()));
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn stop(&mut self, reason: ExitReason) {
        self.should_stop = true;
        self.stop_reason = Some(reason);
    }

    /// Returns true if the actor should stop.
    pub(crate) fn should_stop(&self) -> bool {
        self.should_stop
    }

    /// Returns the stop reason if the actor should stop.
    pub(crate) fn stop_reason(&self) -> Option<&ExitReason> {
        self.stop_reason.as_ref()
    }

    /// Receives the next message or signal from the mailbox.
    pub(crate) async fn recv(&mut self) -> Option<Envelope> {
        self.mailbox.recv().await
    }

    /// Closes the actor's mailbox.
    #[allow(dead_code)]
    pub(crate) fn close_mailbox(&mut self) {
        self.mailbox.close();
    }

    /// Sends a message to another actor.
    ///
    /// This is the primary way for actors to communicate. The sender Pid is
    /// automatically tracked from this context.
    ///
    /// # Arguments
    ///
    /// * `to` - The Pid of the target actor
    /// * `msg` - The message to send
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The target actor doesn't exist
    /// - The target actor's mailbox is full
    /// - For remote messages: serialization fails or node is unreachable
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, ActorSystem, Message, Pid};
    /// use async_trait::async_trait;
    ///
    /// struct Worker {
    ///     peer: Option<Pid>,
    /// }
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
    ///         if let Some(peer_pid) = self.peer {
    ///             // Send reply to peer
    ///             ctx.send(peer_pid, Box::new("Reply".to_string())).await.ok();
    ///         }
    ///     }
    /// }
    /// ```
    pub async fn send(&self, to: Pid, msg: Message) -> crate::Result<()> {
        if let Some(system_weak) = &self.system
            && let Some(system) = system_weak.upgrade()
        {
            return system.send(to, msg).await;
        }
        Err(crate::ActorError::SendFailed(to))
    }

    /// Looks up a process by name.
    ///
    /// Returns the Pid if the name is registered, or None otherwise.
    ///
    /// In Erlang: `whereis(Name)`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
    ///         if let Some(manager_pid) = ctx.whereis("manager") {
    ///             ctx.send(manager_pid, Box::new("status")).await.ok();
    ///         }
    ///     }
    /// }
    /// ```
    pub fn whereis(&self, name: &str) -> Option<Pid> {
        if let Some(system_weak) = &self.system
            && let Some(system) = system_weak.upgrade()
        {
            return system.whereis(name);
        }
        None
    }

    /// Schedules a message to be sent after a delay.
    ///
    /// Returns a TimerRef that can be used to cancel the timer.
    ///
    /// In Erlang: `erlang:send_after(Time, Dest, Message)`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message};
    /// use joerl::scheduler::Destination;
    /// use async_trait::async_trait;
    /// use std::time::Duration;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
    ///         // Schedule a reminder to ourselves
    ///         ctx.send_after(
    ///             Destination::Pid(ctx.pid()),
    ///             Box::new("reminder"),
    ///             Duration::from_secs(5)
    ///         );
    ///     }
    /// }
    /// ```
    pub fn send_after(
        &self,
        dest: crate::scheduler::Destination,
        msg: Message,
        duration: std::time::Duration,
    ) -> Option<crate::scheduler::TimerRef> {
        if let Some(system_weak) = &self.system
            && let Some(system) = system_weak.upgrade()
        {
            return Some(system.send_after(dest, msg, duration));
        }
        None
    }

    /// Selectively receive a message matching the predicate.
    ///
    /// This is similar to Erlang's `receive` with pattern matching.
    /// Messages that don't match remain in the mailbox and will be
    /// checked again on subsequent receive calls or regular message processing.
    ///
    /// This allows actors to wait for specific messages while leaving
    /// other messages in the mailbox for later processing.
    ///
    /// In Erlang: `receive Pattern -> Body end`
    ///
    /// # Arguments
    ///
    /// * `predicate` - Function that returns `Some(T)` if message matches
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// #[derive(Clone)]
    /// struct Response {
    ///     id: u64,
    ///     data: String,
    /// }
    ///
    /// struct RpcActor {
    ///     next_id: u64,
    /// }
    ///
    /// #[async_trait]
    /// impl Actor for RpcActor {
    ///     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
    ///         // Make a request with an ID
    ///         let req_id = self.next_id;
    ///         self.next_id += 1;
    ///         
    ///         // ... send request somewhere ...
    ///         
    ///         // Wait for specific response
    ///         let response = ctx.receive(|msg| {
    ///             msg.downcast_ref::<Response>()
    ///                 .filter(|r| r.id == req_id)
    ///                 .cloned()
    ///         }).await;
    ///         
    ///         // Meanwhile, other messages stay in mailbox
    ///     }
    /// }
    /// ```
    pub async fn receive<F, T>(&mut self, predicate: F) -> Option<T>
    where
        F: FnMut(&Message) -> Option<T>,
    {
        self.mailbox.recv_matching(predicate, None).await
    }

    /// Selectively receive a message with timeout.
    ///
    /// Like `receive()`, but returns `None` if no matching message
    /// arrives within the timeout duration.
    ///
    /// In Erlang: `receive Pattern -> Body after Timeout -> TimeoutBody end`
    ///
    /// # Arguments
    ///
    /// * `predicate` - Function that returns `Some(T)` if message matches
    /// * `timeout` - Maximum time to wait
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    /// use std::time::Duration;
    ///
    /// #[derive(Clone)]
    /// struct Ack {
    ///     id: u64,
    /// }
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, ctx: &mut ActorContext) {
    ///         // Wait for ack, but timeout after 5 seconds
    ///         let ack = ctx.receive_timeout(
    ///             |msg| msg.downcast_ref::<Ack>().cloned(),
    ///             Duration::from_secs(5)
    ///         ).await;
    ///         
    ///         match ack {
    ///             Some(ack) => println!("Got ack: {}", ack.id),
    ///             None => println!("Timeout waiting for ack"),
    ///         }
    ///     }
    /// }
    /// ```
    pub async fn receive_timeout<F, T>(
        &mut self,
        predicate: F,
        timeout: std::time::Duration,
    ) -> Option<T>
    where
        F: FnMut(&Message) -> Option<T>,
    {
        self.mailbox.recv_matching(predicate, Some(timeout)).await
    }

    /// Try to receive a matching message without blocking.
    ///
    /// Returns immediately with `Some(T)` if a matching message is found,
    /// or `None` if no match is available.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::{Actor, ActorContext, Message};
    /// use async_trait::async_trait;
    ///
    /// struct Worker;
    ///
    /// #[async_trait]
    /// impl Actor for Worker {
    ///     async fn handle_message(&mut self, _msg: Message, ctx: &mut ActorContext) {
    ///         // Check if there's a ready message without waiting
    ///         if let Some(ready) = ctx.try_receive(|msg| {
    ///             msg.downcast_ref::<String>()
    ///                 .filter(|s| s.as_str() == "ready")
    ///                 .cloned()
    ///         }) {
    ///             println!("Got ready signal: {}", ready);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn try_receive<F, T>(&mut self, predicate: F) -> Option<T>
    where
        F: FnMut(&Message) -> Option<T>,
    {
        self.mailbox.try_recv_matching(predicate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mailbox::{DEFAULT_MAILBOX_CAPACITY, Mailbox};

    struct TestActor {
        messages_received: usize,
    }

    #[async_trait]
    impl Actor for TestActor {
        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {
            self.messages_received += 1;
        }
    }

    #[tokio::test]
    async fn test_actor_context_pid() {
        let pid = Pid::new();
        let (mailbox, _sender) = Mailbox::new(DEFAULT_MAILBOX_CAPACITY);
        let ctx = ActorContext::new(pid, mailbox);
        assert_eq!(ctx.pid(), pid);
    }

    #[tokio::test]
    async fn test_actor_context_trap_exit() {
        let pid = Pid::new();
        let (mailbox, _sender) = Mailbox::new(DEFAULT_MAILBOX_CAPACITY);
        let mut ctx = ActorContext::new(pid, mailbox);

        assert!(!ctx.is_trapping_exits());
        ctx.trap_exit(true);
        assert!(ctx.is_trapping_exits());
    }

    #[tokio::test]
    async fn test_actor_context_stop() {
        let pid = Pid::new();
        let (mailbox, _sender) = Mailbox::new(DEFAULT_MAILBOX_CAPACITY);
        let mut ctx = ActorContext::new(pid, mailbox);

        assert!(!ctx.should_stop());
        ctx.stop(ExitReason::Normal);
        assert!(ctx.should_stop());
        assert_eq!(ctx.stop_reason(), Some(&ExitReason::Normal));
    }

    #[tokio::test]
    async fn test_actor_handle_exit_signal() {
        let pid = Pid::new();
        let (mailbox, _sender) = Mailbox::new(DEFAULT_MAILBOX_CAPACITY);
        let mut ctx = ActorContext::new(pid, mailbox);
        let mut actor = TestActor {
            messages_received: 0,
        };

        // Non-trapped exit should stop actor
        let signal = Signal::Exit {
            from: Pid::new(),
            reason: ExitReason::Panic("error".to_string()),
        };
        actor.handle_signal(signal, &mut ctx).await;
        assert!(ctx.should_stop());
    }

    #[tokio::test]
    async fn test_actor_trap_exit_signal() {
        let pid = Pid::new();
        let (mailbox, _sender) = Mailbox::new(DEFAULT_MAILBOX_CAPACITY);
        let mut ctx = ActorContext::new(pid, mailbox);
        let mut actor = TestActor {
            messages_received: 0,
        };

        ctx.trap_exit(true);

        // Trapped exit should NOT stop actor
        let signal = Signal::Exit {
            from: Pid::new(),
            reason: ExitReason::Panic("error".to_string()),
        };
        actor.handle_signal(signal, &mut ctx).await;
        assert!(!ctx.should_stop());
    }

    #[tokio::test]
    async fn test_actor_stop_signal() {
        let pid = Pid::new();
        let (mailbox, _sender) = Mailbox::new(DEFAULT_MAILBOX_CAPACITY);
        let mut ctx = ActorContext::new(pid, mailbox);
        let mut actor = TestActor {
            messages_received: 0,
        };

        actor.handle_signal(Signal::Stop, &mut ctx).await;
        assert!(ctx.should_stop());
        assert_eq!(ctx.stop_reason(), Some(&ExitReason::Normal));
    }

    #[tokio::test]
    async fn test_actor_kill_signal() {
        let pid = Pid::new();
        let (mailbox, _sender) = Mailbox::new(DEFAULT_MAILBOX_CAPACITY);
        let mut ctx = ActorContext::new(pid, mailbox);
        let mut actor = TestActor {
            messages_received: 0,
        };

        actor.handle_signal(Signal::Kill, &mut ctx).await;
        assert!(ctx.should_stop());
        assert_eq!(ctx.stop_reason(), Some(&ExitReason::Killed));
    }
}
