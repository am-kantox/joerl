//! Turnstile FSM example using gen_statem DSL
//!
//! This example demonstrates a turnstile state machine with the following states:
//! - locked: Initial state, waiting for coin
//! - unlocked: After coin inserted, waiting for push
//!
//! Events:
//! - coin: Insert a coin  
//! - push: Push the turnstile
//! - off: Shut down the turnstile (terminal transition)
//!
//! ## FSM Diagram
//!
//! ```text
//!     [*] --> locked
//!     locked --coin--> unlocked
//!     locked --push--> locked (electrocutes!)
//!     unlocked --push--> locked
//!     unlocked --coin--> unlocked (donation)
//!     unlocked --off--> [*] (terminate)
//! ```

use joerl::{ActorSystem, ExitReason, gen_statem};
use std::sync::Arc;

#[gen_statem(fsm = r#"
    [*] --> locked
    locked --> |coin| unlocked
    locked --> |push| locked
    unlocked --> |push| locked  
    unlocked --> |coin| unlocked
    unlocked --> |off| [*]
"#)]
#[derive(Debug, Clone)]
struct Turnstile {
    donations: u32,
    pushes: u32,
}

impl Turnstile {
    /// Called on every state transition
    fn on_transition(
        &mut self,
        event: TurnstileEvent,
        state: TurnstileState,
    ) -> TurnstileTransitionResult {
        match (state.clone(), event.clone()) {
            (TurnstileState::Locked, TurnstileEvent::Push) => {
                println!("⚡ Electrocuting person trying to push locked turnstile!");
                self.pushes += 1;
                TurnstileTransitionResult::Keep(self.clone())
            }
            (TurnstileState::Locked, TurnstileEvent::Coin) => {
                println!("💰 Coin inserted, unlocking turnstile");
                self.donations += 1;
                TurnstileTransitionResult::Next(TurnstileState::Unlocked, self.clone())
            }
            (TurnstileState::Unlocked, TurnstileEvent::Push) => {
                println!("🚶 Person passed through, locking turnstile");
                self.pushes += 1;
                TurnstileTransitionResult::Next(TurnstileState::Locked, self.clone())
            }
            (TurnstileState::Unlocked, TurnstileEvent::Coin) => {
                println!("🎁 Thanks! This donation will go to the animal shelter");
                self.donations += 1;
                TurnstileTransitionResult::Keep(self.clone())
            }
            (TurnstileState::Unlocked, TurnstileEvent::Off) => {
                println!("🔌 Shutting down turnstile");
                // FSM specifies unlocked->off->[*], so we stay in Unlocked
                // but the macro will detect this is a terminal transition
                TurnstileTransitionResult::Keep(self.clone())
            }
            _ => TurnstileTransitionResult::Error(format!(
                "Invalid transition: {:?} with event {:?}",
                state, event
            )),
        }
    }

    /// Called when entering a new state
    fn on_enter(
        &self,
        old_state: &TurnstileState,
        new_state: &TurnstileState,
        _data: &TurnstileData,
    ) {
        println!("🔄 State transition: {:?} -> {:?}", old_state, new_state);
    }

    /// Called on termination
    fn on_terminate(&self, reason: &ExitReason, state: &TurnstileState, data: &TurnstileData) {
        println!(
            "💀 Turnstile terminated in state {:?} with reason {:?}",
            state, reason
        );
        println!(
            "📊 Final stats: {} donations, {} pushes",
            data.donations, data.pushes
        );
    }
}

#[tokio::main]
async fn main() {
    println!("\n╔═══════════════════════════════════════╗");
    println!("║  Turnstile FSM Example (gen_statem)  ║");
    println!("╚═══════════════════════════════════════╝\n");

    println!("FSM States: locked, unlocked");
    println!("Events: coin, push, off\n");
    println!("Starting turnstile in LOCKED state...\n");
    println!("───────────────────────────────────────\n");

    let system = Arc::new(ActorSystem::new());

    let initial_data = Turnstile {
        donations: 0,
        pushes: 0,
    };

    let turnstile = Turnstile(&system, initial_data);

    // Simulate usage
    println!("[Event] Sending: Push (while locked)");
    turnstile
        .send(Box::new(TurnstileEvent::Push))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    println!("[Event] Sending: Coin");
    turnstile
        .send(Box::new(TurnstileEvent::Coin))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    println!("[Event] Sending: Coin (donation while unlocked)");
    turnstile
        .send(Box::new(TurnstileEvent::Coin))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    println!("[Event] Sending: Push (person passes through)");
    turnstile
        .send(Box::new(TurnstileEvent::Push))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    println!("[Event] Sending: Coin");
    turnstile
        .send(Box::new(TurnstileEvent::Coin))
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    println!("[Event] Sending: Off (shutdown)");
    turnstile.send(Box::new(TurnstileEvent::Off)).await.unwrap();

    // Give time for termination
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    println!("\n───────────────────────────────────────");
    println!("✅ Example completed successfully");
    println!("\nKey Features Demonstrated:");
    println!("  • State transitions with validation");
    println!("  • Event handling (coin, push, off)");
    println!("  • Keep transitions (coin while unlocked)");
    println!("  • Terminal transitions (off → terminate)");
    println!("  • Callback invocations (on_transition, on_enter, on_terminate)");
    println!("  • Data persistence across transitions\n");
}
