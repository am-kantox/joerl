//! Generic server behavior inspired by Erlang's `gen_server`.
//!
//! This module provides a higher-level abstraction for building actors with
//! synchronous call/reply and asynchronous cast semantics, mirroring Erlang/OTP's
//! gen_server behavior.
//!
//! ## Erlang/OTP Comparison
//!
//! In Erlang, gen_server provides a client-server pattern with callbacks:
//!
//! ```erlang
//! -module(counter).
//! -behaviour(gen_server).
//!
//! init([]) -> {ok, 0}.
//!
//! handle_call(get, _From, State) ->
//!     {reply, State, State};
//! handle_call({add, N}, _From, State) ->
//!     {reply, ok, State + N}.
//!
//! handle_cast({increment}, State) ->
//!     {noreply, State + 1}.
//! ```
//!
//! In joerl, the equivalent looks like:
//!
//! ```rust
//! use joerl::gen_server::{GenServer, GenServerContext, CallResponse};
//! use async_trait::async_trait;
//!
//! struct Counter;
//!
//! #[derive(Debug)]
//! enum CounterCall {
//!     Get,
//!     Add(i32),
//! }
//!
//! #[derive(Debug)]
//! enum CounterCast {
//!     Increment,
//! }
//!
//! #[async_trait]
//! impl GenServer for Counter {
//!     type State = i32;
//!     type Call = CounterCall;
//!     type Cast = CounterCast;
//!     type CallReply = i32;
//!
//!     async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
//!         0
//!     }
//!
//!     async fn handle_call(
//!         &mut self,
//!         call: Self::Call,
//!         state: &mut Self::State,
//!         _ctx: &mut GenServerContext<'_, Self>,
//!     ) -> CallResponse<Self::CallReply> {
//!         match call {
//!             CounterCall::Get => CallResponse::Reply(*state),
//!             CounterCall::Add(n) => {
//!                 *state += n;
//!                 CallResponse::Reply(*state)
//!             }
//!         }
//!     }
//!
//!     async fn handle_cast(
//!         &mut self,
//!         cast: Self::Cast,
//!         state: &mut Self::State,
//!         _ctx: &mut GenServerContext<'_, Self>,
//!     ) {
//!         match cast {
//!             CounterCast::Increment => *state += 1,
//!         }
//!     }
//! }
//! ```
//!
//! ## Key Differences from Raw Actors
//!
//! - **Type Safety**: Call and Cast messages are strongly typed
//! - **Synchronous Calls**: `call()` waits for a response from the server
//! - **State Management**: Explicit state type and lifecycle
//! - **Cleaner API**: Separates synchronous calls from asynchronous casts
//!
//! ## Usage
//!
//! ```rust
//! use joerl::{ActorSystem, gen_server};
//!
//! # use async_trait::async_trait;
//! # struct Counter;
//! # #[derive(Debug)]
//! # enum CounterCall { Get }
//! # #[derive(Debug)]
//! # enum CounterCast { Increment }
//! # #[async_trait]
//! # impl gen_server::GenServer for Counter {
//! #     type State = i32;
//! #     type Call = CounterCall;
//! #     type Cast = CounterCast;
//! #     type CallReply = i32;
//! #     async fn init(&mut self, _ctx: &mut gen_server::GenServerContext<'_, Self>) -> Self::State { 0 }
//! #     async fn handle_call(&mut self, call: Self::Call, state: &mut Self::State, _ctx: &mut gen_server::GenServerContext<'_, Self>) -> gen_server::CallResponse<Self::CallReply> { gen_server::CallResponse::Reply(*state) }
//! #     async fn handle_cast(&mut self, _cast: Self::Cast, state: &mut Self::State, _ctx: &mut gen_server::GenServerContext<'_, Self>) { *state += 1; }
//! # }
//! # async fn example() {
//! let system = ActorSystem::new();
//! let counter = gen_server::spawn(&system, Counter);
//!
//! // Synchronous call - waits for response
//! let value = counter.call(CounterCall::Get).await.unwrap();
//!
//! // Asynchronous cast - fire and forget
//! counter.cast(CounterCast::Increment).await.unwrap();
//! # }
//! ```

use crate::{
    actor::{Actor, ActorContext},
    error::{ActorError, Result},
    message::{ExitReason, Message},
    pid::Pid,
    system::ActorSystem,
    telemetry::{GenServerMetrics, actor_type_name},
};
use async_trait::async_trait;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Response from `handle_call` - can be immediate or deferred.
///
/// In Erlang, gen_server's handle_call can return:
/// - `{reply, Reply, State}` - immediate reply
/// - `{noreply, State}` - defer reply, call `gen_server:reply/2` later
///
/// This enum provides the same semantics in Rust.
pub enum CallResponse<T> {
    /// Reply immediately (default behavior).
    ///
    /// The value is sent back to the caller right away.
    Reply(T),

    /// Defer the reply.
    ///
    /// The server can continue processing messages and call `reply()` later.
    /// This is useful for:
    /// - Long-running operations that shouldn't block the mailbox
    /// - Spawning background tasks
    /// - Waiting for external events
    ///
    /// In Erlang: `{noreply, State}` + `gen_server:reply(From, Reply)`
    NoReply,
}

/// Handle for sending deferred replies.
///
/// When `handle_call` returns `CallResponse::NoReply`, a `ReplyHandle` is
/// provided to the callback. This handle can be used to send the reply later,
/// even from a different task or actor.
///
/// In Erlang: the `From` parameter in `gen_server:reply/2`
///
/// # Examples
///
/// ```rust
/// use joerl::gen_server::{GenServer, GenServerContext, CallResponse, ReplyHandle};
/// use async_trait::async_trait;
///
/// struct AsyncWorker;
///
/// #[derive(Debug)]
/// enum WorkRequest {
///     Expensive(String),
/// }
///
/// #[async_trait]
/// impl GenServer for AsyncWorker {
///     type State = ();
///     type Call = WorkRequest;
///     type Cast = ();
///     type CallReply = String;
///
///     async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
///         ()
///     }
///
///     async fn handle_call(
///         &mut self,
///         call: Self::Call,
///         _state: &mut Self::State,
///         ctx: &mut GenServerContext<'_, Self>,
///     ) -> CallResponse<Self::CallReply> {
///         match call {
///             WorkRequest::Expensive(data) => {
///                 // Get the reply handle
///                 let handle = ctx.reply_handle();
///                 
///                 // Spawn background work
///                 tokio::spawn(async move {
///                     let result = expensive_work(data).await;
///                     handle.reply(result).ok();
///                 });
///                 
///                 // Return immediately - mailbox keeps processing
///                 CallResponse::NoReply
///             }
///         }
///     }
///
///     async fn handle_cast(&mut self, _: Self::Cast, _: &mut Self::State, _: &mut GenServerContext<'_, Self>) {}
/// }
///
/// # async fn expensive_work(_data: String) -> String { "result".to_string() }
/// ```
pub struct ReplyHandle<T> {
    reply_tx: Option<oneshot::Sender<T>>,
}

impl<T> ReplyHandle<T> {
    /// Create a new reply handle (internal use).
    pub(crate) fn new(reply_tx: oneshot::Sender<T>) -> Self {
        Self {
            reply_tx: Some(reply_tx),
        }
    }

    /// Send the deferred reply.
    ///
    /// This consumes the handle and sends the reply to the waiting caller.
    /// If the caller has already timed out or cancelled, this returns an error.
    ///
    /// In Erlang: `gen_server:reply(From, Reply)`
    ///
    /// # Errors
    ///
    /// Returns an error if the receiver has been dropped or reply was already sent.
    pub fn reply(mut self, value: T) -> Result<()> {
        if let Some(tx) = self.reply_tx.take() {
            tx.send(value)
                .map_err(|_| ActorError::other("reply failed: receiver dropped"))
        } else {
            Err(ActorError::other("reply already sent"))
        }
    }
}

impl<T> Drop for ReplyHandle<T> {
    fn drop(&mut self) {
        if self.reply_tx.is_some() {
            tracing::warn!("ReplyHandle dropped without calling reply() - caller will timeout");
        }
    }
}

/// The GenServer trait defines callbacks for the generic server behavior.
///
/// This trait is inspired by Erlang's gen_server behavior and provides
/// a structured way to build stateful actors with call/cast semantics.
///
/// ## Type Parameters
///
/// - `State`: The internal state type of the server
/// - `Call`: The type of synchronous call requests
/// - `Cast`: The type of asynchronous cast messages
/// - `CallReply`: The type returned from synchronous calls
#[async_trait]
pub trait GenServer: Send + 'static {
    /// The internal state type
    type State: Send + 'static;

    /// The synchronous call request type
    type Call: Send + Debug + 'static;

    /// The asynchronous cast message type
    type Cast: Send + Debug + 'static;

    /// The reply type for synchronous calls
    type CallReply: Send + 'static;

    /// Initialize the server state.
    ///
    /// Called when the server starts, before processing any messages.
    ///
    /// In Erlang: `init/1`
    async fn init(&mut self, ctx: &mut GenServerContext<'_, Self>) -> Self::State;

    /// Handle synchronous call requests.
    ///
    /// This method is called when `GenServerRef::call()` is invoked.
    /// Return `CallResponse::Reply(value)` for immediate replies, or
    /// `CallResponse::NoReply` to defer the reply.
    ///
    /// When returning `NoReply`, obtain a `ReplyHandle` via
    /// `ctx.reply_handle()` before returning.
    ///
    /// In Erlang: `handle_call/3` returning `{reply, Reply, State}` or `{noreply, State}`
    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply>;

    /// Handle asynchronous cast messages.
    ///
    /// This method is called when `GenServerRef::cast()` is invoked.
    /// No reply is sent to the caller.
    ///
    /// In Erlang: `handle_cast/2`
    async fn handle_cast(
        &mut self,
        cast: Self::Cast,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    );

    /// Handle actor termination.
    ///
    /// Called when the server is about to stop. Use this for cleanup.
    ///
    /// In Erlang: `terminate/2`
    async fn terminate(
        &mut self,
        _reason: &ExitReason,
        _state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        // Default: no-op
    }

    /// Handle generic messages (optional).
    ///
    /// Override this to handle messages that aren't Call or Cast.
    ///
    /// In Erlang: `handle_info/2`
    async fn handle_info(
        &mut self,
        _msg: Message,
        _state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        // Default: ignore unknown messages
    }
}

/// Context provided to GenServer callbacks.
///
/// Similar to Erlang's gen_server state, but provides access to the
/// underlying actor context for advanced operations.
pub struct GenServerContext<'a, G: GenServer + ?Sized> {
    actor_ctx: &'a mut ActorContext,
    reply_handle: Option<ReplyHandle<G::CallReply>>,
    _phantom: PhantomData<G>,
}

impl<'a, G: GenServer> GenServerContext<'a, G> {
    fn new(actor_ctx: &'a mut ActorContext) -> Self {
        Self {
            actor_ctx,
            reply_handle: None,
            _phantom: PhantomData,
        }
    }

    fn set_reply_handle(&mut self, handle: ReplyHandle<G::CallReply>) {
        self.reply_handle = Some(handle);
    }

    /// Get the server's Pid
    pub fn pid(&self) -> Pid {
        self.actor_ctx.pid()
    }

    /// Stop the server with a reason
    pub fn stop(&mut self, reason: ExitReason) {
        self.actor_ctx.stop(reason);
    }

    /// Enable or disable exit signal trapping
    pub fn trap_exit(&mut self, trap: bool) {
        self.actor_ctx.trap_exit(trap);
    }

    /// Get a reply handle for deferred replies.
    ///
    /// This should only be called when returning `CallResponse::NoReply`
    /// from `handle_call`. The handle can be stored and used later to
    /// send the reply.
    ///
    /// # Panics
    ///
    /// Panics if called outside of a `handle_call` context or if the
    /// reply handle has already been taken.
    pub fn reply_handle(&mut self) -> ReplyHandle<G::CallReply> {
        self.reply_handle
            .take()
            .expect("reply_handle() called outside handle_call or already taken")
    }
}

/// Internal message types for GenServer
enum GenServerMsg<G: GenServer> {
    Call {
        request: G::Call,
        reply_tx: oneshot::Sender<G::CallReply>,
    },
    Cast {
        message: G::Cast,
    },
    Info {
        message: Message,
    },
}

/// Internal actor implementation that wraps a GenServer
struct GenServerActor<G: GenServer> {
    server: G,
    state: Option<G::State>,
    server_type: &'static str,
}

impl<G: GenServer> GenServerActor<G> {
    fn new(server: G) -> Self {
        let server_type = actor_type_name::<G>();
        Self {
            server,
            state: None,
            server_type,
        }
    }
}

#[async_trait]
impl<G: GenServer> Actor for GenServerActor<G> {
    async fn started(&mut self, ctx: &mut ActorContext) {
        let mut gen_ctx = GenServerContext::new(ctx);
        let state = self.server.init(&mut gen_ctx).await;
        self.state = Some(state);
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return, // Not initialized yet
        };

        let mut gen_ctx = GenServerContext::new(ctx);

        // Downcast to owned value
        if let Ok(gen_msg) = msg.downcast::<GenServerMsg<G>>() {
            match *gen_msg {
                GenServerMsg::Call { request, reply_tx } => {
                    let _span = GenServerMetrics::call_span(self.server_type);
                    GenServerMetrics::calls_in_flight_inc(self.server_type);

                    // Set up reply handle for potential NoReply response
                    gen_ctx.set_reply_handle(ReplyHandle::new(reply_tx));

                    let response = self.server.handle_call(request, state, &mut gen_ctx).await;

                    match response {
                        CallResponse::Reply(value) => {
                            // Immediate reply - get the handle back and use it
                            if let Some(handle) = gen_ctx.reply_handle.take() {
                                let _ = handle.reply(value); // Ignore send errors
                            }
                        }
                        CallResponse::NoReply => {
                            // Deferred reply - handle was taken by user code via reply_handle()
                            // or will be dropped with a warning
                        }
                    }

                    GenServerMetrics::calls_in_flight_dec(self.server_type);
                }
                GenServerMsg::Cast { message } => {
                    GenServerMetrics::cast(self.server_type);
                    self.server.handle_cast(message, state, &mut gen_ctx).await;
                }
                GenServerMsg::Info { message } => {
                    self.server.handle_info(message, state, &mut gen_ctx).await;
                }
            }
        }
    }

    async fn stopped(&mut self, reason: &ExitReason, ctx: &mut ActorContext) {
        if let Some(state) = self.state.as_mut() {
            let mut gen_ctx = GenServerContext::new(ctx);
            self.server.terminate(reason, state, &mut gen_ctx).await;
        }
    }
}

/// Handle to a spawned GenServer.
///
/// Provides `call()` and `cast()` methods for interacting with the server.
pub struct GenServerRef<G: GenServer> {
    pid: Pid,
    system: Arc<ActorSystem>,
    _phantom: PhantomData<G>,
}

impl<G: GenServer> Clone for GenServerRef<G> {
    fn clone(&self) -> Self {
        Self {
            pid: self.pid,
            system: self.system.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<G: GenServer> std::fmt::Debug for GenServerRef<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenServerRef")
            .field("pid", &self.pid)
            .finish()
    }
}

impl<G: GenServer> GenServerRef<G> {
    /// Get the server's Pid
    pub fn pid(&self) -> Pid {
        self.pid
    }

    /// Make a synchronous call to the server.
    ///
    /// This sends a request and waits for a response.
    ///
    /// In Erlang: `gen_server:call/2`
    pub async fn call(&self, request: G::Call) -> Result<G::CallReply> {
        let (tx, rx) = oneshot::channel();

        let msg: GenServerMsg<G> = GenServerMsg::Call {
            request,
            reply_tx: tx,
        };

        self.system.send(self.pid, Box::new(msg)).await?;

        rx.await.map_err(|_| ActorError::ActorNotFound(self.pid))
    }

    /// Send an asynchronous cast to the server.
    ///
    /// This sends a message without waiting for a response.
    ///
    /// In Erlang: `gen_server:cast/2`
    pub async fn cast(&self, message: G::Cast) -> Result<()> {
        let msg: GenServerMsg<G> = GenServerMsg::Cast { message };
        self.system.send(self.pid, Box::new(msg)).await
    }

    /// Send a generic info message to the server.
    ///
    /// In Erlang: `Pid ! Message`
    pub async fn send_info(&self, message: Message) -> Result<()> {
        let msg: GenServerMsg<G> = GenServerMsg::Info { message };
        self.system.send(self.pid, Box::new(msg)).await
    }
}

/// Spawn a GenServer.
///
/// Creates and starts a new GenServer actor.
///
/// In Erlang: `gen_server:start_link/3`
pub fn spawn<G: GenServer>(system: &Arc<ActorSystem>, server: G) -> GenServerRef<G> {
    let actor = GenServerActor::new(server);
    let actor_ref = system.spawn(actor);

    GenServerRef {
        pid: actor_ref.pid(),
        system: system.clone(),
        _phantom: PhantomData,
    }
}

/// Spawn a GenServer and register it with a name.
///
/// Creates and starts a new GenServer actor, then registers it in the
/// system registry with the given name. The server can then be looked up
/// using `ActorSystem::whereis()`.
///
/// Returns an error if the name is already registered.
///
/// In Erlang: `gen_server:start_link({local, Name}, Module, Args, Opts)`
///
/// # Examples
///
/// ```rust
/// use joerl::gen_server::{spawn_named, GenServer, GenServerContext, CallResponse};
/// use joerl::ActorSystem;
/// use async_trait::async_trait;
/// use std::sync::Arc;
///
/// struct Counter;
///
/// #[derive(Debug)]
/// enum CounterCall { Get }
///
/// #[derive(Debug)]
/// enum CounterCast { Increment }
///
/// #[async_trait]
/// impl GenServer for Counter {
///     type State = i32;
///     type Call = CounterCall;
///     type Cast = CounterCast;
///     type CallReply = i32;
///
///     async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
///         0
///     }
///
///     async fn handle_call(
///         &mut self,
///         call: Self::Call,
///         state: &mut Self::State,
///         _ctx: &mut GenServerContext<'_, Self>,
///     ) -> CallResponse<Self::CallReply> {
///         CallResponse::Reply(*state)
///     }
///
///     async fn handle_cast(
///         &mut self,
///         _cast: Self::Cast,
///         state: &mut Self::State,
///         _ctx: &mut GenServerContext<'_, Self>,
///     ) {
///         *state += 1;
///     }
/// }
///
/// # tokio_test::block_on(async {
/// let system = Arc::new(ActorSystem::new());
/// let counter = spawn_named(&system, Counter, "counter").unwrap();
///
/// // Look up by name
/// let pid = system.whereis("counter").unwrap();
/// assert_eq!(pid, counter.pid());
/// # });
/// ```
pub fn spawn_named<G: GenServer>(
    system: &Arc<ActorSystem>,
    server: G,
    name: impl Into<String>,
) -> Result<GenServerRef<G>> {
    let actor = GenServerActor::new(server);
    let actor_ref = system.spawn(actor);
    let pid = actor_ref.pid();

    // Register the name
    system.register(name, pid)?;

    Ok(GenServerRef {
        pid,
        system: system.clone(),
        _phantom: PhantomData,
    })
}
