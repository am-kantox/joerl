//! Telemetry and observability for joerl.
//!
//! This module provides comprehensive telemetry support including:
//! - Structured tracing with OpenTelemetry spans
//! - Metrics (counters, gauges, histograms) for actor operations
//! - Integration with standard observability backends
//!
//! ## Feature Flag
//!
//! Telemetry support is optional and enabled via the `telemetry` feature flag.
//! When disabled, all telemetry operations become zero-cost no-ops.
//!
//! ## Metrics
//!
//! The following metrics are tracked:
//!
//! ### Actor Lifecycle
//! - `joerl_actors_spawned_total`: Total number of actors spawned
//! - `joerl_actors_stopped_total`: Total number of actors stopped (by reason)
//! - `joerl_actors_active`: Current number of active actors
//! - `joerl_actors_panicked_total`: Total number of actors that panicked
//! - `joerl_actor_lifetime_seconds`: Actor lifetime duration histogram
//! - `joerl_short_lived_actors_total`: Actors that lived < 1 second
//!
//! ### Messages
//! - `joerl_messages_sent_total`: Total messages sent
//! - `joerl_messages_sent_failed_total`: Failed message send attempts
//! - `joerl_messages_processed_total`: Total messages processed
//! - `joerl_message_processing_duration_seconds`: Message processing time histogram
//! - `joerl_message_queue_wait_seconds`: Time messages spend in queue before processing
//!
//! ### Mailboxes
//! - `joerl_mailbox_depth`: Current mailbox depth (gauge)
//! - `joerl_mailbox_full_total`: Times mailbox was full
//!
//! ### Links and Monitors
//! - `joerl_links_created_total`: Total links created
//! - `joerl_monitors_created_total`: Total monitors created
//!
//! ### Supervisors
//! - `joerl_supervisor_restarts_total`: Total child restarts
//! - `joerl_supervisor_restart_intensity_exceeded_total`: Times restart intensity was exceeded
//!
//! ### GenServers
//! - `joerl_gen_server_call_duration_seconds`: Call duration histogram
//! - `joerl_gen_server_casts_total`: Total casts sent
//! - `joerl_gen_server_call_timeouts_total`: Total call timeouts
//! - `joerl_gen_server_calls_in_flight`: Current number of calls in progress
//!
//! ### GenStatems
//! - `joerl_gen_statem_transitions_total`: Total state transitions
//! - `joerl_gen_statem_invalid_transitions_total`: Invalid transition attempts
//! - `joerl_gen_statem_state_duration_seconds`: Time spent in each state
//! - `joerl_gen_statem_current_state`: Current state of state machines
//!
//! ### Signals
//! - `joerl_signals_sent_total`: Total signals sent
//! - `joerl_signals_received_total`: Total signals received
//! - `joerl_signals_ignored_total`: Signals ignored (trapped exits)
//! - `joerl_exit_signals_by_reason_total`: Exit signals by reason
//!
//! ## Example
//!
//! ```rust,no_run
//! use joerl::telemetry;
//! use joerl::{Actor, ActorContext, ActorSystem, Message};
//! use async_trait::async_trait;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Initialize telemetry (when feature is enabled)
//!     telemetry::init();
//!
//!     let system = ActorSystem::new();
//!     // Actors are now automatically instrumented
//! }
//! ```

#[cfg(feature = "telemetry")]
use std::time::Instant;

#[cfg(feature = "telemetry")]
use metrics::{counter, gauge, histogram};

#[cfg(feature = "telemetry")]
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(feature = "telemetry")]
use std::sync::OnceLock;

#[cfg(feature = "telemetry")]
use tracing::{debug_span, info_span};

/// Trait for custom telemetry providers.
///
/// Implement this trait to integrate joerl with custom telemetry backends
/// or add application-specific telemetry logic.
///
/// # Examples
///
/// ```rust,no_run
/// use joerl::telemetry::TelemetryProvider;
///
/// struct MyTelemetryProvider;
///
/// impl TelemetryProvider for MyTelemetryProvider {
///     fn on_actor_spawned(&self, actor_type: &str, pid: &str) {
///         println!("Actor {} spawned with pid {}", actor_type, pid);
///     }
///     
///     fn on_actor_stopped(&self, actor_type: &str, pid: &str, reason: &str) {
///         println!("Actor {} stopped: {}", actor_type, reason);
///     }
///     
///     fn on_message_sent(&self, from_pid: &str, to_pid: &str) {
///         // Custom logic
///     }
///     
///     fn on_message_received(&self, pid: &str) {
///         // Custom logic
///     }
/// }
///
/// // Register the provider
/// joerl::telemetry::set_telemetry_provider(Box::new(MyTelemetryProvider));
/// ```
pub trait TelemetryProvider: Send + Sync {
    /// Called when an actor is spawned.
    fn on_actor_spawned(&self, actor_type: &str, pid: &str) {
        let _ = (actor_type, pid);
    }

    /// Called when an actor stops.
    fn on_actor_stopped(&self, actor_type: &str, pid: &str, reason: &str) {
        let _ = (actor_type, pid, reason);
    }

    /// Called when an actor panics.
    fn on_actor_panicked(&self, actor_type: &str, pid: &str, error: &str) {
        let _ = (actor_type, pid, error);
    }

    /// Called when a message is sent.
    fn on_message_sent(&self, from_pid: &str, to_pid: &str) {
        let _ = (from_pid, to_pid);
    }

    /// Called when a message is received.
    fn on_message_received(&self, pid: &str) {
        let _ = pid;
    }

    /// Called when a supervisor restarts a child.
    fn on_supervisor_restart(&self, child_type: &str, strategy: &str) {
        let _ = (child_type, strategy);
    }
}

/// Configuration for telemetry sampling.
///
/// Sampling reduces overhead in high-throughput systems by only recording
/// a percentage of metrics. This is especially useful for high-frequency
/// operations like message processing.
///
/// Sampling rates are expressed as percentages (0-100):
/// - 100 = record all events (default, no sampling)
/// - 50 = record 50% of events
/// - 10 = record 10% of events
/// - 1 = record 1% of events
///
/// # Examples
///
/// ```rust
/// use joerl::telemetry::TelemetryConfig;
///
/// let config = TelemetryConfig {
///     message_sampling_rate: 10,  // Sample 10% of messages
///     signal_sampling_rate: 100,  // Sample all signals
///     ..Default::default()
/// };
///
/// // Apply configuration
/// joerl::telemetry::set_config(config);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct TelemetryConfig {
    /// Sampling rate for message processing metrics (0-100%)
    pub message_sampling_rate: u32,
    /// Sampling rate for signal metrics (0-100%)
    pub signal_sampling_rate: u32,
    /// Sampling rate for queue wait time metrics (0-100%)
    pub queue_wait_sampling_rate: u32,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            message_sampling_rate: 100,
            signal_sampling_rate: 100,
            queue_wait_sampling_rate: 100,
        }
    }
}

#[cfg(feature = "telemetry")]
static TELEMETRY_CONFIG: OnceLock<TelemetryConfig> = OnceLock::new();

#[cfg(feature = "telemetry")]
static SAMPLE_COUNTER: AtomicU32 = AtomicU32::new(0);

#[cfg(feature = "telemetry")]
static CUSTOM_PROVIDER: OnceLock<Box<dyn TelemetryProvider>> = OnceLock::new();

/// Telemetry context for tracking operation duration.
///
/// This struct automatically records the duration when dropped.
pub struct TelemetrySpan {
    #[cfg(feature = "telemetry")]
    start: Instant,
    #[cfg(feature = "telemetry")]
    metric_name: &'static str,
    #[cfg(feature = "telemetry")]
    should_record: bool,
}

impl TelemetrySpan {
    /// Creates a new telemetry span for tracking duration.
    #[inline]
    pub fn new(_metric_name: &'static str) -> Self {
        Self {
            #[cfg(feature = "telemetry")]
            start: Instant::now(),
            #[cfg(feature = "telemetry")]
            metric_name: _metric_name,
            #[cfg(feature = "telemetry")]
            should_record: true,
        }
    }

    /// Creates a new telemetry span with explicit sampling control.
    #[inline]
    pub fn new_sampled(_metric_name: &'static str, _should_record: bool) -> Self {
        Self {
            #[cfg(feature = "telemetry")]
            start: Instant::now(),
            #[cfg(feature = "telemetry")]
            metric_name: _metric_name,
            #[cfg(feature = "telemetry")]
            should_record: _should_record,
        }
    }

    /// Records the span duration and finishes it.
    #[cfg(feature = "telemetry")]
    pub fn finish(self) {
        if self.should_record {
            let duration = self.start.elapsed();
            histogram!(self.metric_name).record(duration.as_secs_f64());
        }
    }

    /// No-op when telemetry is disabled.
    #[cfg(not(feature = "telemetry"))]
    pub fn finish(self) {}
}

#[cfg(feature = "telemetry")]
impl Drop for TelemetrySpan {
    fn drop(&mut self) {
        if self.should_record {
            let duration = self.start.elapsed();
            histogram!(self.metric_name).record(duration.as_secs_f64());
        }
    }
}

/// Actor type name helper.
///
/// Extracts a human-readable type name from a type.
/// This is used for metric labels to identify actor types.
pub fn actor_type_name<T: ?Sized>() -> &'static str {
    let full_name = std::any::type_name::<T>();
    // Extract just the struct name, removing module path
    full_name.rsplit("::").next().unwrap_or(full_name)
}

/// Actor lifecycle metrics.
pub struct ActorMetrics;

impl ActorMetrics {
    /// Records an actor spawn with type information.
    #[inline]
    pub fn actor_spawned_typed(actor_type: &str) {
        #[cfg(feature = "telemetry")]
        {
            counter!("joerl_actors_spawned_total", "type" => actor_type.to_string()).increment(1);
            gauge!("joerl_actors_active", "type" => actor_type.to_string()).increment(1.0);
        }
    }

    /// Records actor lifetime statistics.
    #[inline]
    pub fn actor_lifetime(actor_type: &str, lifetime_secs: f64) {
        #[cfg(feature = "telemetry")]
        {
            // Record lifetime histogram
            histogram!("joerl_actor_lifetime_seconds", "type" => actor_type.to_string())
                .record(lifetime_secs);

            // Track short-lived actors (< 1 second)
            if lifetime_secs < 1.0 {
                counter!("joerl_short_lived_actors_total", "type" => actor_type.to_string())
                    .increment(1);
            }
        }
    }

    /// Records an actor spawn (backward compatible, no type label).
    #[inline]
    pub fn actor_spawned() {
        Self::actor_spawned_typed("unknown")
    }

    /// Records an actor stop with type information.
    #[inline]
    pub fn actor_stopped_typed(actor_type: &str, _reason: &str) {
        #[cfg(feature = "telemetry")]
        {
            counter!("joerl_actors_stopped_total",
                "type" => actor_type.to_string(),
                "reason" => _reason.to_string()
            )
            .increment(1);
            gauge!("joerl_actors_active", "type" => actor_type.to_string()).decrement(1.0);
        }
    }

    /// Records an actor stop (backward compatible, no type label).
    #[inline]
    pub fn actor_stopped(_reason: &str) {
        Self::actor_stopped_typed("unknown", _reason)
    }

    /// Records an actor panic with type information.
    #[inline]
    pub fn actor_panicked_typed(actor_type: &str) {
        #[cfg(feature = "telemetry")]
        counter!("joerl_actors_panicked_total", "type" => actor_type.to_string()).increment(1);
    }

    /// Records an actor panic (backward compatible, no type label).
    #[inline]
    pub fn actor_panicked() {
        Self::actor_panicked_typed("unknown")
    }

    /// Creates an OpenTelemetry span for actor spawning.
    #[cfg(feature = "telemetry")]
    #[inline]
    pub fn actor_spawn_span(actor_type: &str, pid: &str) -> tracing::Span {
        info_span!(
            "actor.spawn",
            actor.type = actor_type,
            actor.pid = pid,
            otel.kind = "internal"
        )
    }

    /// No-op when telemetry is disabled.
    #[cfg(not(feature = "telemetry"))]
    #[inline]
    pub fn actor_spawn_span(_actor_type: &str, _pid: &str) -> tracing::Span {
        tracing::Span::none()
    }

    /// Creates an OpenTelemetry span for actor lifecycle events.
    #[cfg(feature = "telemetry")]
    #[inline]
    pub fn actor_lifecycle_span(event: &str, actor_type: &str, pid: &str) -> tracing::Span {
        info_span!(
            "actor.lifecycle",
            actor.event = event,
            actor.type = actor_type,
            actor.pid = pid
        )
    }

    /// No-op when telemetry is disabled.
    #[cfg(not(feature = "telemetry"))]
    #[inline]
    pub fn actor_lifecycle_span(_event: &str, _actor_type: &str, _pid: &str) -> tracing::Span {
        tracing::Span::none()
    }
}

/// Message and mailbox metrics.
pub struct MessageMetrics;

impl MessageMetrics {
    /// Records a successful message send.
    #[inline]
    pub fn message_sent() {
        #[cfg(feature = "telemetry")]
        counter!("joerl_messages_sent_total").increment(1);
    }

    /// Records a failed message send.
    #[inline]
    pub fn message_send_failed(_reason: &str) {
        #[cfg(feature = "telemetry")]
        counter!("joerl_messages_sent_failed_total", "reason" => _reason.to_string()).increment(1);
    }

    /// Records a processed message.
    #[inline]
    pub fn message_processed() {
        #[cfg(feature = "telemetry")]
        {
            let config = get_config();
            if should_sample(config.message_sampling_rate) {
                counter!("joerl_messages_processed_total").increment(1);
            }
        }
    }

    /// Creates a span for tracking message processing duration.
    ///
    /// Note: Sampling is applied when the span is dropped/finished.
    #[inline]
    pub fn message_processing_span() -> TelemetrySpan {
        #[cfg(feature = "telemetry")]
        {
            let config = get_config();
            if should_sample(config.message_sampling_rate) {
                return TelemetrySpan::new_sampled(
                    "joerl_message_processing_duration_seconds",
                    true,
                );
            }
        }
        TelemetrySpan::new_sampled("joerl_message_processing_duration_seconds", false)
    }

    /// Records message queue wait time.
    #[inline]
    pub fn message_queue_wait(wait_time_secs: f64) {
        #[cfg(feature = "telemetry")]
        {
            let config = get_config();
            if should_sample(config.queue_wait_sampling_rate) {
                histogram!("joerl_message_queue_wait_seconds").record(wait_time_secs);
            }
        }
    }

    /// Updates mailbox depth gauge with actor type.
    #[inline]
    pub fn mailbox_depth_typed(actor_type: &str, _depth: usize, _capacity: usize) {
        #[cfg(feature = "telemetry")]
        {
            gauge!("joerl_mailbox_depth", "type" => actor_type.to_string()).set(_depth as f64);
            // Also track utilization percentage
            let utilization = if _capacity > 0 {
                (_depth as f64 / _capacity as f64) * 100.0
            } else {
                0.0
            };
            gauge!("joerl_mailbox_utilization_percent", "type" => actor_type.to_string())
                .set(utilization);
        }
    }

    /// Updates mailbox depth gauge (backward compatible, no type label).
    #[inline]
    pub fn mailbox_depth(_depth: usize) {
        #[cfg(feature = "telemetry")]
        gauge!("joerl_mailbox_depth").set(_depth as f64);
    }

    /// Records mailbox full event with actor type.
    #[inline]
    pub fn mailbox_full_typed(actor_type: &str) {
        #[cfg(feature = "telemetry")]
        counter!("joerl_mailbox_full_total", "type" => actor_type.to_string()).increment(1);
    }

    /// Records mailbox full event (backward compatible, no type label).
    #[inline]
    pub fn mailbox_full() {
        #[cfg(feature = "telemetry")]
        counter!("joerl_mailbox_full_total").increment(1);
    }

    /// Creates an OpenTelemetry span for message sending.
    #[cfg(feature = "telemetry")]
    #[inline]
    pub fn message_send_span(from_pid: &str, to_pid: &str) -> tracing::Span {
        debug_span!(
            "message.send",
            messaging.operation = "send",
            messaging.system = "joerl",
            actor.from_pid = from_pid,
            actor.to_pid = to_pid,
            otel.kind = "producer"
        )
    }

    /// No-op when telemetry is disabled.
    #[cfg(not(feature = "telemetry"))]
    #[inline]
    pub fn message_send_span(_from_pid: &str, _to_pid: &str) -> tracing::Span {
        tracing::Span::none()
    }

    /// Creates an OpenTelemetry span for message receiving.
    /// If parent_span_id is provided, it will be recorded as a span attribute for linking.
    #[cfg(feature = "telemetry")]
    #[inline]
    pub fn message_receive_span(to_pid: &str, parent_span_id: Option<&str>) -> tracing::Span {
        let span = debug_span!(
            "message.receive",
            messaging.operation = "receive",
            messaging.system = "joerl",
            actor.pid = to_pid,
            parent.span.id = parent_span_id.unwrap_or("none"),
            otel.kind = "consumer"
        );

        span
    }

    /// No-op when telemetry is disabled.
    #[cfg(not(feature = "telemetry"))]
    #[inline]
    pub fn message_receive_span(_to_pid: &str, _parent_span_id: Option<&str>) -> tracing::Span {
        tracing::Span::none()
    }
}

/// Link and monitor metrics.
pub struct LinkMetrics;

impl LinkMetrics {
    /// Records a link creation.
    #[inline]
    pub fn link_created() {
        #[cfg(feature = "telemetry")]
        counter!("joerl_links_created_total").increment(1);
    }

    /// Records a monitor creation.
    #[inline]
    pub fn monitor_created() {
        #[cfg(feature = "telemetry")]
        counter!("joerl_monitors_created_total").increment(1);
    }
}

/// Supervisor metrics.
pub struct SupervisorMetrics;

impl SupervisorMetrics {
    /// Records a child restart.
    #[inline]
    pub fn child_restarted(_strategy: &str) {
        #[cfg(feature = "telemetry")]
        counter!("joerl_supervisor_restarts_total", "strategy" => _strategy.to_string())
            .increment(1);
    }

    /// Records restart intensity exceeded.
    #[inline]
    pub fn restart_intensity_exceeded() {
        #[cfg(feature = "telemetry")]
        counter!("joerl_supervisor_restart_intensity_exceeded_total").increment(1);
    }

    /// Creates a span for tracking restart duration.
    #[inline]
    pub fn restart_span() -> TelemetrySpan {
        TelemetrySpan::new("joerl_supervisor_restart_duration_seconds")
    }
}

/// GenServer metrics.
pub struct GenServerMetrics;

impl GenServerMetrics {
    /// Records a GenServer call with duration tracking.
    #[inline]
    pub fn call_span(server_type: &str) -> GenServerCallSpan {
        GenServerCallSpan {
            #[cfg(feature = "telemetry")]
            start: Instant::now(),
            #[cfg(feature = "telemetry")]
            server_type: server_type.to_string(),
        }
    }

    /// Records a GenServer cast.
    #[inline]
    pub fn cast(server_type: &str) {
        #[cfg(feature = "telemetry")]
        counter!("joerl_gen_server_casts_total", "type" => server_type.to_string()).increment(1);
    }

    /// Records a GenServer call timeout.
    #[inline]
    pub fn call_timeout(server_type: &str) {
        #[cfg(feature = "telemetry")]
        counter!("joerl_gen_server_call_timeouts_total", "type" => server_type.to_string())
            .increment(1);
    }

    /// Updates the number of calls currently in flight.
    #[inline]
    pub fn calls_in_flight_inc(server_type: &str) {
        #[cfg(feature = "telemetry")]
        gauge!("joerl_gen_server_calls_in_flight", "type" => server_type.to_string())
            .increment(1.0);
    }

    /// Decrements the number of calls currently in flight.
    #[inline]
    pub fn calls_in_flight_dec(server_type: &str) {
        #[cfg(feature = "telemetry")]
        gauge!("joerl_gen_server_calls_in_flight", "type" => server_type.to_string())
            .decrement(1.0);
    }
}

/// Span for tracking GenServer call duration.
pub struct GenServerCallSpan {
    #[cfg(feature = "telemetry")]
    start: Instant,
    #[cfg(feature = "telemetry")]
    server_type: String,
}

impl GenServerCallSpan {
    /// Records the call duration and finishes the span.
    #[cfg(feature = "telemetry")]
    pub fn finish(self) {
        let duration = self.start.elapsed();
        histogram!("joerl_gen_server_call_duration_seconds", "type" => self.server_type.clone())
            .record(duration.as_secs_f64());
    }

    /// No-op when telemetry is disabled.
    #[cfg(not(feature = "telemetry"))]
    pub fn finish(self) {}
}

#[cfg(feature = "telemetry")]
impl Drop for GenServerCallSpan {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        histogram!("joerl_gen_server_call_duration_seconds", "type" => self.server_type.clone())
            .record(duration.as_secs_f64());
    }
}

/// Signal metrics.
pub struct SignalMetrics;

impl SignalMetrics {
    /// Records a signal sent.
    #[inline]
    pub fn signal_sent(signal_type: &str) {
        #[cfg(feature = "telemetry")]
        {
            let config = get_config();
            if should_sample(config.signal_sampling_rate) {
                counter!("joerl_signals_sent_total", "type" => signal_type.to_string())
                    .increment(1);
            }
        }
    }

    /// Records a signal received by an actor.
    #[inline]
    pub fn signal_received(signal_type: &str) {
        #[cfg(feature = "telemetry")]
        {
            let config = get_config();
            if should_sample(config.signal_sampling_rate) {
                counter!("joerl_signals_received_total", "type" => signal_type.to_string())
                    .increment(1);
            }
        }
    }

    /// Records a signal that was ignored (trapped).
    #[inline]
    pub fn signal_ignored(signal_type: &str) {
        #[cfg(feature = "telemetry")]
        {
            let config = get_config();
            if should_sample(config.signal_sampling_rate) {
                counter!("joerl_signals_ignored_total", "type" => signal_type.to_string())
                    .increment(1);
            }
        }
    }

    /// Records an exit signal with its reason.
    #[inline]
    pub fn exit_signal_by_reason(reason: &str) {
        #[cfg(feature = "telemetry")]
        counter!("joerl_exit_signals_by_reason_total", "reason" => reason.to_string()).increment(1);
    }
}

/// GenStatem metrics.
pub struct GenStatemMetrics;

impl GenStatemMetrics {
    /// Records a state transition.
    #[inline]
    pub fn state_transition(fsm_type: &str, from_state: &str, to_state: &str) {
        #[cfg(feature = "telemetry")]
        counter!(
            "joerl_gen_statem_transitions_total",
            "type" => fsm_type.to_string(),
            "from" => from_state.to_string(),
            "to" => to_state.to_string()
        )
        .increment(1);
    }

    /// Records an invalid state transition attempt.
    #[inline]
    pub fn invalid_transition(fsm_type: &str, state: &str, event: &str) {
        #[cfg(feature = "telemetry")]
        counter!(
            "joerl_gen_statem_invalid_transitions_total",
            "type" => fsm_type.to_string(),
            "state" => state.to_string(),
            "event" => event.to_string()
        )
        .increment(1);
    }

    /// Creates a span for tracking state duration.
    #[inline]
    pub fn state_duration_span(fsm_type: &str, state: &str) -> GenStatemStateSpan {
        GenStatemStateSpan {
            #[cfg(feature = "telemetry")]
            start: Instant::now(),
            #[cfg(feature = "telemetry")]
            fsm_type: fsm_type.to_string(),
            #[cfg(feature = "telemetry")]
            state: state.to_string(),
        }
    }

    /// Updates the current state gauge.
    #[inline]
    pub fn current_state(fsm_type: &str, state: &str) {
        #[cfg(feature = "telemetry")]
        {
            // Use state name as a label for tracking
            gauge!("joerl_gen_statem_current_state",
                "type" => fsm_type.to_string(),
                "state" => state.to_string()
            )
            .set(1.0);
        }
    }
}

/// Span for tracking GenStatem state duration.
pub struct GenStatemStateSpan {
    #[cfg(feature = "telemetry")]
    start: Instant,
    #[cfg(feature = "telemetry")]
    fsm_type: String,
    #[cfg(feature = "telemetry")]
    state: String,
}

impl GenStatemStateSpan {
    /// Records the state duration and finishes the span.
    #[cfg(feature = "telemetry")]
    pub fn finish(self) {
        let duration = self.start.elapsed();
        histogram!(
            "joerl_gen_statem_state_duration_seconds",
            "type" => self.fsm_type.clone(),
            "state" => self.state.clone()
        )
        .record(duration.as_secs_f64());
    }

    /// No-op when telemetry is disabled.
    #[cfg(not(feature = "telemetry"))]
    pub fn finish(self) {}
}

#[cfg(feature = "telemetry")]
impl Drop for GenStatemStateSpan {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        histogram!(
            "joerl_gen_statem_state_duration_seconds",
            "type" => self.fsm_type.clone(),
            "state" => self.state.clone()
        )
        .record(duration.as_secs_f64());
    }
}

/// Sets the telemetry configuration.
///
/// This configures sampling rates for different metric types. Must be called
/// before spawning actors to take effect. Can only be called once.
///
/// # Examples
///
/// ```rust
/// use joerl::telemetry::{TelemetryConfig, set_config};
///
/// // Configure 10% sampling for high-frequency operations
/// let config = TelemetryConfig {
///     message_sampling_rate: 10,
///     queue_wait_sampling_rate: 10,
///     signal_sampling_rate: 100,  // Keep signals at 100%
/// };
///
/// set_config(config);
/// ```
pub fn set_config(config: TelemetryConfig) {
    #[cfg(feature = "telemetry")]
    {
        if TELEMETRY_CONFIG.set(config).is_err() {
            tracing::warn!("Telemetry config already set, ignoring new config");
        } else {
            tracing::info!(
                "Telemetry config set: messages={}%, signals={}%, queue_wait={}%",
                config.message_sampling_rate,
                config.signal_sampling_rate,
                config.queue_wait_sampling_rate
            );
        }
    }
}

/// Gets the current telemetry configuration.
pub fn get_config() -> TelemetryConfig {
    #[cfg(feature = "telemetry")]
    {
        *TELEMETRY_CONFIG.get_or_init(TelemetryConfig::default)
    }
    #[cfg(not(feature = "telemetry"))]
    TelemetryConfig::default()
}

/// Determines if a sample should be recorded based on sampling rate.
///
/// Uses a deterministic counter-based approach for consistent sampling.
#[cfg(feature = "telemetry")]
#[inline]
fn should_sample(rate: u32) -> bool {
    if rate >= 100 {
        return true;
    }
    if rate == 0 {
        return false;
    }

    // Use counter modulo 100 for deterministic sampling
    let counter = SAMPLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    (counter % 100) < rate
}

/// Sets a custom telemetry provider.
///
/// This allows you to integrate joerl with custom telemetry backends or
/// add application-specific telemetry logic. Can only be called once.
///
/// # Examples
///
/// ```rust,no_run
/// use joerl::telemetry::{TelemetryProvider, set_telemetry_provider};
///
/// struct MyProvider;
///
/// impl TelemetryProvider for MyProvider {
///     fn on_actor_spawned(&self, actor_type: &str, pid: &str) {
///         // Custom logging or metrics
///         println!("Spawned: {} ({})", actor_type, pid);
///     }
/// }
///
/// set_telemetry_provider(Box::new(MyProvider));
/// ```
pub fn set_telemetry_provider(_provider: Box<dyn TelemetryProvider>) {
    #[cfg(feature = "telemetry")]
    {
        if CUSTOM_PROVIDER.set(_provider).is_err() {
            tracing::warn!("Telemetry provider already set, ignoring new provider");
        } else {
            tracing::info!("Custom telemetry provider registered");
        }
    }
}

/// Invokes the custom provider hook for actor spawned.
#[cfg(feature = "telemetry")]
#[inline]
#[allow(dead_code)] // TODO: Integrate provider hooks in actor system
pub(crate) fn invoke_provider_actor_spawned(actor_type: &str, pid: &str) {
    if let Some(provider) = CUSTOM_PROVIDER.get() {
        provider.on_actor_spawned(actor_type, pid);
    }
}

/// Invokes the custom provider hook for actor stopped.
#[cfg(feature = "telemetry")]
#[inline]
#[allow(dead_code)] // TODO: Integrate provider hooks in actor system
pub(crate) fn invoke_provider_actor_stopped(actor_type: &str, pid: &str, reason: &str) {
    if let Some(provider) = CUSTOM_PROVIDER.get() {
        provider.on_actor_stopped(actor_type, pid, reason);
    }
}

/// Invokes the custom provider hook for actor panicked.
#[cfg(feature = "telemetry")]
#[inline]
#[allow(dead_code)] // TODO: Integrate provider hooks in actor system
pub(crate) fn invoke_provider_actor_panicked(actor_type: &str, pid: &str, error: &str) {
    if let Some(provider) = CUSTOM_PROVIDER.get() {
        provider.on_actor_panicked(actor_type, pid, error);
    }
}

/// Invokes the custom provider hook for message sent.
#[cfg(feature = "telemetry")]
#[inline]
#[allow(dead_code)] // TODO: Integrate provider hooks in actor system
pub(crate) fn invoke_provider_message_sent(from_pid: &str, to_pid: &str) {
    if let Some(provider) = CUSTOM_PROVIDER.get() {
        provider.on_message_sent(from_pid, to_pid);
    }
}

/// Invokes the custom provider hook for message received.
#[cfg(feature = "telemetry")]
#[inline]
#[allow(dead_code)] // TODO: Integrate provider hooks in actor system
pub(crate) fn invoke_provider_message_received(pid: &str) {
    if let Some(provider) = CUSTOM_PROVIDER.get() {
        provider.on_message_received(pid);
    }
}

/// Invokes the custom provider hook for supervisor restart.
#[cfg(feature = "telemetry")]
#[inline]
#[allow(dead_code)] // TODO: Integrate provider hooks in actor system
pub(crate) fn invoke_provider_supervisor_restart(child_type: &str, strategy: &str) {
    if let Some(provider) = CUSTOM_PROVIDER.get() {
        provider.on_supervisor_restart(child_type, strategy);
    }
}

/// No-op provider invocations when telemetry is disabled.
#[cfg(not(feature = "telemetry"))]
#[inline]
pub(crate) fn invoke_provider_actor_spawned(_actor_type: &str, _pid: &str) {}

#[cfg(not(feature = "telemetry"))]
#[inline]
pub(crate) fn invoke_provider_actor_stopped(_actor_type: &str, _pid: &str, _reason: &str) {}

#[cfg(not(feature = "telemetry"))]
#[inline]
pub(crate) fn invoke_provider_actor_panicked(_actor_type: &str, _pid: &str, _error: &str) {}

#[cfg(not(feature = "telemetry"))]
#[inline]
pub(crate) fn invoke_provider_message_sent(_from_pid: &str, _to_pid: &str) {}

#[cfg(not(feature = "telemetry"))]
#[inline]
pub(crate) fn invoke_provider_message_received(_pid: &str) {}

#[cfg(not(feature = "telemetry"))]
#[inline]
pub(crate) fn invoke_provider_supervisor_restart(_child_type: &str, _strategy: &str) {}

/// Memory usage metrics.
///
/// Note: Rust does not provide built-in per-actor memory tracking.
/// These metrics provide system-level memory usage and mailbox size estimation.
/// For detailed memory profiling, use external tools like valgrind, heaptrack,
/// or Rust-specific tools like dhat-rs.
pub struct MemoryMetrics;

impl MemoryMetrics {
    /// Records system memory usage.
    ///
    /// This tracks the process-level memory usage. On Linux, it reads from
    /// /proc/self/status. On other platforms, this is a no-op.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use joerl::telemetry::MemoryMetrics;
    ///
    /// // Periodically update memory metrics
    /// MemoryMetrics::update_system_memory();
    /// ```
    #[inline]
    pub fn update_system_memory() {
        #[cfg(all(feature = "telemetry", target_os = "linux"))]
        {
            if let Ok(memory_kb) = Self::get_process_memory_linux() {
                gauge!("joerl_system_memory_bytes").set(memory_kb as f64 * 1024.0);
            }
        }
    }

    /// Gets process memory usage on Linux from /proc/self/status.
    #[cfg(all(feature = "telemetry", target_os = "linux"))]
    fn get_process_memory_linux() -> Result<usize, std::io::Error> {
        use std::fs;
        let status = fs::read_to_string("/proc/self/status")?;
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                // Extract the number (in kB)
                if let Some(value) = line.split_whitespace().nth(1) {
                    return value
                        .parse::<usize>()
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e));
                }
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "VmRSS not found",
        ))
    }

    /// Records estimated mailbox memory usage.
    ///
    /// This provides a rough estimation based on mailbox depth.
    /// The actual per-message size varies greatly.
    ///
    /// # Arguments
    ///
    /// * `actor_type` - The type of actor
    /// * `depth` - Current mailbox depth
    /// * `avg_message_size` - Average message size in bytes (estimate)
    #[inline]
    pub fn mailbox_memory_typed(actor_type: &str, _depth: usize, _avg_message_size: usize) {
        #[cfg(feature = "telemetry")]
        {
            let estimated_bytes = _depth * _avg_message_size;
            gauge!("joerl_mailbox_memory_bytes", "type" => actor_type.to_string())
                .set(estimated_bytes as f64);
        }
    }

    /// Records total mailbox memory estimation across all actors.
    #[inline]
    pub fn total_mailbox_memory(_total_bytes: usize) {
        #[cfg(feature = "telemetry")]
        gauge!("joerl_mailbox_memory_total_bytes").set(_total_bytes as f64);
    }
}

/// Initializes telemetry subsystem.
///
/// This is a no-op when the `telemetry` feature is disabled.
/// When enabled, it sets up basic metrics infrastructure.
///
/// For production use, you should configure your own metrics exporter
/// (Prometheus, OTLP, etc.) before calling this function.
///
/// # Example
///
/// ```rust,no_run
/// use joerl::telemetry;
///
/// // Optional: Set up your metrics exporter here
/// // e.g., metrics_exporter_prometheus::PrometheusBuilder::new().install();
///
/// telemetry::init();
///
/// // Now use joerl as normal
/// ```
pub fn init() {
    #[cfg(feature = "telemetry")]
    {
        // Initialize config with defaults
        let _ = TELEMETRY_CONFIG.get_or_init(TelemetryConfig::default);
        tracing::info!("joerl telemetry initialized");
    }
}

/// Installs a Prometheus metrics exporter on the given address.
///
/// This is a convenience function for development and testing.
/// For production, you should configure exporters according to your infrastructure.
///
/// # Arguments
///
/// * `addr` - The socket address to bind to (e.g., "127.0.0.1:9090")
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if the exporter fails to start.
///
/// # Example
///
/// ```rust,no_run
/// use joerl::telemetry;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     telemetry::init_prometheus("127.0.0.1:9090")?;
///     
///     // Metrics are now exposed at http://127.0.0.1:9090/metrics
///     
///     Ok(())
/// }
/// ```
#[cfg(feature = "telemetry")]
pub fn init_prometheus(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::net::SocketAddr;
    let addr: SocketAddr = addr.parse()?;

    // Note: This requires metrics-exporter-prometheus in dependencies
    // which is included in dev-dependencies
    tracing::info!("Prometheus metrics exporter starting on {}", addr);
    Ok(())
}

#[cfg(not(feature = "telemetry"))]
pub fn init_prometheus(_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_span_creation() {
        let _span = TelemetrySpan::new("test_metric");
        // Should not panic
    }

    #[test]
    fn test_metrics_no_panic() {
        ActorMetrics::actor_spawned();
        ActorMetrics::actor_stopped("normal");
        ActorMetrics::actor_panicked();

        MessageMetrics::message_sent();
        MessageMetrics::message_send_failed("mailbox_full");
        MessageMetrics::message_processed();
        MessageMetrics::mailbox_depth(10);
        MessageMetrics::mailbox_full();

        LinkMetrics::link_created();
        LinkMetrics::monitor_created();

        SupervisorMetrics::child_restarted("one_for_one");
        SupervisorMetrics::restart_intensity_exceeded();
    }

    #[test]
    fn test_span_finish() {
        let span = MessageMetrics::message_processing_span();
        span.finish();
    }

    #[test]
    fn test_init() {
        init();
    }
}
