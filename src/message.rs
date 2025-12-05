//! Message types for inter-actor communication.
//!
//! Messages in joerl follow Erlang conventions:
//! - User messages (arbitrary data)
//! - System signals (EXIT, DOWN, etc.)

use crate::Pid;
use std::any::Any;
use std::fmt;

/// A message that can be sent between actors.
///
/// In Erlang, any term can be sent as a message. We use `Box<dyn Any>` to achieve
/// similar flexibility, though with less type safety at compile time.
pub type Message = Box<dyn Any + Send + 'static>;

/// System signals sent between actors for lifecycle management.
///
/// Signals are internal messages used by the actor system to manage
/// actor lifecycle events. Unlike user messages, signals have special
/// handling semantics.
///
/// # Examples
///
/// ```rust
/// use joerl::{Signal, ExitReason, Pid, MonitorRef};
///
/// let pid = Pid::new();
/// let exit_signal = Signal::Exit {
///     from: pid,
///     reason: ExitReason::Normal,
/// };
///
/// // Signals are cloneable
/// let cloned = exit_signal.clone();
/// ```
#[derive(Debug, Clone)]
pub enum Signal {
    /// EXIT signal sent when a linked actor terminates.
    /// In Erlang: {'EXIT', FromPid, Reason}
    Exit { from: Pid, reason: ExitReason },

    /// DOWN signal sent to monitoring actors when monitored actor terminates.
    /// In Erlang: {'DOWN', MonitorRef, process, Pid, Reason}
    Down {
        reference: MonitorRef,
        pid: Pid,
        reason: ExitReason,
    },

    /// Request to terminate the actor gracefully.
    Stop,

    /// Request to terminate a linked actor.
    Kill,
}

/// Reason for actor termination.
///
/// Exit reasons indicate why an actor stopped. They are used to determine
/// whether a supervisor should restart a failed actor.
///
/// # Examples
///
/// ```rust
/// use joerl::ExitReason;
///
/// let normal = ExitReason::Normal;
/// assert!(normal.is_normal());
/// assert!(normal.is_trappable());
///
/// let killed = ExitReason::Killed;
/// assert!(!killed.is_normal());
/// assert!(!killed.is_trappable()); // Cannot be trapped
///
/// let panic = ExitReason::Panic("something went wrong".to_string());
/// assert!(!panic.is_normal());
/// assert_eq!(panic.to_string(), "panic: something went wrong");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    /// Normal termination.
    Normal,

    /// Actor was killed explicitly.
    Killed,

    /// Actor panicked with an error message.
    Panic(String),

    /// Actor terminated due to supervisor shutdown.
    Shutdown,

    /// Custom exit reason.
    Custom(String),
}

impl ExitReason {
    /// Returns true if this is a normal exit.
    ///
    /// Normal exits typically don't trigger restarts in supervisors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::ExitReason;
    ///
    /// assert!(ExitReason::Normal.is_normal());
    /// assert!(!ExitReason::Killed.is_normal());
    /// assert!(!ExitReason::Panic("error".into()).is_normal());
    /// ```
    pub fn is_normal(&self) -> bool {
        matches!(self, ExitReason::Normal)
    }

    /// Returns true if this exit reason can be trapped.
    ///
    /// Actors with `trap_exit(true)` can handle trappable exit signals
    /// without terminating. The `Killed` reason cannot be trapped.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::ExitReason;
    ///
    /// assert!(ExitReason::Normal.is_trappable());
    /// assert!(ExitReason::Panic("error".into()).is_trappable());
    /// assert!(!ExitReason::Killed.is_trappable()); // Cannot be trapped
    /// ```
    pub fn is_trappable(&self) -> bool {
        !matches!(self, ExitReason::Killed)
    }
}

impl fmt::Display for ExitReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExitReason::Normal => write!(f, "normal"),
            ExitReason::Killed => write!(f, "killed"),
            ExitReason::Panic(msg) => write!(f, "panic: {}", msg),
            ExitReason::Shutdown => write!(f, "shutdown"),
            ExitReason::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

/// Reference to a monitor relationship.
///
/// Returned when calling `monitor/2`, used to identify DOWN messages.
/// Each monitor reference is unique and can be used to match DOWN signals
/// with the corresponding monitor request.
///
/// In Erlang: reference()
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
/// // MonitorRef is returned when setting up a monitor
/// let monitor_ref = worker.monitor(observer.pid()).expect("Failed to monitor");
///
/// // MonitorRefs are Copy, PartialEq, and can be displayed
/// let monitor_ref_copy = monitor_ref;
/// assert_eq!(monitor_ref, monitor_ref_copy);
/// println!("Monitor ref: {}", monitor_ref);
/// # });
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MonitorRef(pub(crate) u64);

impl MonitorRef {
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Display for MonitorRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#Ref<{}>", self.0)
    }
}

/// Internal message envelope that wraps both user messages and system signals.
#[derive(Debug)]
pub(crate) enum Envelope {
    /// User message.
    Message(Message),
    /// System signal.
    Signal(Signal),
}

impl Envelope {
    pub(crate) fn message(msg: Message) -> Self {
        Envelope::Message(msg)
    }

    pub(crate) fn signal(signal: Signal) -> Self {
        Envelope::Signal(signal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_reason_normal() {
        let reason = ExitReason::Normal;
        assert!(reason.is_normal());
        assert!(reason.is_trappable());
    }

    #[test]
    fn test_exit_reason_killed() {
        let reason = ExitReason::Killed;
        assert!(!reason.is_normal());
        assert!(!reason.is_trappable());
    }

    #[test]
    fn test_exit_reason_display() {
        assert_eq!(ExitReason::Normal.to_string(), "normal");
        assert_eq!(ExitReason::Killed.to_string(), "killed");
        assert_eq!(
            ExitReason::Panic("error".to_string()).to_string(),
            "panic: error"
        );
    }

    #[test]
    fn test_monitor_ref() {
        let ref1 = MonitorRef::new(1);
        let ref2 = MonitorRef::new(2);
        assert_ne!(ref1, ref2);
        assert_eq!(ref1, MonitorRef::new(1));
    }

    #[test]
    fn test_signal_clone() {
        let signal = Signal::Stop;
        let cloned = signal.clone();
        assert!(matches!(cloned, Signal::Stop));
    }

    #[test]
    fn test_message_boxing() {
        let msg: Message = Box::new(42i32);
        let value = msg.downcast_ref::<i32>();
        assert_eq!(value, Some(&42));
    }
}
