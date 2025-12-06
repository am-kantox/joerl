//! # joerl - An Erlang-inspired Actor Model Library for Rust
//!
//! Named in tribute to Joe Armstrong, the creator of Erlang, `joerl` brings
//! the power and elegance of the Erlang actor model to Rust using async/await.
//!
//! ## Features
//!
//! - **Actor Model**: Lightweight actors that communicate via message passing
//! - **Supervision Trees**: Robust error handling with configurable restart strategies
//! - **Links and Monitors**: Actor relationships for failure detection and propagation
//! - **Bounded Mailboxes**: Backpressure support to prevent resource exhaustion
//! - **Async/Await**: Built on tokio for excellent performance
//! - **Erlang Conventions**: Familiar API for Erlang/OTP developers
//!
//! ## Quick Start
//!
//! ```
//! use joerl::{Actor, ActorContext, ActorSystem, Message};
//! use async_trait::async_trait;
//!
//! // Define your actor
//! struct Counter {
//!     count: i32,
//! }
//!
//! #[async_trait]
//! impl Actor for Counter {
//!     async fn started(&mut self, ctx: &mut ActorContext) {
//!         println!("Counter started with pid {}", ctx.pid());
//!     }
//!
//!     async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
//!         if let Some(cmd) = msg.downcast_ref::<&str>() {
//!             match *cmd {
//!                 "increment" => self.count += 1,
//!                 "get" => println!("Count: {}", self.count),
//!                 _ => {}
//!             }
//!         }
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let system = ActorSystem::new();
//!     let counter = system.spawn(Counter { count: 0 });
//!     
//!     counter.send(Box::new("increment")).await.unwrap();
//!     counter.send(Box::new("get")).await.unwrap();
//! }
//! ```
//!
//! ## Erlang Terminology Mapping
//!
//! | Erlang | joerl |
//! |--------|-------|
//! | `spawn/1` | `system.spawn(actor)` |
//! | `Pid` | `Pid` |
//! | `send/2` (!) | `actor_ref.send(msg)` |
//! | `link/1` | `actor_ref.link(from_pid)` |
//! | `monitor/2` | `actor_ref.monitor(from_pid)` |
//! | `process_flag(trap_exit, true)` | `ctx.trap_exit(true)` |
//! | `{'EXIT', Pid, Reason}` | `Signal::Exit { from, reason }` |
//! | `{'DOWN', Ref, process, Pid, Reason}` | `Signal::Down { reference, pid, reason }` |
//!
//! ## Architecture
//!
//! The library is organized into several key modules:
//!
//! - [`actor`]: Core actor trait and context
//! - [`system`]: Actor system runtime and registry
//! - [`message`]: Message types and signals
//! - [`mailbox`]: Bounded mailbox implementation
//! - [`supervisor`]: Supervision trees and restart strategies
//! - [`gen_server`]: Generic server behavior (Erlang's gen_server)
//! - [`mod@gen_statem`]: Generic state machine behavior (Erlang's gen_statem)
//! - [`error`]: Error types and results

#![doc = include_str!("../../stuff/PANIC_HANDLING.md")]

/* content of the module */
pub mod actor;
pub mod error;
pub mod gen_server;
pub mod gen_statem;
pub mod mailbox;
pub mod message;
pub mod pid;
pub mod supervisor;
pub mod system;

// Re-export commonly used types
pub use actor::{Actor, ActorContext};
pub use error::{ActorError, Result};
pub use message::{ExitReason, Message, MonitorRef, Signal};
pub use pid::Pid;
pub use supervisor::{RestartStrategy, Supervisor, SupervisorSpec};
pub use system::{ActorRef, ActorSystem};

// Re-export proc macros
pub use joerl_macro::gen_statem;

// Re-export dependencies used in generated code
pub use async_trait;

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct PingPong {
        count: usize,
        max: usize,
    }

    #[async_trait]
    impl Actor for PingPong {
        async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
            if let Some(_other) = msg.downcast_ref::<Pid>() {
                self.count += 1;
                println!("{} received ping #{}", ctx.pid(), self.count);

                if self.count < self.max {
                    // Send back to other actor
                    let _ = ctx.pid(); // Access current pid
                }
            }
        }
    }

    #[tokio::test]
    async fn test_basic_actor_lifecycle() {
        let system = ActorSystem::new();
        let actor = system.spawn(PingPong { count: 0, max: 3 });

        actor.send(Box::new(42i32)).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert!(system.is_alive(actor.pid()));
    }

    #[tokio::test]
    async fn test_actor_link_failure() {
        use tokio::time::{Duration, sleep};

        struct FailingActor;

        #[async_trait]
        impl Actor for FailingActor {
            async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
                if let Some(cmd) = msg.downcast_ref::<&str>()
                    && *cmd == "die"
                {
                    ctx.stop(ExitReason::Panic("intentional".to_string()));
                }
            }
        }

        struct MonitoringActor {
            monitored: Option<Pid>,
            received_exit: bool,
        }

        #[async_trait]
        impl Actor for MonitoringActor {
            async fn started(&mut self, ctx: &mut ActorContext) {
                ctx.trap_exit(true);
            }

            async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
                if let Signal::Exit { from, .. } = signal
                    && Some(from) == self.monitored
                {
                    self.received_exit = true;
                }
            }

            async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
        }

        let system = ActorSystem::new();
        let failing = system.spawn(FailingActor);
        let monitoring = system.spawn(MonitoringActor {
            monitored: Some(failing.pid()),
            received_exit: false,
        });

        system.link(monitoring.pid(), failing.pid()).unwrap();

        failing.send(Box::new("die")).await.unwrap();
        sleep(Duration::from_millis(50)).await;
    }
}
