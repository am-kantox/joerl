//! Named process registry for Erlang-style process registration.
//!
//! This module provides a thread-safe registry for associating names with Pids,
//! enabling location-transparent addressing similar to Erlang's `register/2`,
//! `whereis/1`, and `unregister/1`.
//!
//! # Examples
//!
//! ```rust
//! use joerl::{ActorSystem, Actor, ActorContext, Message};
//! use async_trait::async_trait;
//!
//! struct Worker;
//!
//! #[async_trait]
//! impl Actor for Worker {
//!     async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
//! }
//!
//! # tokio_test::block_on(async {
//! let system = ActorSystem::new();
//! let worker = system.spawn(Worker);
//!
//! // Register the worker with a name
//! system.register("worker1", worker.pid()).unwrap();
//!
//! // Look up the pid by name
//! let pid = system.whereis("worker1").unwrap();
//! assert_eq!(pid, worker.pid());
//!
//! // Unregister the name
//! system.unregister("worker1").unwrap();
//! # });
//! ```

use crate::Pid;
use crate::error::{ActorError, Result};
use dashmap::DashMap;
use std::sync::Arc;

#[cfg(feature = "telemetry")]
use crate::telemetry::RegistryMetrics;

/// A thread-safe registry for mapping names to process IDs.
///
/// The Registry maintains bidirectional mappings between names and Pids,
/// allowing both forward lookup (name → Pid) and reverse lookup (Pid → name).
/// When a process terminates, its name registration is automatically cleaned up.
#[derive(Default)]
pub struct Registry {
    /// Forward mapping: name → Pid
    names: Arc<DashMap<String, Pid>>,
    /// Reverse mapping: Pid → name
    pids: Arc<DashMap<Pid, String>>,
}

impl Registry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            names: Arc::new(DashMap::new()),
            pids: Arc::new(DashMap::new()),
        }
    }

    /// Registers a process with the given name.
    ///
    /// If the name is already registered to another process, returns
    /// `ActorError::NameAlreadyRegistered`. If the Pid is already registered
    /// under a different name, the old name is automatically unregistered first.
    ///
    /// In Erlang: `register(Name, Pid)`
    ///
    /// # Arguments
    ///
    /// * `name` - The name to register (must be non-empty)
    /// * `pid` - The process ID to associate with the name
    ///
    /// # Errors
    ///
    /// Returns `ActorError::NameAlreadyRegistered` if the name is already taken.
    /// Returns `ActorError::InvalidName` if the name is empty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::registry::Registry;
    /// use joerl::Pid;
    ///
    /// let registry = Registry::new();
    /// let pid = Pid::new();
    ///
    /// registry.register("my_process", pid).unwrap();
    ///
    /// // Trying to register the same name again fails
    /// let pid2 = Pid::new();
    /// assert!(registry.register("my_process", pid2).is_err());
    /// ```
    pub fn register(&self, name: impl Into<String>, pid: Pid) -> Result<()> {
        let name = name.into();

        // Validate name
        if name.is_empty() {
            return Err(ActorError::InvalidName("Name cannot be empty".to_string()));
        }

        // Check if this Pid is already registered under a different name
        if let Some(old_name) = self.pids.get(&pid)
            && old_name.value() != &name
        {
            // Unregister the old name first
            self.names.remove(old_name.value());
        }

        // Try to insert the name
        match self.names.entry(name.clone()) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                #[cfg(feature = "telemetry")]
                RegistryMetrics::registration_conflict();
                Err(ActorError::NameAlreadyRegistered(name))
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(pid);
                self.pids.insert(pid, name.clone());
                #[cfg(feature = "telemetry")]
                RegistryMetrics::process_registered();
                Ok(())
            }
        }
    }

    /// Unregisters a name from the registry.
    ///
    /// Returns the Pid that was associated with the name.
    ///
    /// In Erlang: `unregister(Name)`
    ///
    /// # Arguments
    ///
    /// * `name` - The name to unregister
    ///
    /// # Errors
    ///
    /// Returns `ActorError::NameNotRegistered` if the name is not registered.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::registry::Registry;
    /// use joerl::Pid;
    ///
    /// let registry = Registry::new();
    /// let pid = Pid::new();
    ///
    /// registry.register("my_process", pid).unwrap();
    /// let unregistered_pid = registry.unregister("my_process").unwrap();
    /// assert_eq!(unregistered_pid, pid);
    ///
    /// // Trying to unregister again fails
    /// assert!(registry.unregister("my_process").is_err());
    /// ```
    pub fn unregister(&self, name: &str) -> Result<Pid> {
        if let Some((_, pid)) = self.names.remove(name) {
            self.pids.remove(&pid);
            #[cfg(feature = "telemetry")]
            RegistryMetrics::process_unregistered();
            Ok(pid)
        } else {
            Err(ActorError::NameNotRegistered(name.to_string()))
        }
    }

    /// Looks up a process ID by name.
    ///
    /// Returns `Some(Pid)` if the name is registered, or `None` if not found.
    ///
    /// In Erlang: `whereis(Name)`
    ///
    /// # Arguments
    ///
    /// * `name` - The name to look up
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::registry::Registry;
    /// use joerl::Pid;
    ///
    /// let registry = Registry::new();
    /// let pid = Pid::new();
    ///
    /// registry.register("my_process", pid).unwrap();
    /// assert_eq!(registry.whereis("my_process"), Some(pid));
    /// assert_eq!(registry.whereis("unknown"), None);
    /// ```
    pub fn whereis(&self, name: &str) -> Option<Pid> {
        #[cfg(feature = "telemetry")]
        RegistryMetrics::lookup_performed();

        self.names.get(name).map(|entry| *entry.value())
    }

    /// Returns a list of all registered names.
    ///
    /// In Erlang: `registered()`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::registry::Registry;
    /// use joerl::Pid;
    ///
    /// let registry = Registry::new();
    /// registry.register("process1", Pid::new()).unwrap();
    /// registry.register("process2", Pid::new()).unwrap();
    ///
    /// let names = registry.registered();
    /// assert_eq!(names.len(), 2);
    /// assert!(names.contains(&"process1".to_string()));
    /// assert!(names.contains(&"process2".to_string()));
    /// ```
    pub fn registered(&self) -> Vec<String> {
        self.names.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Looks up the registered name for a process ID.
    ///
    /// Returns `Some(String)` if the Pid is registered, or `None` if not found.
    /// This is the reverse operation of `whereis`.
    ///
    /// # Arguments
    ///
    /// * `pid` - The process ID to look up
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::registry::Registry;
    /// use joerl::Pid;
    ///
    /// let registry = Registry::new();
    /// let pid = Pid::new();
    ///
    /// registry.register("my_process", pid).unwrap();
    /// assert_eq!(registry.name_of(pid), Some("my_process".to_string()));
    /// ```
    pub fn name_of(&self, pid: Pid) -> Option<String> {
        self.pids.get(&pid).map(|entry| entry.value().clone())
    }

    /// Returns the number of registered names.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::registry::Registry;
    /// use joerl::Pid;
    ///
    /// let registry = Registry::new();
    /// assert_eq!(registry.len(), 0);
    ///
    /// registry.register("process1", Pid::new()).unwrap();
    /// assert_eq!(registry.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Returns true if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Cleans up any name registrations for the given Pid.
    ///
    /// This is called internally when an actor terminates to ensure
    /// that dead processes don't remain in the registry.
    ///
    /// # Arguments
    ///
    /// * `pid` - The process ID to clean up
    pub(crate) fn cleanup_pid(&self, pid: Pid) {
        if let Some((_, name)) = self.pids.remove(&pid) {
            self.names.remove(&name);
            #[cfg(feature = "telemetry")]
            RegistryMetrics::process_unregistered();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_whereis() {
        let registry = Registry::new();
        let pid = Pid::new();

        registry.register("test", pid).unwrap();
        assert_eq!(registry.whereis("test"), Some(pid));
    }

    #[test]
    fn test_register_duplicate_name() {
        let registry = Registry::new();
        let pid1 = Pid::new();
        let pid2 = Pid::new();

        registry.register("test", pid1).unwrap();
        let result = registry.register("test", pid2);
        assert!(matches!(result, Err(ActorError::NameAlreadyRegistered(_))));
    }

    #[test]
    fn test_register_same_pid_different_name() {
        let registry = Registry::new();
        let pid = Pid::new();

        registry.register("name1", pid).unwrap();
        registry.register("name2", pid).unwrap();

        // The pid should now be registered as "name2"
        assert_eq!(registry.whereis("name1"), None);
        assert_eq!(registry.whereis("name2"), Some(pid));
        assert_eq!(registry.name_of(pid), Some("name2".to_string()));
    }

    #[test]
    fn test_unregister() {
        let registry = Registry::new();
        let pid = Pid::new();

        registry.register("test", pid).unwrap();
        let unregistered_pid = registry.unregister("test").unwrap();
        assert_eq!(unregistered_pid, pid);
        assert_eq!(registry.whereis("test"), None);
    }

    #[test]
    fn test_unregister_not_registered() {
        let registry = Registry::new();
        let result = registry.unregister("nonexistent");
        assert!(matches!(result, Err(ActorError::NameNotRegistered(_))));
    }

    #[test]
    fn test_registered() {
        let registry = Registry::new();
        let pid1 = Pid::new();
        let pid2 = Pid::new();

        registry.register("process1", pid1).unwrap();
        registry.register("process2", pid2).unwrap();

        let names = registry.registered();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"process1".to_string()));
        assert!(names.contains(&"process2".to_string()));
    }

    #[test]
    fn test_name_of() {
        let registry = Registry::new();
        let pid = Pid::new();

        registry.register("my_process", pid).unwrap();
        assert_eq!(registry.name_of(pid), Some("my_process".to_string()));
    }

    #[test]
    fn test_cleanup_pid() {
        let registry = Registry::new();
        let pid = Pid::new();

        registry.register("test", pid).unwrap();
        registry.cleanup_pid(pid);

        assert_eq!(registry.whereis("test"), None);
        assert_eq!(registry.name_of(pid), None);
    }

    #[test]
    fn test_empty_name() {
        let registry = Registry::new();
        let pid = Pid::new();

        let result = registry.register("", pid);
        assert!(matches!(result, Err(ActorError::InvalidName(_))));
    }

    #[test]
    fn test_len_and_is_empty() {
        let registry = Registry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        let pid = Pid::new();
        registry.register("test", pid).unwrap();
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);

        registry.unregister("test").unwrap();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }
}
