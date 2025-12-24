//! Error types for the joerl actor system.

use crate::Pid;
use thiserror::Error;

/// Result type for actor operations.
///
/// This is a type alias for `std::result::Result<T, ActorError>` that makes
/// error handling more ergonomic throughout the library.
///
/// # Examples
///
/// ```rust
/// use joerl::{ActorError, Result, Pid};
///
/// fn check_actor_alive(pid: Pid, alive: bool) -> Result<()> {
///     if alive {
///         Ok(())
///     } else {
///         Err(ActorError::ActorNotFound(pid))
///     }
/// }
/// ```
pub type Result<T> = std::result::Result<T, ActorError>;

/// Errors that can occur in the actor system.
///
/// This enum covers all possible errors that can occur during actor operations.
/// All variants implement `Display` and `Error` through the `thiserror` crate.
///
/// # Examples
///
/// ```rust
/// use joerl::{ActorError, Pid};
///
/// let pid = Pid::new();
/// let error = ActorError::ActorNotFound(pid);
/// assert_eq!(error.to_string(), format!("Actor {} not found", pid));
///
/// let timeout = ActorError::Timeout;
/// assert_eq!(timeout.to_string(), "Operation timed out");
/// ```
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

    /// Name is already registered to another process.
    #[error("Name '{0}' is already registered")]
    NameAlreadyRegistered(String),

    /// Name is not registered.
    #[error("Name '{0}' is not registered")]
    NameNotRegistered(String),

    /// Invalid process name.
    #[error("Invalid name: {0}")]
    InvalidName(String),

    /// Timer not found or already cancelled.
    #[error("Timer {0} not found")]
    TimerNotFound(u64),

    /// Generic actor error.
    #[error("Actor error: {0}")]
    Other(String),
}

impl ActorError {
    /// Creates a new generic error from a string message.
    ///
    /// This is a convenience method for creating ad-hoc errors when
    /// none of the predefined variants fit.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::ActorError;
    ///
    /// let error = ActorError::other("custom error message");
    /// assert_eq!(error.to_string(), "Actor error: custom error message");
    ///
    /// // Works with String or &str
    /// let error2 = ActorError::other(String::from("another error"));
    /// ```
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
