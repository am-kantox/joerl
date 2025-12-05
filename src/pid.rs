//! Process identifier (Pid) for actors.
//!
//! In Erlang terminology, a Pid uniquely identifies a process (actor).
//! This implementation uses a combination of node ID and sequence number
//! to ensure uniqueness even in distributed systems.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static PID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Process identifier that uniquely identifies an actor.
///
/// Pids are lightweight, copyable handles that can be sent between actors.
/// They follow Erlang naming conventions where a process is identified by
/// a unique Pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pid {
    /// Node identifier for distributed systems (0 for local)
    pub(crate) node: u32,
    /// Unique sequence number within the node
    pub(crate) id: u64,
}

impl Pid {
    /// Creates a new unique Pid for the local node.
    ///
    /// # Examples
    ///
    /// ```
    /// use joerl::Pid;
    ///
    /// let pid1 = Pid::new();
    /// let pid2 = Pid::new();
    /// assert_ne!(pid1, pid2);
    /// ```
    pub fn new() -> Self {
        Self {
            node: 0,
            id: PID_COUNTER.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Creates a Pid with a specific node and id (primarily for testing and remote actors).
    #[cfg(test)]
    pub(crate) fn with_node(node: u32, id: u64) -> Self {
        Self { node, id }
    }

    /// Returns true if this Pid represents a local actor.
    pub fn is_local(&self) -> bool {
        self.node == 0
    }

    /// Returns the node identifier.
    pub fn node(&self) -> u32 {
        self.node
    }

    /// Returns the process identifier.
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Default for Pid {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.node == 0 {
            write!(f, "<0.{}.0>", self.id)
        } else {
            write!(f, "<{}.{}.0>", self.node, self.id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_unique() {
        let pid1 = Pid::new();
        let pid2 = Pid::new();
        assert_ne!(pid1, pid2);
    }

    #[test]
    fn test_pid_is_local() {
        let pid = Pid::new();
        assert!(pid.is_local());

        let remote_pid = Pid::with_node(1, 42);
        assert!(!remote_pid.is_local());
    }

    #[test]
    fn test_pid_display() {
        let pid = Pid::with_node(0, 123);
        assert_eq!(format!("{}", pid), "<0.123.0>");

        let remote_pid = Pid::with_node(2, 456);
        assert_eq!(format!("{}", remote_pid), "<2.456.0>");
    }

    #[test]
    fn test_pid_clone_copy() {
        let pid1 = Pid::new();
        let pid2 = pid1;
        assert_eq!(pid1, pid2);
    }

    #[test]
    fn test_pid_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let pid = Pid::new();
        set.insert(pid);
        assert!(set.contains(&pid));
    }
}
