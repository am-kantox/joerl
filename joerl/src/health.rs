//! Health check system for monitoring actor system health.
//!
//! This module provides health check functionality for production deployments,
//! including Kubernetes liveness/readiness probes and monitoring integrations.
//!
//! ## Features
//!
//! - Configurable health thresholds
//! - Actor system health indicators
//! - HTTP endpoint support (optional feature)
//! - Kubernetes integration ready
//!
//! ## Examples
//!
//! ```rust
//! use joerl::health::{SystemHealth, HealthConfig};
//! use joerl::ActorSystem;
//!
//! # async fn example() {
//! let system = ActorSystem::new();
//! let config = HealthConfig::default();
//! let health = SystemHealth::new(&system, config);
//!
//! if health.is_healthy() {
//!     println!("System is healthy!");
//! } else {
//!     println!("System unhealthy: {:?}", health.get_issues());
//! }
//! # }
//! ```

use crate::ActorSystem;
use std::sync::Arc;

/// Configuration for health check thresholds.
///
/// These thresholds determine when the system is considered unhealthy.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Maximum actor count before considering unhealthy (0 = unlimited)
    pub max_actors: usize,
    /// Minimum actor count (system unhealthy if below this)
    pub min_actors: usize,
    /// Maximum panic rate per second before unhealthy
    pub max_panic_rate: f64,
    /// Maximum restart rate per second before unhealthy
    pub max_restart_rate: f64,
    /// Maximum mailbox backpressure events per second
    pub max_backpressure_rate: f64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            max_actors: 10_000,           // Alert if > 10K actors
            min_actors: 0,                // No minimum by default
            max_panic_rate: 1.0,          // 1 panic/sec threshold
            max_restart_rate: 10.0,       // 10 restarts/sec threshold
            max_backpressure_rate: 100.0, // 100 backpressure events/sec
        }
    }
}

/// Health status of the actor system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// System is healthy and operating normally
    Healthy,
    /// System is degraded but still operational
    Degraded,
    /// System is unhealthy and may not be operational
    Unhealthy,
}

/// Health issue detected in the system.
#[derive(Debug, Clone)]
pub struct HealthIssue {
    /// Severity of the issue
    pub severity: IssueSeverity,
    /// Description of the issue
    pub description: String,
    /// Current value that triggered the issue
    pub current_value: f64,
    /// Threshold that was exceeded
    pub threshold: f64,
}

/// Severity level of a health issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// Warning - system degraded but operational
    Warning,
    /// Critical - system unhealthy
    Critical,
}

/// System health checker.
///
/// Provides health check functionality for monitoring and alerting.
pub struct SystemHealth {
    system: Arc<ActorSystem>,
    config: HealthConfig,
}

impl SystemHealth {
    /// Creates a new health checker for the given actor system.
    pub fn new(system: &Arc<ActorSystem>, config: HealthConfig) -> Self {
        Self {
            system: Arc::clone(system),
            config,
        }
    }

    /// Checks if the system is healthy.
    ///
    /// Returns `true` if all health checks pass, `false` otherwise.
    pub fn is_healthy(&self) -> bool {
        matches!(self.get_status(), HealthStatus::Healthy)
    }

    /// Checks if the system is ready to accept traffic.
    ///
    /// Similar to `is_healthy()` but may have different criteria.
    /// Used for Kubernetes readiness probes.
    pub fn is_ready(&self) -> bool {
        // For now, ready == healthy
        // Could be extended to check for initialization state
        self.is_healthy()
    }

    /// Gets the current health status.
    pub fn get_status(&self) -> HealthStatus {
        let issues = self.get_issues();

        if issues.is_empty() {
            return HealthStatus::Healthy;
        }

        // Check for critical issues
        let has_critical = issues.iter().any(|i| i.severity == IssueSeverity::Critical);

        if has_critical {
            HealthStatus::Unhealthy
        } else {
            HealthStatus::Degraded
        }
    }

    /// Gets a list of current health issues.
    pub fn get_issues(&self) -> Vec<HealthIssue> {
        let mut issues = Vec::new();

        // Check actor count
        let actor_count = self.system.actor_count();

        if self.config.max_actors > 0 && actor_count > self.config.max_actors {
            issues.push(HealthIssue {
                severity: IssueSeverity::Warning,
                description: format!("Too many actors: {}", actor_count),
                current_value: actor_count as f64,
                threshold: self.config.max_actors as f64,
            });
        }

        if actor_count < self.config.min_actors {
            issues.push(HealthIssue {
                severity: IssueSeverity::Critical,
                description: format!("Too few actors: {}", actor_count),
                current_value: actor_count as f64,
                threshold: self.config.min_actors as f64,
            });
        }

        // Note: Panic rate, restart rate, and backpressure rate checks
        // would require time-windowed metric tracking, which we can add
        // in a future enhancement. For now, we focus on current state.

        issues
    }

    /// Gets a detailed health report as a string.
    pub fn get_report(&self) -> String {
        let status = self.get_status();
        let issues = self.get_issues();
        let actor_count = self.system.actor_count();

        let mut report = format!("Status: {:?}\nActors: {}\n", status, actor_count);

        if !issues.is_empty() {
            report.push_str("\nIssues:\n");
            for issue in issues {
                report.push_str(&format!(
                    "  [{:?}] {} (current: {:.1}, threshold: {:.1})\n",
                    issue.severity, issue.description, issue.current_value, issue.threshold
                ));
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actor, ActorContext, Message};
    use async_trait::async_trait;

    struct TestActor;

    #[async_trait]
    impl Actor for TestActor {
        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    }

    #[tokio::test]
    async fn test_health_check_healthy() {
        let system = ActorSystem::new();
        let config = HealthConfig::default();
        let health = SystemHealth::new(&system, config);

        assert!(health.is_healthy());
        assert_eq!(health.get_status(), HealthStatus::Healthy);
        assert!(health.get_issues().is_empty());
    }

    #[tokio::test]
    async fn test_health_check_too_many_actors() {
        let system = ActorSystem::new();
        let config = HealthConfig {
            max_actors: 2,
            ..Default::default()
        };

        // Spawn 3 actors (exceeds max of 2)
        let _a1 = system.spawn(TestActor);
        let _a2 = system.spawn(TestActor);
        let _a3 = system.spawn(TestActor);

        let health = SystemHealth::new(&system, config);

        assert_eq!(health.get_status(), HealthStatus::Degraded);
        assert_eq!(health.get_issues().len(), 1);
        assert_eq!(health.get_issues()[0].severity, IssueSeverity::Warning);
    }

    #[tokio::test]
    async fn test_health_check_ready() {
        let system = ActorSystem::new();
        let config = HealthConfig::default();
        let health = SystemHealth::new(&system, config);

        assert!(health.is_ready());
    }

    #[tokio::test]
    async fn test_health_report() {
        let system = ActorSystem::new();
        let config = HealthConfig::default();
        let health = SystemHealth::new(&system, config);

        let report = health.get_report();
        assert!(report.contains("Status: Healthy"));
        assert!(report.contains("Actors: 0"));
    }
}
