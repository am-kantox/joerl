//! Message scheduler for delayed and recurring message delivery.
//!
//! This module provides Erlang-style timer functionality for scheduling messages
//! to be delivered after a delay, similar to `erlang:send_after/3`.
//!
//! # Examples
//!
//! ```rust
//! use joerl::{ActorSystem, Actor, ActorContext, Message};
//! use async_trait::async_trait;
//! use std::time::Duration;
//!
//! struct Worker;
//!
//! #[async_trait]
//! impl Actor for Worker {
//!     async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
//!         if let Some(text) = msg.downcast_ref::<String>() {
//!             println!("Received: {}", text);
//!         }
//!     }
//! }
//!
//! # tokio_test::block_on(async {
//! use joerl::scheduler::Destination;
//!
//! let system = ActorSystem::new();
//! let worker = system.spawn(Worker);
//!
//! // Schedule a message to be sent after 1 second
//! let timer_ref = system.send_after(
//!     Destination::Pid(worker.pid()),
//!     Box::new("Hello from the future!".to_string()),
//!     Duration::from_secs(1)
//! );
//!
//! // Timer can be cancelled if needed
//! // system.cancel_timer(timer_ref).unwrap();
//! # });
//! ```

use crate::error::{ActorError, Result};
use crate::{ActorSystem, Message, Pid};
use dashmap::DashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "telemetry")]
use crate::telemetry::SchedulerMetrics;

/// Global counter for generating unique timer references.
static TIMER_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Destination for a message.
///
/// Messages can be sent to either a specific Pid or a registered name.
/// The system will resolve named destinations at the time of delivery.
///
/// # Examples
///
/// ```rust
/// use joerl::scheduler::Destination;
/// use joerl::Pid;
///
/// let dest1 = Destination::Pid(Pid::new());
/// let dest2 = Destination::Name("worker".to_string());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Send to a specific process ID.
    Pid(Pid),
    /// Send to a named process (resolved at delivery time).
    Name(String),
}

impl From<Pid> for Destination {
    fn from(pid: Pid) -> Self {
        Destination::Pid(pid)
    }
}

impl From<String> for Destination {
    fn from(name: String) -> Self {
        Destination::Name(name)
    }
}

impl From<&str> for Destination {
    fn from(name: &str) -> Self {
        Destination::Name(name.to_string())
    }
}

impl fmt::Display for Destination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Destination::Pid(pid) => write!(f, "{}", pid),
            Destination::Name(name) => write!(f, "{}", name),
        }
    }
}

/// Reference to a scheduled timer.
///
/// Can be used to cancel the timer before it fires.
///
/// In Erlang: `reference()` (timer reference)
///
/// # Examples
///
/// ```rust
/// use joerl::scheduler::TimerRef;
///
/// // TimerRef is created internally by the scheduler
/// // and returned from send_after calls.
/// // Users don't create TimerRef directly.
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerRef(u64);

impl TimerRef {
    /// Creates a new timer reference with the given ID.
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the ID of this timer reference.
    pub fn id(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for TimerRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#TimerRef<{}>", self.0)
    }
}

/// Internal scheduler state for managing active timers.
pub(crate) struct Scheduler {
    /// Map of active timers to their cancellation tokens.
    timers: Arc<DashMap<TimerRef, CancellationToken>>,
}

impl Scheduler {
    /// Creates a new scheduler.
    pub(crate) fn new() -> Self {
        Self {
            timers: Arc::new(DashMap::new()),
        }
    }

    /// Schedules a message to be sent after a delay.
    ///
    /// The message will be delivered to the destination after the specified duration.
    /// Returns a TimerRef that can be used to cancel the timer.
    ///
    /// In Erlang: `erlang:send_after(Time, Dest, Message)`
    ///
    /// # Arguments
    ///
    /// * `system` - The actor system to use for sending
    /// * `dest` - The destination (Pid or name) to send to
    /// * `msg` - The message to send
    /// * `duration` - How long to wait before sending
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use joerl::{ActorSystem, Message};
    /// # use joerl::scheduler::Destination;
    /// # use std::time::Duration;
    /// # use std::sync::Arc;
    /// # fn example(system: Arc<ActorSystem>) {
    /// use joerl::Pid;
    ///
    /// let timer_ref = system.send_after(
    ///     Destination::Pid(Pid::new()),
    ///     Box::new("delayed message"),
    ///     Duration::from_millis(500)
    /// );
    /// # }
    /// ```
    pub(crate) fn send_after(
        &self,
        system: Arc<ActorSystem>,
        dest: Destination,
        msg: Message,
        duration: Duration,
    ) -> TimerRef {
        let timer_ref = TimerRef::new(TIMER_COUNTER.fetch_add(1, Ordering::Relaxed));
        let cancel_token = CancellationToken::new();

        // Store the cancellation token
        self.timers.insert(timer_ref, cancel_token.clone());

        #[cfg(feature = "telemetry")]
        SchedulerMetrics::message_scheduled();

        // Spawn a task to deliver the message after the delay
        let timers = Arc::clone(&self.timers);
        tokio::spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    // Timer was cancelled
                    #[cfg(feature = "telemetry")]
                    SchedulerMetrics::timer_cancelled();
                }
                _ = tokio::time::sleep(duration) => {
                    // Timer expired, send the message
                    let result = match &dest {
                        Destination::Pid(pid) => {
                            system.send(*pid, msg).await
                        }
                        Destination::Name(name) => {
                            match system.whereis(name) {
                                Some(pid) => system.send(pid, msg).await,
                                None => {
                                    tracing::warn!(
                                        "Scheduled message destination '{}' not found",
                                        name
                                    );
                                    Err(ActorError::NameNotRegistered(name.clone()))
                                }
                            }
                        }
                    };

                    if let Err(e) = result {
                        tracing::warn!(
                            "Failed to deliver scheduled message to {}: {}",
                            dest,
                            e
                        );
                    }

                    #[cfg(feature = "telemetry")]
                    SchedulerMetrics::message_delivered();
                }
            }

            // Clean up the timer reference
            timers.remove(&timer_ref);
        });

        timer_ref
    }

    /// Cancels a scheduled timer.
    ///
    /// Returns `Ok(true)` if the timer was found and cancelled,
    /// `Ok(false)` if the timer had already fired or been cancelled,
    /// or an error if the timer reference is invalid.
    ///
    /// In Erlang: `erlang:cancel_timer(TimerRef)`
    ///
    /// # Arguments
    ///
    /// * `timer_ref` - The timer reference to cancel
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use joerl::{ActorSystem, Message};
    /// # use joerl::scheduler::{Destination, TimerRef};
    /// # use std::time::Duration;
    /// # async fn example(system: &ActorSystem, timer_ref: TimerRef) {
    /// let cancelled = system.cancel_timer(timer_ref).unwrap();
    /// if cancelled {
    ///     println!("Timer was cancelled");
    /// } else {
    ///     println!("Timer had already fired");
    /// }
    /// # }
    /// ```
    pub(crate) fn cancel_timer(&self, timer_ref: TimerRef) -> Result<bool> {
        if let Some((_, token)) = self.timers.remove(&timer_ref) {
            token.cancel();
            #[cfg(feature = "telemetry")]
            SchedulerMetrics::timer_cancelled();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Returns the number of active timers.
    #[allow(dead_code)]
    pub(crate) fn active_timer_count(&self) -> usize {
        self.timers.len()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destination_from_pid() {
        let pid = Pid::new();
        let dest: Destination = pid.into();
        assert_eq!(dest, Destination::Pid(pid));
    }

    #[test]
    fn test_destination_from_string() {
        let dest: Destination = "worker".into();
        assert_eq!(dest, Destination::Name("worker".to_string()));
    }

    #[test]
    fn test_destination_display() {
        let pid = Pid::new();
        let dest_pid = Destination::Pid(pid);
        let dest_name = Destination::Name("worker".to_string());

        assert!(format!("{}", dest_pid).contains(&pid.to_string()));
        assert_eq!(format!("{}", dest_name), "worker");
    }

    #[test]
    fn test_timer_ref_equality() {
        let ref1 = TimerRef::new(1);
        let ref2 = TimerRef::new(1);
        let ref3 = TimerRef::new(2);

        assert_eq!(ref1, ref2);
        assert_ne!(ref1, ref3);
    }

    #[test]
    fn test_timer_ref_display() {
        let timer_ref = TimerRef::new(42);
        assert_eq!(format!("{}", timer_ref), "#TimerRef<42>");
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = Scheduler::new();
        assert_eq!(scheduler.active_timer_count(), 0);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_timer() {
        let scheduler = Scheduler::new();
        let fake_ref = TimerRef::new(999);
        let result = scheduler.cancel_timer(fake_ref).unwrap();
        assert!(!result); // Returns false for non-existent timer
    }
}
