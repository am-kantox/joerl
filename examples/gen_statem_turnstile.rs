//! GenStatem turnstile example.
//!
//! Demonstrates the use of the GenStatem behavior (inspired by Erlang's gen_statem)
//! for building finite state machines with explicit state transitions.
//!
//! This example implements a classic turnstile state machine:
//! - Starts in the Locked state
//! - Accepts coins to unlock
//! - Allows pushing through when unlocked
//! - Returns to locked after someone passes through
//!
//! ## Erlang/OTP Comparison
//!
//! This example is equivalent to the following Erlang gen_statem:
//!
//! ```erlang
//! -module(turnstile).
//! -behaviour(gen_statem).
//!
//! -export([start_link/0, coin/1, push/1]).
//! -export([init/1, callback_mode/0, locked/3, unlocked/3, terminate/3]).
//!
//! start_link() ->
//!     gen_statem:start_link(?MODULE, [], []).
//!
//! coin(Pid) -> gen_statem:call(Pid, coin).
//! push(Pid) -> gen_statem:call(Pid, push).
//!
//! init([]) ->
//!     {ok, locked, #{coins => 0, passes => 0}}.
//!
//! callback_mode() -> state_functions.
//!
//! locked({call, From}, coin, Data) ->
//!     NewData = Data#{coins => maps:get(coins, Data) + 1},
//!     {next_state, unlocked, NewData, [{reply, From, unlocked}]};
//! locked({call, From}, push, Data) ->
//!     {keep_state, Data, [{reply, From, {error, locked}}]}.
//!
//! unlocked({call, From}, push, Data) ->
//!     NewData = Data#{passes => maps:get(passes, Data) + 1},
//!     {next_state, locked, NewData, [{reply, From, passed}]};
//! unlocked({call, From}, coin, Data) ->
//!     NewData = Data#{coins => maps:get(coins, Data) + 1},
//!     {keep_state, NewData, [{reply, From, thank_you}]}.
//!
//! terminate(_Reason, _State, _Data) ->
//!     ok.
//! ```

use async_trait::async_trait;
use joerl::ActorSystem;
use joerl::gen_statem::{GenStatem, StateMachineContext, StateTransition};

// ============================================================================
// Turnstile State Machine Implementation
// ============================================================================

/// The turnstile state machine struct
struct Turnstile;

/// States of the turnstile
#[derive(Debug, Clone, PartialEq)]
enum TurnstileState {
    Locked,
    Unlocked,
}

/// Events that can occur
#[derive(Debug)]
enum TurnstileEvent {
    Coin,
    Push,
}

/// Data associated with the turnstile
#[derive(Debug)]
struct TurnstileData {
    coins: u32,
    passes: u32,
}

/// Reply type for turnstile operations
#[derive(Debug, PartialEq)]
enum TurnstileReply {
    Unlocked,
    Passed,
    ThankYou,
    Locked,
}

#[async_trait]
impl GenStatem for Turnstile {
    type State = TurnstileState;
    type Data = TurnstileData;
    type Event = TurnstileEvent;
    type Reply = TurnstileReply;

    async fn init(&mut self, ctx: &mut StateMachineContext<'_, Self>) -> (Self::State, Self::Data) {
        println!("[Turnstile {}] Initializing in Locked state", ctx.pid());
        (
            TurnstileState::Locked,
            TurnstileData {
                coins: 0,
                passes: 0,
            },
        )
    }

    async fn handle_event(
        &mut self,
        event: Self::Event,
        state: Self::State,
        data: &mut Self::Data,
        ctx: &mut StateMachineContext<'_, Self>,
    ) -> StateTransition<Self> {
        println!(
            "[Turnstile {}] State={:?}, Event={:?}, Data=coins:{} passes:{}",
            ctx.pid(),
            state,
            event,
            data.coins,
            data.passes
        );

        match (state, event) {
            // Locked state - insert coin
            (TurnstileState::Locked, TurnstileEvent::Coin) => {
                data.coins += 1;
                println!(
                    "[Turnstile {}] Coin inserted! Unlocking... (total coins: {})",
                    ctx.pid(),
                    data.coins
                );
                StateTransition::next_state(TurnstileState::Unlocked, TurnstileReply::Unlocked)
            }

            // Locked state - try to push
            (TurnstileState::Locked, TurnstileEvent::Push) => {
                println!("[Turnstile {}] Cannot push - still locked!", ctx.pid());
                StateTransition::keep_state(TurnstileReply::Locked)
            }

            // Unlocked state - push through
            (TurnstileState::Unlocked, TurnstileEvent::Push) => {
                data.passes += 1;
                println!(
                    "[Turnstile {}] Pushed through! Locking... (total passes: {})",
                    ctx.pid(),
                    data.passes
                );
                StateTransition::next_state(TurnstileState::Locked, TurnstileReply::Passed)
            }

            // Unlocked state - insert another coin
            (TurnstileState::Unlocked, TurnstileEvent::Coin) => {
                data.coins += 1;
                println!(
                    "[Turnstile {}] Extra coin deposited (total coins: {})",
                    ctx.pid(),
                    data.coins
                );
                StateTransition::keep_state(TurnstileReply::ThankYou)
            }
        }
    }

    async fn state_enter(
        &mut self,
        old_state: &Self::State,
        new_state: &Self::State,
        data: &mut Self::Data,
        ctx: &mut StateMachineContext<'_, Self>,
    ) {
        println!(
            "[Turnstile {}] State transition: {:?} -> {:?} (coins:{} passes:{})",
            ctx.pid(),
            old_state,
            new_state,
            data.coins,
            data.passes
        );
    }

    async fn terminate(
        &mut self,
        reason: &joerl::ExitReason,
        state: &Self::State,
        data: &mut Self::Data,
        ctx: &mut StateMachineContext<'_, Self>,
    ) {
        println!(
            "[Turnstile {}] Terminating in state {:?} with reason: {} (coins:{} passes:{})",
            ctx.pid(),
            state,
            reason,
            data.coins,
            data.passes
        );
    }
}

// ============================================================================
// Example Runner
// ============================================================================

#[tokio::main]
async fn main() {
    println!("=== GenStatem Turnstile Example ===\n");
    println!("This demonstrates Erlang-style gen_statem (finite state machine) in Rust\n");

    let system = ActorSystem::new();

    // Spawn the turnstile state machine
    println!("Starting turnstile state machine...\n");
    let turnstile = joerl::gen_statem::spawn(&system, Turnstile);
    println!("Turnstile started with PID: {}\n", turnstile.pid());

    // Wait for initialization
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("--- Scenario 1: Try to push when locked ---\n");

    let result = turnstile.call(TurnstileEvent::Push).await.unwrap();
    println!("Client: Result = {:?}\n", result);
    assert_eq!(result, TurnstileReply::Locked);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("--- Scenario 2: Insert coin and push through ---\n");

    let result = turnstile.call(TurnstileEvent::Coin).await.unwrap();
    println!("Client: Result = {:?}\n", result);
    assert_eq!(result, TurnstileReply::Unlocked);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let result = turnstile.call(TurnstileEvent::Push).await.unwrap();
    println!("Client: Result = {:?}\n", result);
    assert_eq!(result, TurnstileReply::Passed);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("--- Scenario 3: Insert coin twice (extra coin) ---\n");

    let result = turnstile.call(TurnstileEvent::Coin).await.unwrap();
    println!("Client: Result = {:?}\n", result);
    assert_eq!(result, TurnstileReply::Unlocked);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Insert another coin while unlocked
    let result = turnstile.call(TurnstileEvent::Coin).await.unwrap();
    println!("Client: Result = {:?} (extra coin)\n", result);
    assert_eq!(result, TurnstileReply::ThankYou);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("--- Scenario 4: Push through after extra coin ---\n");

    let result = turnstile.call(TurnstileEvent::Push).await.unwrap();
    println!("Client: Result = {:?}\n", result);
    assert_eq!(result, TurnstileReply::Passed);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("--- Scenario 5: Multiple people passing through ---\n");

    for i in 1..=3 {
        println!("Person {} approaching...", i);

        let result = turnstile.call(TurnstileEvent::Coin).await.unwrap();
        println!("  Inserted coin -> {:?}", result);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let result = turnstile.call(TurnstileEvent::Push).await.unwrap();
        println!("  Pushed through -> {:?}\n", result);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    println!("--- Summary ---");
    println!("✓ Demonstrated state transitions (Locked ↔ Unlocked)");
    println!("✓ Handled events in different states");
    println!("✓ Maintained state data across transitions");
    println!("✓ state_enter callbacks for transition logging");
    println!("✓ Erlang gen_statem pattern in Rust");

    println!("\nExample completed!");

    // Give time for termination logging
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
