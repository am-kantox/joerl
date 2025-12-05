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
    pub fn is_normal(&self) -> bool {
        matches!(self, ExitReason::Normal)
    }

    /// Returns true if this exit reason should be trapped.
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
