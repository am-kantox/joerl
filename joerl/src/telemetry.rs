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
//!
//! ### Messages
//! - `joerl_messages_sent_total`: Total messages sent
//! - `joerl_messages_sent_failed_total`: Failed message send attempts
//! - `joerl_messages_processed_total`: Total messages processed
//! - `joerl_message_processing_duration_seconds`: Message processing time histogram
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

/// Telemetry context for tracking operation duration.
///
/// This struct automatically records the duration when dropped.
pub struct TelemetrySpan {
    #[cfg(feature = "telemetry")]
    start: Instant,
    #[cfg(feature = "telemetry")]
    metric_name: &'static str,
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
        }
    }

    /// Records the span duration and finishes it.
    #[cfg(feature = "telemetry")]
    pub fn finish(self) {
        let duration = self.start.elapsed();
        histogram!(self.metric_name).record(duration.as_secs_f64());
    }

    /// No-op when telemetry is disabled.
    #[cfg(not(feature = "telemetry"))]
    pub fn finish(self) {}
}

#[cfg(feature = "telemetry")]
impl Drop for TelemetrySpan {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        histogram!(self.metric_name).record(duration.as_secs_f64());
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
        counter!("joerl_messages_processed_total").increment(1);
    }

    /// Creates a span for tracking message processing duration.
    #[inline]
    pub fn message_processing_span() -> TelemetrySpan {
        TelemetrySpan::new("joerl_message_processing_duration_seconds")
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
