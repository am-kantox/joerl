//! Error types for the joerl actor system.

use crate::Pid;
use thiserror::Error;

/// Result type for actor operations.
pub type Result<T> = std::result::Result<T, ActorError>;

/// Errors that can occur in the actor system.
#[derive(Error, Debug, Clone)]
pub enum ActorError {
    /// Actor with the given Pid does not exist or has terminated.
    #[error("Actor {0} not found")]
    ActorNotFound(Pid),

    /// Failed to send a message (mailbox full or actor dead).
    #[error("Failed to send message to {0}")]
    SendFailed(Pid),

    /// Mailbox is full and cannot accept more messages.
    #[error("Mailbox full for actor {0}")]
    MailboxFull(Pid),

    /// Actor has already been stopped.
    #[error("Actor {0} already stopped")]
    ActorStopped(Pid),

    /// Actor initialization failed.
    #[error("Actor initialization failed: {0}")]
    InitFailed(String),

    /// Actor panicked during execution.
    #[error("Actor {0} panicked: {1}")]
    ActorPanicked(Pid, String),

    /// Supervisor strategy error.
    #[error("Supervisor error: {0}")]
    SupervisorError(String),

    /// Link or monitor operation failed.
    #[error("Link/monitor operation failed: {0}")]
    LinkError(String),

    /// Timeout waiting for response.
    #[error("Operation timed out")]
    Timeout,

    /// Generic actor error.
    #[error("Actor error: {0}")]
    Other(String),
}

impl ActorError {
    /// Creates a new error from a string message.
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pid;

    #[test]
    fn test_error_display() {
        let pid = Pid::new();
        let err = ActorError::ActorNotFound(pid);
        assert!(format!("{}", err).contains("not found"));
    }

    #[test]
    fn test_error_clone() {
        let err1 = ActorError::Timeout;
        let err2 = err1.clone();
        assert!(matches!(err2, ActorError::Timeout));
    }

    #[test]
    fn test_other_error() {
        let err = ActorError::other("custom error");
        assert!(matches!(err, ActorError::Other(_)));
    }
}
