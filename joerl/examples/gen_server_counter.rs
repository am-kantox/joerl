//! GenServer counter example.
//!
//! Demonstrates the use of the GenServer behavior (inspired by Erlang's gen_server)
//! for building stateful actors with synchronous call/reply and asynchronous cast
//! semantics.
//!
//! ## Erlang/OTP Comparison
//!
//! This example is equivalent to the following Erlang gen_server:
//!
//! ```erlang
//! -module(counter).
//! -behaviour(gen_server).
//!
//! -export([start_link/0, increment/1, decrement/1, add/2, get/1, stop/1]).
//! -export([init/1, handle_call/3, handle_cast/2, terminate/2]).
//!
//! start_link() ->
//!     gen_server:start_link(?MODULE, [], []).
//!
//! increment(Pid) -> gen_server:cast(Pid, increment).
//! decrement(Pid) -> gen_server:cast(Pid, decrement).
//! add(Pid, N) -> gen_server:call(Pid, {add, N}).
//! get(Pid) -> gen_server:call(Pid, get).
//! stop(Pid) -> gen_server:stop(Pid).
//!
//! init([]) ->
//!     {ok, 0}.
//!
//! handle_call(get, _From, State) ->
//!     {reply, State, State};
//! handle_call({add, N}, _From, State) ->
//!     NewState = State + N,
//!     {reply, NewState, NewState}.
//!
//! handle_cast(increment, State) ->
//!     {noreply, State + 1};
//! handle_cast(decrement, State) ->
//!     {noreply, State - 1}.
//!
//! terminate(_Reason, _State) ->
//!     ok.
//! ```

use async_trait::async_trait;
use joerl::ActorSystem;
use joerl::gen_server::{GenServer, GenServerContext};

// ============================================================================
// Counter GenServer Implementation
// ============================================================================

/// The counter server struct
struct CounterServer;

/// Call messages (synchronous - wait for response)
#[derive(Debug)]
enum CounterCall {
    /// Get the current value
    Get,
    /// Add a value and return the new total
    Add(i32),
}

/// Cast messages (asynchronous - fire and forget)
#[derive(Debug)]
enum CounterCast {
    /// Increment by 1
    Increment,
    /// Decrement by 1
    Decrement,
    /// Reset to 0
    Reset,
}

#[async_trait]
impl GenServer for CounterServer {
    type State = i32;
    type Call = CounterCall;
    type Cast = CounterCast;
    type CallReply = i32;

    async fn init(&mut self, ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        println!("[Counter {}] Initializing with state = 0", ctx.pid());
        0
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> Self::CallReply {
        match call {
            CounterCall::Get => {
                println!("[Counter {}] handle_call(Get) -> {}", ctx.pid(), *state);
                *state
            }
            CounterCall::Add(n) => {
                *state += n;
                println!(
                    "[Counter {}] handle_call(Add({})) -> new state = {}",
                    ctx.pid(),
                    n,
                    *state
                );
                *state
            }
        }
    }

    async fn handle_cast(
        &mut self,
        cast: Self::Cast,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) {
        match cast {
            CounterCast::Increment => {
                *state += 1;
                println!(
                    "[Counter {}] handle_cast(Increment) -> state = {}",
                    ctx.pid(),
                    *state
                );
            }
            CounterCast::Decrement => {
                *state -= 1;
                println!(
                    "[Counter {}] handle_cast(Decrement) -> state = {}",
                    ctx.pid(),
                    *state
                );
            }
            CounterCast::Reset => {
                *state = 0;
                println!("[Counter {}] handle_cast(Reset) -> state = 0", ctx.pid());
            }
        }
    }

    async fn terminate(
        &mut self,
        reason: &joerl::ExitReason,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) {
        println!(
            "[Counter {}] Terminating with reason: {}, final state: {}",
            ctx.pid(),
            reason,
            *state
        );
    }
}

// ============================================================================
// Example Runner
// ============================================================================

#[tokio::main]
async fn main() {
    println!("=== GenServer Counter Example ===\n");
    println!("This demonstrates Erlang-style gen_server behavior in Rust\n");

    let system = ActorSystem::new();

    // Spawn the counter server
    println!("Starting counter server...\n");
    let counter = joerl::gen_server::spawn(&system, CounterServer);
    println!("Counter server started with PID: {}\n", counter.pid());

    // Wait for initialization
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("--- Asynchronous casts (fire and forget) ---\n");

    // Cast: Increment (async, no response)
    counter.cast(CounterCast::Increment).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    counter.cast(CounterCast::Increment).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    counter.cast(CounterCast::Increment).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    println!("\n--- Synchronous calls (wait for response) ---\n");

    // Call: Get (sync, returns current value)
    let value = counter.call(CounterCall::Get).await.unwrap();
    println!("Client: Received response from Get: {}\n", value);
    assert_eq!(value, 3);

    // Call: Add (sync, returns new value)
    let value = counter.call(CounterCall::Add(7)).await.unwrap();
    println!("Client: Received response from Add(7): {}\n", value);
    assert_eq!(value, 10);

    // Another Get to verify
    let value = counter.call(CounterCall::Get).await.unwrap();
    println!("Client: Received response from Get: {}\n", value);
    assert_eq!(value, 10);

    println!("--- More casts ---\n");

    // More casts
    counter.cast(CounterCast::Decrement).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    counter.cast(CounterCast::Decrement).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Final get
    let value = counter.call(CounterCall::Get).await.unwrap();
    println!("Client: Final value: {}\n", value);
    assert_eq!(value, 8);

    // Reset
    counter.cast(CounterCast::Reset).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let value = counter.call(CounterCall::Get).await.unwrap();
    println!("Client: Value after reset: {}\n", value);
    assert_eq!(value, 0);

    println!("--- Summary ---");
    println!("✓ Demonstrated async casts (increment, decrement, reset)");
    println!("✓ Demonstrated sync calls (get, add)");
    println!("✓ State management with type-safe messages");
    println!("✓ Erlang gen_server pattern in Rust");

    println!("\nExample completed!");

    // Give time for termination logging
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
