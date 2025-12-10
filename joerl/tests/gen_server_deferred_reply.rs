//! Tests for GenServer deferred reply feature (NoReply + ReplyHandle).

use async_trait::async_trait;
use joerl::{
    ActorSystem,
    gen_server::{self, CallResponse, GenServer, GenServerContext, ReplyHandle},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Counter that supports immediate and deferred replies
struct FlexibleCounter;

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
enum CounterCall {
    GetImmediate,
    GetDeferred,
    GetDeferredSlow,
}

#[derive(Debug)]
enum CounterCast {
    Increment,
}

#[async_trait]
impl GenServer for FlexibleCounter {
    type State = i32;
    type Call = CounterCall;
    type Cast = CounterCast;
    type CallReply = i32;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        0
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match call {
            CounterCall::GetImmediate => {
                // Immediate reply
                CallResponse::Reply(*state)
            }
            CounterCall::GetDeferred => {
                // Deferred reply - return immediately
                let handle = ctx.reply_handle();
                let value = *state;

                tokio::spawn(async move {
                    // Simulate some work
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    handle.reply(value).expect("reply failed");
                });

                CallResponse::NoReply
            }
            CounterCall::GetDeferredSlow => {
                // Deferred reply with longer delay
                let handle = ctx.reply_handle();
                let value = *state;

                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    handle.reply(value).expect("reply failed");
                });

                CallResponse::NoReply
            }
        }
    }

    async fn handle_cast(
        &mut self,
        cast: Self::Cast,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        match cast {
            CounterCast::Increment => *state += 1,
        }
    }
}

#[tokio::test]
async fn test_immediate_reply() {
    let system = Arc::new(ActorSystem::new());
    let counter = gen_server::spawn(&system, FlexibleCounter);

    // Cast to increment
    counter.cast(CounterCast::Increment).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Immediate reply
    let value = counter.call(CounterCall::GetImmediate).await.unwrap();
    assert_eq!(value, 1);
}

#[tokio::test]
async fn test_deferred_reply() {
    let system = Arc::new(ActorSystem::new());
    let counter = gen_server::spawn(&system, FlexibleCounter);

    // Cast to increment
    counter.cast(CounterCast::Increment).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Deferred reply - should still work
    let value = counter.call(CounterCall::GetDeferred).await.unwrap();
    assert_eq!(value, 1);
}

#[tokio::test]
async fn test_deferred_reply_allows_mailbox_processing() {
    let system = Arc::new(ActorSystem::new());
    let counter = gen_server::spawn(&system, FlexibleCounter);

    // Make a slow deferred call
    let slow_call = tokio::spawn({
        let counter = counter.clone();
        async move { counter.call(CounterCall::GetDeferredSlow).await }
    });

    // Give it time to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    // While the slow call is pending, we should be able to cast
    counter.cast(CounterCast::Increment).await.unwrap();
    counter.cast(CounterCast::Increment).await.unwrap();
    counter.cast(CounterCast::Increment).await.unwrap();

    // Wait a bit for casts to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Make an immediate call to verify state
    let value = counter.call(CounterCall::GetImmediate).await.unwrap();
    assert_eq!(value, 3, "mailbox should have processed casts");

    // The slow call should eventually complete with the original value (0)
    let slow_value = slow_call.await.unwrap().unwrap();
    assert_eq!(slow_value, 0, "deferred reply should return original value");
}

#[tokio::test]
async fn test_multiple_concurrent_deferred_replies() {
    let system = Arc::new(ActorSystem::new());
    let counter = gen_server::spawn(&system, FlexibleCounter);

    // Increment first
    counter.cast(CounterCast::Increment).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Make multiple concurrent deferred calls
    let mut handles = vec![];
    for _ in 0..5 {
        let counter_clone = counter.clone();
        let handle =
            tokio::spawn(
                async move { counter_clone.call(CounterCall::GetDeferred).await.unwrap() },
            );
        handles.push(handle);
    }

    // All should eventually return 1
    for handle in handles {
        let value = handle.await.unwrap();
        assert_eq!(value, 1);
    }
}

/// Server that can be used to test reply handle behavior
struct ControlledReplyServer {
    reply_handles: Arc<Mutex<Vec<ReplyHandle<i32>>>>,
}

#[derive(Debug)]
enum ControlledCall {
    DeferReply,
}

#[derive(Debug)]
enum ControlledCast {
    ReplyToAll(i32),
}

#[async_trait]
impl GenServer for ControlledReplyServer {
    type State = Arc<Mutex<Vec<ReplyHandle<i32>>>>;
    type Call = ControlledCall;
    type Cast = ControlledCast;
    type CallReply = i32;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        self.reply_handles.clone()
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match call {
            ControlledCall::DeferReply => {
                let handle = ctx.reply_handle();
                state.lock().await.push(handle);
                CallResponse::NoReply
            }
        }
    }

    async fn handle_cast(
        &mut self,
        cast: Self::Cast,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        match cast {
            ControlledCast::ReplyToAll(value) => {
                let mut handles = state.lock().await;
                for handle in handles.drain(..) {
                    handle.reply(value).ok();
                }
            }
        }
    }
}

#[tokio::test]
async fn test_manual_reply_control() {
    let system = Arc::new(ActorSystem::new());
    let reply_handles = Arc::new(Mutex::new(Vec::new()));
    let server = gen_server::spawn(
        &system,
        ControlledReplyServer {
            reply_handles: reply_handles.clone(),
        },
    );

    // Make 3 deferred calls
    let call1 = tokio::spawn({
        let server = server.clone();
        async move { server.call(ControlledCall::DeferReply).await }
    });

    let call2 = tokio::spawn({
        let server = server.clone();
        async move { server.call(ControlledCall::DeferReply).await }
    });

    let call3 = tokio::spawn({
        let server = server.clone();
        async move { server.call(ControlledCall::DeferReply).await }
    });

    // Wait for calls to be registered
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Verify handles were stored
    assert_eq!(reply_handles.lock().await.len(), 3);

    // Reply to all with value 42
    server.cast(ControlledCast::ReplyToAll(42)).await.unwrap();

    // All calls should complete with 42
    assert_eq!(call1.await.unwrap().unwrap(), 42);
    assert_eq!(call2.await.unwrap().unwrap(), 42);
    assert_eq!(call3.await.unwrap().unwrap(), 42);

    // Handles should be consumed
    assert_eq!(reply_handles.lock().await.len(), 0);
}

/// Server that tests error handling
struct ErrorTestServer;

#[derive(Debug)]
enum ErrorCall {
    TakeTwice,
}

#[async_trait]
impl GenServer for ErrorTestServer {
    type State = ();
    type Call = ErrorCall;
    type Cast = ();
    type CallReply = String;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {}

    async fn handle_call(
        &mut self,
        call: Self::Call,
        _state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match call {
            ErrorCall::TakeTwice => {
                let handle1 = ctx.reply_handle();

                // This should panic because we already took the handle
                // In a real scenario, this would be caught by the panic handler
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = ctx.reply_handle(); // Should panic
                }))
                .expect_err("should panic on second take");

                handle1.reply("success".to_string()).ok();
                CallResponse::NoReply
            }
        }
    }

    async fn handle_cast(
        &mut self,
        _: Self::Cast,
        _: &mut Self::State,
        _: &mut GenServerContext<'_, Self>,
    ) {
    }
}

#[tokio::test]
async fn test_reply_handle_can_only_be_taken_once() {
    let system = Arc::new(ActorSystem::new());
    let server = gen_server::spawn(&system, ErrorTestServer);

    // This should work - the panic is caught internally
    let result = server.call(ErrorCall::TakeTwice).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
}
