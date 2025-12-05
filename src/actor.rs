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
        }
    }

    /// Returns the Pid of this actor.
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
    /// In Erlang: `process_flag(trap_exit, true)`
    pub fn trap_exit(&mut self, trap: bool) {
        self.trap_exit = trap;
    }

    /// Stops the actor with the given reason.
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
