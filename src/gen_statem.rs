//! Generic state machine behavior inspired by Erlang's `gen_statem`.
//!
//! This module provides a framework for building finite state machines (FSMs)
//! with state transitions and event handling, mirroring Erlang/OTP's gen_statem
//! behavior.
//!
//! ## Erlang/OTP Comparison
//!
//! In Erlang, gen_statem provides state machine patterns:
//!
//! ```erlang
//! -module(door).
//! -behaviour(gen_statem).
//!
//! callback_mode() -> state_functions.
//!
//! init([]) -> {ok, locked, #{code => "1234"}}.
//!
//! locked({call, From}, {button, Digit}, Data) ->
//!     case enter_code(Digit, Data) of
//!         {ok, NewData} ->
//!             {next_state, open, NewData, [{reply, From, ok}]};
//!         {error, NewData} ->
//!             {keep_state, NewData, [{reply, From, error}]}
//!     end;
//! locked(cast, timeout, Data) ->
//!     {keep_state, Data}.
//!
//! open({call, From}, lock, Data) ->
//!     {next_state, locked, Data, [{reply, From, locked}]}.
//! ```
//!
//! In joerl, the equivalent looks like:
//!
//! ```rust
//! use joerl::gen_statem::{GenStatem, StateMachineContext, StateTransition};
//! use async_trait::async_trait;
//!
//! struct Door;
//!
//! #[derive(Debug, Clone, PartialEq)]
//! enum DoorState {
//!     Locked,
//!     Open,
//! }
//!
//! #[derive(Debug)]
//! enum DoorEvent {
//!     Button(char),
//!     Lock,
//! }
//!
//! #[async_trait]
//! impl GenStatem for Door {
//!     type State = DoorState;
//!     type Data = String;  // PIN code
//!     type Event = DoorEvent;
//!     type Reply = Result<(), String>;
//!
//!     async fn init(&mut self, _ctx: &mut StateMachineContext<'_, Self>) -> (Self::State, Self::Data) {
//!         (DoorState::Locked, "1234".to_string())
//!     }
//!
//!     async fn handle_event(
//!         &mut self,
//!         event: Self::Event,
//!         state: Self::State,
//!         data: &mut Self::Data,
//!         _ctx: &mut StateMachineContext<'_, Self>,
//!     ) -> StateTransition<Self> {
//!         match (state, event) {
//!             (DoorState::Locked, DoorEvent::Button(digit)) => {
//!                 if digit == data.chars().next().unwrap() {
//!                     StateTransition::next_state(DoorState::Open, Ok(()))
//!                 } else {
//!                     StateTransition::keep_state(Err("Wrong code".into()))
//!                 }
//!             }
//!             (DoorState::Open, DoorEvent::Lock) => {
//!                 StateTransition::next_state(DoorState::Locked, Ok(()))
//!             }
//!             _ => StateTransition::keep_state(Err("Invalid event".into())),
//!         }
//!     }
//! }
//! ```
//!
//! ## Key Concepts
//!
//! - **State**: The current state of the machine (e.g., Locked, Open)
//! - **Data**: Associated data that persists across state transitions
//! - **Events**: Inputs that trigger state transitions
//! - **Transitions**: Changes from one state to another
//!
//! ## Usage
//!
//! ```rust
//! use joerl::{ActorSystem, gen_statem};
//!
//! # use async_trait::async_trait;
//! # struct Door;
//! # #[derive(Debug, Clone, PartialEq)]
//! # enum DoorState { Locked, Open }
//! # #[derive(Debug)]
//! # enum DoorEvent { Button(char), Lock }
//! # #[async_trait]
//! # impl gen_statem::GenStatem for Door {
//! #     type State = DoorState;
//! #     type Data = String;
//! #     type Event = DoorEvent;
//! #     type Reply = Result<(), String>;
//! #     async fn init(&mut self, _ctx: &mut gen_statem::StateMachineContext<'_, Self>) -> (Self::State, Self::Data) {
//! #         (DoorState::Locked, "1234".to_string())
//! #     }
//! #     async fn handle_event(&mut self, event: Self::Event, state: Self::State, data: &mut Self::Data, _ctx: &mut gen_statem::StateMachineContext<'_, Self>) -> gen_statem::StateTransition<Self> {
//! #         gen_statem::StateTransition::keep_state(Ok(()))
//! #     }
//! # }
//! # async fn example() {
//! let system = ActorSystem::new();
//! let door = gen_statem::spawn(&system, Door);
//!
//! // Send event and wait for reply
//! let result = door.call(DoorEvent::Button('1')).await.unwrap();
//! # }
//! ```

use crate::{
    actor::{Actor, ActorContext},
    error::{ActorError, Result},
    message::{ExitReason, Message},
    pid::Pid,
    system::{ActorRef, ActorSystem},
};
use async_trait::async_trait;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::oneshot;

/// The GenStatem trait defines callbacks for the generic state machine behavior.
///
/// This trait is inspired by Erlang's gen_statem behavior and provides
/// a structured way to build finite state machines with explicit state
/// transitions.
///
/// ## Type Parameters
///
/// - `State`: The state type (e.g., enum of possible states)
/// - `Data`: Associated data that persists across transitions
/// - `Event`: The event type that triggers transitions
/// - `Reply`: The reply type for synchronous calls
#[async_trait]
pub trait GenStatem: Send + 'static {
    /// The state type
    type State: Send + Clone + Debug + 'static;

    /// The data type associated with the state machine
    type Data: Send + 'static;

    /// The event type
    type Event: Send + Debug + 'static;

    /// The reply type for calls
    type Reply: Send + 'static;

    /// Initialize the state machine.
    ///
    /// Returns the initial state and data.
    ///
    /// In Erlang: `init/1` returning `{ok, State, Data}`
    async fn init(&mut self, ctx: &mut StateMachineContext<'_, Self>) -> (Self::State, Self::Data);

    /// Handle an event in a given state.
    ///
    /// This is called for all events and should return a state transition.
    ///
    /// In Erlang: state function callbacks (e.g., `locked/3`, `open/3`)
    async fn handle_event(
        &mut self,
        event: Self::Event,
        state: Self::State,
        data: &mut Self::Data,
        ctx: &mut StateMachineContext<'_, Self>,
    ) -> StateTransition<Self>;

    /// Called when entering a new state (optional).
    ///
    /// Override this to perform actions when transitioning into a state.
    ///
    /// In Erlang: state_enter calls
    async fn state_enter(
        &mut self,
        _old_state: &Self::State,
        _new_state: &Self::State,
        _data: &mut Self::Data,
        _ctx: &mut StateMachineContext<'_, Self>,
    ) {
        // Default: no-op
    }

    /// Handle state machine termination.
    ///
    /// Called when the state machine is about to stop.
    ///
    /// In Erlang: `terminate/3`
    async fn terminate(
        &mut self,
        _reason: &ExitReason,
        _state: &Self::State,
        _data: &mut Self::Data,
        _ctx: &mut StateMachineContext<'_, Self>,
    ) {
        // Default: no-op
    }
}

/// Result of processing an event - indicates state transition.
///
/// This type specifies what should happen after processing an event:
/// - Keep the current state
/// - Transition to a new state
/// - Stop the state machine
pub struct StateTransition<G: GenStatem + ?Sized> {
    /// The new state (None means keep current state)
    pub(crate) new_state: Option<G::State>,
    /// Reply to send back for calls
    pub(crate) reply: G::Reply,
    /// Whether to stop the state machine
    pub(crate) stop: bool,
}

impl<G: GenStatem> StateTransition<G> {
    /// Keep the current state.
    ///
    /// In Erlang: `{keep_state, Data, [{reply, From, Reply}]}`
    pub fn keep_state(reply: G::Reply) -> Self {
        Self {
            new_state: None,
            reply,
            stop: false,
        }
    }

    /// Transition to a new state.
    ///
    /// In Erlang: `{next_state, NewState, Data, [{reply, From, Reply}]}`
    pub fn next_state(new_state: G::State, reply: G::Reply) -> Self {
        Self {
            new_state: Some(new_state),
            reply,
            stop: false,
        }
    }

    /// Stop the state machine.
    ///
    /// In Erlang: `{stop, normal, Reply, Data}`
    pub fn stop(reply: G::Reply) -> Self {
        Self {
            new_state: None,
            reply,
            stop: true,
        }
    }
}

/// Context provided to GenStatem callbacks.
///
/// Provides access to the state machine's Pid and control operations.
pub struct StateMachineContext<'a, G: GenStatem + ?Sized> {
    actor_ctx: &'a mut ActorContext,
    _phantom: PhantomData<G>,
}

impl<'a, G: GenStatem> StateMachineContext<'a, G> {
    fn new(actor_ctx: &'a mut ActorContext) -> Self {
        Self {
            actor_ctx,
            _phantom: PhantomData,
        }
    }

    /// Get the state machine's Pid
    pub fn pid(&self) -> Pid {
        self.actor_ctx.pid()
    }

    /// Stop the state machine with a reason
    pub fn stop(&mut self, reason: ExitReason) {
        self.actor_ctx.stop(reason);
    }

    /// Enable or disable exit signal trapping
    pub fn trap_exit(&mut self, trap: bool) {
        self.actor_ctx.trap_exit(trap);
    }
}

/// Internal message types for GenStatem
enum StatemMsg<G: GenStatem> {
    Call {
        event: G::Event,
        reply_tx: oneshot::Sender<G::Reply>,
    },
    Cast {
        event: G::Event,
    },
}

/// Internal actor implementation that wraps a GenStatem
struct GenStatemActor<G: GenStatem> {
    statem: G,
    state: Option<G::State>,
    data: Option<G::Data>,
}

impl<G: GenStatem> GenStatemActor<G> {
    fn new(statem: G) -> Self {
        Self {
            statem,
            state: None,
            data: None,
        }
    }
}

#[async_trait]
impl<G: GenStatem> Actor for GenStatemActor<G> {
    async fn started(&mut self, ctx: &mut ActorContext) {
        let mut sm_ctx = StateMachineContext::new(ctx);
        let (state, data) = self.statem.init(&mut sm_ctx).await;
        self.state = Some(state);
        self.data = Some(data);
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        let state = match self.state.take() {
            Some(s) => s,
            None => return, // Not initialized yet
        };

        let data = match self.data.as_mut() {
            Some(d) => d,
            None => {
                self.state = Some(state);
                return;
            }
        };

        let mut sm_ctx = StateMachineContext::new(ctx);

        // Downcast to owned value
        if let Ok(statem_msg) = msg.downcast::<StatemMsg<G>>() {
            match *statem_msg {
                StatemMsg::Call { event, reply_tx } => {
                    let transition = self
                        .statem
                        .handle_event(event, state.clone(), data, &mut sm_ctx)
                        .await;

                    let _ = reply_tx.send(transition.reply);

                    if transition.stop {
                        sm_ctx.stop(ExitReason::Normal);
                        self.state = Some(state);
                        return;
                    }

                    if let Some(new_state) = transition.new_state {
                        // State transition occurred
                        self.statem
                            .state_enter(&state, &new_state, data, &mut sm_ctx)
                            .await;
                        self.state = Some(new_state);
                    } else {
                        // Keep current state
                        self.state = Some(state);
                    }
                }
                StatemMsg::Cast { event } => {
                    let transition = self
                        .statem
                        .handle_event(event, state.clone(), data, &mut sm_ctx)
                        .await;

                    if transition.stop {
                        sm_ctx.stop(ExitReason::Normal);
                        self.state = Some(state);
                        return;
                    }

                    if let Some(new_state) = transition.new_state {
                        // State transition occurred
                        self.statem
                            .state_enter(&state, &new_state, data, &mut sm_ctx)
                            .await;
                        self.state = Some(new_state);
                    } else {
                        // Keep current state
                        self.state = Some(state);
                    }
                }
            }
        } else {
            // Put state back if we couldn't process the message
            self.state = Some(state);
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, ctx: &mut ActorContext) {
        if let (Some(state), Some(data)) = (self.state.as_ref(), self.data.as_mut()) {
            let mut sm_ctx = StateMachineContext::new(ctx);
            self.statem
                .terminate(reason, state, data, &mut sm_ctx)
                .await;
        }
    }
}

/// Handle to a spawned GenStatem.
///
/// Provides methods for sending events to the state machine.
pub struct GenStatemRef<G: GenStatem> {
    actor_ref: ActorRef,
    _phantom: PhantomData<G>,
}

impl<G: GenStatem> GenStatemRef<G> {
    /// Get the state machine's Pid
    pub fn pid(&self) -> Pid {
        self.actor_ref.pid()
    }

    /// Send an event and wait for a reply (synchronous call).
    ///
    /// In Erlang: `gen_statem:call/2`
    pub async fn call(&self, event: G::Event) -> Result<G::Reply> {
        let (tx, rx) = oneshot::channel();

        let msg: StatemMsg<G> = StatemMsg::Call {
            event,
            reply_tx: tx,
        };

        self.actor_ref.send(Box::new(msg)).await?;

        rx.await
            .map_err(|_| ActorError::ActorNotFound(self.actor_ref.pid()))
    }

    /// Send an event without waiting for a reply (asynchronous cast).
    ///
    /// In Erlang: `gen_statem:cast/2`
    pub async fn cast(&self, event: G::Event) -> Result<()> {
        let msg: StatemMsg<G> = StatemMsg::Cast { event };
        self.actor_ref.send(Box::new(msg)).await
    }
}

/// Spawn a GenStatem.
///
/// Creates and starts a new state machine actor.
///
/// In Erlang: `gen_statem:start_link/3`
pub fn spawn<G: GenStatem>(system: &Arc<ActorSystem>, statem: G) -> GenStatemRef<G> {
    let actor = GenStatemActor::new(statem);
    let actor_ref = system.spawn(actor);

    GenStatemRef {
        actor_ref,
        _phantom: PhantomData,
    }
}
