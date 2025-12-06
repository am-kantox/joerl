//! Panic handling demonstration.
//!
//! This example demonstrates how joerl handles actor panics in a way that
//! mimics Erlang/OTP behavior:
//! - Panics are caught and converted to ExitReason::Panic
//! - Links receive EXIT signals
//! - Monitors receive DOWN signals
//! - Supervisors automatically restart panicked actors
//! - Cleanup always happens (no resource leaks)
//!
//! Run with: cargo run --example panic_handling

use async_trait::async_trait;
use joerl::{
    Actor, ActorContext, ActorSystem, ExitReason, Message, Signal,
    supervisor::{ChildSpec, RestartIntensity, RestartStrategy, SupervisorSpec, spawn_supervisor},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::time::{Duration, sleep};

// ============================================================================
// SCENARIO 1: Basic Panic in Actor
// ============================================================================

/// A simple worker that panics when told to do so.
struct PanickingWorker {
    name: String,
}

#[async_trait]
impl Actor for PanickingWorker {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("✓ [{}] Worker started: {}", self.name, ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "panic" => {
                    println!("💥 [{}] About to panic!", self.name);
                    panic!("Intentional panic in {}", self.name);
                }
                "work" => {
                    println!("⚙️  [{}] Doing some work", self.name);
                }
                "stop" => {
                    println!("🛑 [{}] Stopping normally", self.name);
                    ctx.stop(ExitReason::Normal);
                }
                _ => {}
            }
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, ctx: &mut ActorContext) {
        match reason {
            ExitReason::Panic(msg) => {
                println!("✗ [{}] Stopped due to panic: {}", self.name, msg);
            }
            _ => {
                println!("✓ [{}] Stopped: {}", self.name, reason);
            }
        }
        println!("🧹 [{}] Cleanup completed for {}", self.name, ctx.pid());
    }
}

// ============================================================================
// SCENARIO 2: Panic with Links
// ============================================================================

/// A supervisor that traps exits to handle linked actor failures.
struct LinkSupervisor {
    name: String,
    panic_detected: Arc<AtomicBool>,
}

#[async_trait]
impl Actor for LinkSupervisor {
    async fn started(&mut self, ctx: &mut ActorContext) {
        ctx.trap_exit(true);
        println!(
            "✓ [{}] Supervisor started (trapping exits): {}",
            self.name,
            ctx.pid()
        );
    }

    async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}

    async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
        if let Signal::Exit { from, reason } = signal {
            match reason {
                ExitReason::Panic(msg) => {
                    println!(
                        "📨 [{}] EXIT signal received! Linked actor {} panicked: {}",
                        self.name, from, msg
                    );
                    println!(
                        "   [{}] I detected the panic and can handle it gracefully!",
                        self.name
                    );
                    self.panic_detected.store(true, Ordering::SeqCst);
                }
                _ => {
                    println!("📨 [{}] EXIT signal from {}: {}", self.name, from, reason);
                }
            }
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, _ctx: &mut ActorContext) {
        println!("✗ [{}] Supervisor stopped: {}", self.name, reason);
    }
}

// ============================================================================
// SCENARIO 3: Panic with Monitors
// ============================================================================

/// An observer that monitors actors without being affected by their failures.
struct PanicObserver {
    name: String,
    down_count: Arc<AtomicUsize>,
}

#[async_trait]
impl Actor for PanicObserver {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("👁️  [{}] Observer started: {}", self.name, ctx.pid());
    }

    async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}

    async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
        if let Signal::Down {
            reference,
            pid,
            reason,
        } = signal
        {
            match reason {
                ExitReason::Panic(msg) => {
                    let count = self.down_count.fetch_add(1, Ordering::SeqCst) + 1;
                    println!(
                        "📨 [{}] DOWN signal #{}: Actor {} panicked: {}",
                        self.name, count, pid, msg
                    );
                    println!("   [{}] Monitor ref: {}", self.name, reference);
                    println!(
                        "   [{}] I'm still alive and can continue observing!",
                        self.name
                    );
                }
                _ => {
                    println!("📨 [{}] DOWN signal from {}: {}", self.name, pid, reason);
                }
            }
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, _ctx: &mut ActorContext) {
        println!("✗ [{}] Observer stopped: {}", self.name, reason);
    }
}

// ============================================================================
// SCENARIO 4: Panic with Supervisor Restart
// ============================================================================

/// A worker that panics on first start but succeeds on restart.
struct UnstableWorker {
    name: String,
    start_count: Arc<AtomicUsize>,
}

#[async_trait]
impl Actor for UnstableWorker {
    async fn started(&mut self, ctx: &mut ActorContext) {
        let count = self.start_count.fetch_add(1, Ordering::SeqCst);
        println!(
            "✓ [{}] Worker starting (attempt #{}): {}",
            self.name,
            count + 1,
            ctx.pid()
        );

        if count == 0 {
            // First start: panic immediately
            println!("💥 [{}] Panicking on first start!", self.name);
            panic!("Initial startup panic");
        } else {
            // Subsequent starts: work normally
            println!("✓ [{}] Successfully started after restart!", self.name);
        }
    }

    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>()
            && *cmd == "work"
        {
            println!("⚙️  [{}] Working normally after recovery", self.name);
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, _ctx: &mut ActorContext) {
        println!("✗ [{}] Stopped: {}", self.name, reason);
    }
}

// ============================================================================
// SCENARIO 5: Panic in Different Lifecycle Hooks
// ============================================================================

/// A worker that can panic in different lifecycle stages.
struct LifecyclePanic {
    name: String,
    panic_stage: &'static str,
}

#[async_trait]
impl Actor for LifecyclePanic {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("✓ [{}] Started: {}", self.name, ctx.pid());
        if self.panic_stage == "started" {
            println!("💥 [{}] Panicking in started() hook!", self.name);
            panic!("Panic in started hook");
        }
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            if *cmd == "panic_now" && self.panic_stage == "handle_message" {
                println!("💥 [{}] Panicking in handle_message()!", self.name);
                panic!("Panic in message handler");
            } else if *cmd == "stop" {
                ctx.stop(ExitReason::Normal);
            }
        }
    }

    async fn stopped(&mut self, _reason: &ExitReason, _ctx: &mut ActorContext) {
        if self.panic_stage == "stopped" {
            println!("💥 [{}] Panicking in stopped() hook!", self.name);
            // Even if stopped panics, cleanup will still happen!
            panic!("Panic in stopped hook");
        }
        println!("✓ [{}] Cleanup in stopped()", self.name);
    }
}

// ============================================================================
// MAIN
// ============================================================================

#[tokio::main]
async fn main() {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║        JOERL - PANIC HANDLING DEMONSTRATION              ║");
    println!("║        (Erlang/OTP-style error handling in Rust)         ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    let system = Arc::new(ActorSystem::new());

    // ========================================================================
    // SCENARIO 1: Basic Panic
    // ========================================================================
    println!("\n┌─────────────────────────────────────────────────────────┐");
    println!("│ SCENARIO 1: Basic Panic in Actor                       │");
    println!("└─────────────────────────────────────────────────────────┘");
    println!("Demonstrates that panics are caught and cleanup happens.\n");

    let worker1 = system.spawn(PanickingWorker {
        name: "BasicWorker".to_string(),
    });

    sleep(Duration::from_millis(50)).await;
    worker1.send(Box::new("work")).await.ok();
    sleep(Duration::from_millis(50)).await;

    println!("\nTriggering panic...");
    worker1.send(Box::new("panic")).await.ok();
    sleep(Duration::from_millis(200)).await;

    println!("✓ Actor cleaned up properly despite panic!");

    // ========================================================================
    // SCENARIO 2: Panic with Links
    // ========================================================================
    println!("\n\n┌─────────────────────────────────────────────────────────┐");
    println!("│ SCENARIO 2: Panic with Links                           │");
    println!("└─────────────────────────────────────────────────────────┘");
    println!("Linked actors receive EXIT signals when one panics.\n");

    let panic_detected = Arc::new(AtomicBool::new(false));

    let supervisor = system.spawn(LinkSupervisor {
        name: "Supervisor".to_string(),
        panic_detected: Arc::clone(&panic_detected),
    });

    let worker2 = system.spawn(PanickingWorker {
        name: "LinkedWorker".to_string(),
    });

    // Link them together
    system.link(supervisor.pid(), worker2.pid()).unwrap();
    sleep(Duration::from_millis(50)).await;

    println!("Triggering panic in linked worker...");
    worker2.send(Box::new("panic")).await.ok();
    sleep(Duration::from_millis(200)).await;

    if panic_detected.load(Ordering::SeqCst) {
        println!("✓ Supervisor successfully detected and handled the panic!");
    }

    // ========================================================================
    // SCENARIO 3: Panic with Monitors
    // ========================================================================
    println!("\n\n┌─────────────────────────────────────────────────────────┐");
    println!("│ SCENARIO 3: Panic with Monitors                        │");
    println!("└─────────────────────────────────────────────────────────┘");
    println!("Monitors receive DOWN signals but aren't affected.\n");

    let down_count = Arc::new(AtomicUsize::new(0));

    let observer = system.spawn(PanicObserver {
        name: "Observer".to_string(),
        down_count: Arc::clone(&down_count),
    });

    let worker3 = system.spawn(PanickingWorker {
        name: "MonitoredWorker".to_string(),
    });

    // Observer monitors worker3
    worker3.monitor(observer.pid()).unwrap();
    sleep(Duration::from_millis(50)).await;

    println!("Triggering panic in monitored worker...");
    worker3.send(Box::new("panic")).await.ok();
    sleep(Duration::from_millis(200)).await;

    if down_count.load(Ordering::SeqCst) > 0 {
        println!("✓ Observer received DOWN signal and continues running!");
    }

    // ========================================================================
    // SCENARIO 4: Supervisor Restart
    // ========================================================================
    println!("\n\n┌─────────────────────────────────────────────────────────┐");
    println!("│ SCENARIO 4: Supervisor Automatically Restarts          │");
    println!("└─────────────────────────────────────────────────────────┘");
    println!("Supervisors detect panics and restart children.\n");

    let start_count = Arc::new(AtomicUsize::new(0));
    let start_count_clone = Arc::clone(&start_count);

    let spec = SupervisorSpec::new(RestartStrategy::OneForOne)
        .intensity(RestartIntensity {
            max_restarts: 3,
            within_seconds: 5,
        })
        .child(ChildSpec::new("unstable", move || {
            Box::new(UnstableWorker {
                name: "UnstableWorker".to_string(),
                start_count: Arc::clone(&start_count_clone),
            })
        }));

    let _sup = spawn_supervisor(&system, spec);

    println!("Supervisor starting child (which will panic)...");
    sleep(Duration::from_millis(300)).await;

    let starts = start_count.load(Ordering::SeqCst);
    println!("\n✓ Worker started {} times (initial + restart)", starts);
    if starts >= 2 {
        println!("✓ Supervisor successfully restarted the panicked worker!");
    }

    // ========================================================================
    // SCENARIO 5: Panic in Lifecycle Hooks
    // ========================================================================
    println!("\n\n┌─────────────────────────────────────────────────────────┐");
    println!("│ SCENARIO 5: Panic in Lifecycle Hooks                   │");
    println!("└─────────────────────────────────────────────────────────┘");
    println!("Even panics in started/stopped hooks are handled.\n");

    // Test panic in started()
    println!("5a. Panic in started() hook:");
    let down_count2 = Arc::new(AtomicUsize::new(0));
    let obs2 = system.spawn(PanicObserver {
        name: "LifecycleObserver".to_string(),
        down_count: Arc::clone(&down_count2),
    });

    let lc1 = system.spawn(LifecyclePanic {
        name: "StartedPanic".to_string(),
        panic_stage: "started",
    });
    lc1.monitor(obs2.pid()).unwrap();
    sleep(Duration::from_millis(200)).await;

    if down_count2.load(Ordering::SeqCst) > 0 {
        println!("✓ Monitor notified even when panic was in started()!\n");
    }

    // Test panic in handle_message()
    println!("5b. Panic in handle_message():");
    let down_count3 = Arc::new(AtomicUsize::new(0));
    let obs3 = system.spawn(PanicObserver {
        name: "MessageObserver".to_string(),
        down_count: Arc::clone(&down_count3),
    });

    let lc2 = system.spawn(LifecyclePanic {
        name: "MessagePanic".to_string(),
        panic_stage: "handle_message",
    });
    lc2.monitor(obs3.pid()).unwrap();
    sleep(Duration::from_millis(50)).await;
    lc2.send(Box::new("panic_now")).await.ok();
    sleep(Duration::from_millis(200)).await;

    if down_count3.load(Ordering::SeqCst) > 0 {
        println!("✓ Monitor notified for panic in message handler!\n");
    }

    // Test panic in stopped()
    println!("5c. Panic in stopped() hook:");
    let down_count4 = Arc::new(AtomicUsize::new(0));
    let obs4 = system.spawn(PanicObserver {
        name: "StoppedObserver".to_string(),
        down_count: Arc::clone(&down_count4),
    });

    let lc3 = system.spawn(LifecyclePanic {
        name: "StoppedPanic".to_string(),
        panic_stage: "stopped",
    });
    lc3.monitor(obs4.pid()).unwrap();
    sleep(Duration::from_millis(50)).await;
    lc3.send(Box::new("stop")).await.ok();
    sleep(Duration::from_millis(200)).await;

    if down_count4.load(Ordering::SeqCst) > 0 {
        println!("✓ Monitor notified even when stopped() panicked!");
        println!("✓ Cleanup still happened despite panic in stopped()!");
    }

    // ========================================================================
    // SUMMARY
    // ========================================================================
    println!("\n\n╔═══════════════════════════════════════════════════════════╗");
    println!("║                    SUMMARY                               ║");
    println!("╠═══════════════════════════════════════════════════════════╣");
    println!("║ ✓ Panics are caught and converted to ExitReason::Panic  ║");
    println!("║ ✓ Links receive EXIT signals when actors panic          ║");
    println!("║ ✓ Monitors receive DOWN signals when actors panic       ║");
    println!("║ ✓ Supervisors automatically restart panicked children   ║");
    println!("║ ✓ Cleanup ALWAYS happens (no resource leaks)            ║");
    println!("║ ✓ Panics in ANY lifecycle stage are handled properly    ║");
    println!("║                                                           ║");
    println!("║ This matches Erlang/OTP behavior: isolated failures,    ║");
    println!("║ proper notification, and automatic recovery.             ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");
}
