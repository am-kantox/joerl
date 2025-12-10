# Design: `:noreply` with Deferred `reply/2` and Selective `receive`

## Overview

This document proposes two Erlang-like features for joerl:

1. **Deferred reply in `handle_call`**: Allow returning `:noreply` and calling `reply/2` later
2. **Selective receive**: Allow actors to selectively receive messages from mailbox

Both features maintain the actor model guarantees while providing more control.

---

## Feature 1: Deferred Reply (`:noreply` + `reply/2`)

### Erlang Semantics

In Erlang's gen_server, you can defer replies:

```erlang
handle_call(request, From, State) ->
    spawn(fun() -> 
        Result = expensive_computation(),
        gen_server:reply(From, Result)
    end),
    {noreply, State}.
```

This allows:
- Processing the mailbox while computation happens
- Responding out of order
- Spawning background work without blocking

### Proposed Rust API

```rust
#[async_trait]
pub trait GenServer: Send + 'static {
    // ... existing types ...
    
    /// New return type for handle_call
    type CallReply: Send + 'static;
    
    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply>;
}

/// Response from handle_call
pub enum CallResponse<T> {
    /// Reply immediately (current behavior)
    Reply(T),
    
    /// Defer reply - will call reply() later
    NoReply(ReplyHandle<T>),
}

/// Handle for deferred replies
pub struct ReplyHandle<T> {
    reply_tx: oneshot::Sender<T>,
}

impl<T> ReplyHandle<T> {
    /// Send the deferred reply
    pub fn reply(self, value: T) -> Result<()> {
        self.reply_tx.send(value)
            .map_err(|_| ActorError::ReplyFailed)
    }
}
```

### Usage Example

```rust
struct AsyncWorker;

#[async_trait]
impl GenServer for AsyncWorker {
    type State = ();
    type Call = WorkRequest;
    type Cast = ();
    type CallReply = WorkResult;
    
    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match call {
            WorkRequest::Expensive(data) => {
                // Get reply handle
                let reply_handle = ReplyHandle::new();
                let handle_clone = reply_handle.clone_sender();
                
                // Spawn async work
                tokio::spawn(async move {
                    let result = expensive_work(data).await;
                    handle_clone.reply(result).ok();
                });
                
                // Return immediately - mailbox keeps processing
                CallResponse::NoReply(reply_handle)
            }
            WorkRequest::Fast => {
                // Immediate reply still works
                CallResponse::Reply(WorkResult::Done)
            }
        }
    }
}
```

### Implementation Details

```rust
// In gen_server.rs

enum GenServerMsg<G: GenServer> {
    Call {
        request: G::Call,
        reply_tx: oneshot::Sender<G::CallReply>,
    },
    Cast { message: G::Cast },
    Info { message: Message },
}

// Modified handle_message
async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
    // ... existing code ...
    
    GenServerMsg::Call { request, reply_tx } => {
        let response = self.server.handle_call(request, state, &mut gen_ctx).await;
        
        match response {
            CallResponse::Reply(value) => {
                // Immediate reply (current behavior)
                let _ = reply_tx.send(value);
            }
            CallResponse::NoReply(handle) => {
                // Store reply_tx in handle, will be called later
                handle.set_sender(reply_tx);
                // Mailbox continues processing!
            }
        }
    }
}
```

### Benefits

1. **Non-blocking calls**: Long operations don't block mailbox
2. **Parallel work**: Can process multiple calls concurrently
3. **Erlang parity**: Matches gen_server semantics exactly
4. **Backward compatible**: Existing code using `Reply` still works

---

## Feature 2: Selective Receive

### Erlang Semantics

In Erlang, `receive` with pattern matching allows selective message processing:

```erlang
% Wait for specific message
receive
    {response, Ref, Value} when Ref =:= MyRef ->
        Value
after 5000 ->
    timeout
end
```

Messages that don't match stay in the mailbox for later.

### Proposed Rust API

```rust
impl ActorContext {
    /// Selectively receive a message matching the predicate.
    ///
    /// Messages that don't match remain in the mailbox and will be
    /// checked again on the next receive call.
    ///
    /// Similar to Erlang's `receive` with pattern matching.
    pub async fn receive<F, T>(&mut self, predicate: F) -> Option<T>
    where
        F: FnMut(&Message) -> Option<T>,
    {
        self.selective_receive(predicate, None).await
    }
    
    /// Selective receive with timeout.
    ///
    /// Returns None if timeout expires before a matching message arrives.
    pub async fn receive_timeout<F, T>(
        &mut self, 
        predicate: F,
        timeout: Duration
    ) -> Option<T>
    where
        F: FnMut(&Message) -> Option<T>,
    {
        self.selective_receive(predicate, Some(timeout)).await
    }
    
    /// Try to receive a matching message without blocking.
    pub fn try_receive<F, T>(&mut self, predicate: F) -> Option<T>
    where
        F: FnMut(&Message) -> Option<T>,
    {
        // Check current mailbox without waiting
        self.mailbox.try_recv_matching(predicate)
    }
}
```

### Implementation Strategy

The challenge: tokio's mpsc doesn't support selective receive natively.

**Option 1: Message Queue Buffer (Recommended)**

```rust
pub struct Mailbox {
    rx: mpsc::Receiver<Envelope>,
    // Buffer for messages that didn't match
    pending: VecDeque<Envelope>,
}

impl Mailbox {
    pub(crate) async fn recv_matching<F, T>(
        &mut self, 
        mut predicate: F,
        timeout: Option<Duration>
    ) -> Option<T>
    where
        F: FnMut(&Message) -> Option<T>,
    {
        // First check pending messages
        for (idx, envelope) in self.pending.iter().enumerate() {
            if let Envelope::Message(msg) = envelope {
                if let Some(result) = predicate(msg) {
                    self.pending.remove(idx);
                    return Some(result);
                }
            }
        }
        
        // Then check incoming messages
        let deadline = timeout.map(|d| tokio::time::Instant::now() + d);
        
        loop {
            let envelope = if let Some(deadline) = deadline {
                tokio::time::timeout_at(deadline, self.rx.recv())
                    .await
                    .ok()
                    .flatten()?
            } else {
                self.rx.recv().await?
            };
            
            match envelope {
                Envelope::Message(msg) => {
                    if let Some(result) = predicate(&msg) {
                        return Some(result);
                    } else {
                        // Doesn't match - save for later
                        self.pending.push_back(Envelope::Message(msg));
                    }
                }
                Envelope::Signal(sig) => {
                    // Signals always processed immediately
                    self.pending.push_back(Envelope::Signal(sig));
                }
            }
        }
    }
    
    /// Regular recv checks pending first
    pub(crate) async fn recv(&mut self) -> Option<Envelope> {
        if let Some(envelope) = self.pending.pop_front() {
            return Some(envelope);
        }
        self.rx.recv().await
    }
}
```

**Option 2: Channel Wrapper with Peek (Alternative)**

More complex, but could avoid buffering overhead for common case.

### Usage Example

```rust
struct RpcActor {
    next_ref: u64,
}

#[derive(Clone)]
struct RpcRequest {
    ref_id: u64,
    data: String,
}

#[derive(Clone)]
struct RpcResponse {
    ref_id: u64,
    result: String,
}

#[async_trait]
impl Actor for RpcActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(req) = msg.downcast_ref::<RpcRequest>() {
            // Make request
            let ref_id = self.next_ref;
            self.next_ref += 1;
            
            // Send to remote service
            remote_service.send(req.clone()).await;
            
            // Wait for specific response
            let response = ctx.receive(|msg| {
                msg.downcast_ref::<RpcResponse>()
                    .filter(|r| r.ref_id == ref_id)
                    .cloned()
            }).await;
            
            // Meanwhile, other messages are still in mailbox
            // Will be processed after this completes
        }
    }
}
```

### Advanced: Receive in User Code

Allow users to manually drive the message loop:

```rust
#[async_trait]
impl Actor for CustomActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        // Process first message normally
        self.handle_msg(msg);
        
        // Now do custom receive loop
        loop {
            let msg = ctx.receive_timeout(|m| {
                m.downcast_ref::<SpecialMessage>().cloned()
            }, Duration::from_secs(5)).await;
            
            match msg {
                Some(special) => {
                    // Got special message
                    if special.is_done() {
                        break;
                    }
                }
                None => {
                    // Timeout
                    break;
                }
            }
        }
        
        // Continue processing mailbox normally
    }
}
```

### Important Considerations

1. **Performance**: Pending queue adds overhead. Most actors won't use this.
2. **Ordering**: Messages stay ordered within mailbox. Selective receive doesn't violate this.
3. **Signals**: Should signals be selectively receivable? Probably not - always process immediately.
4. **Memory**: Pending queue is bounded by mailbox capacity.

### Benefits

1. **Request-response patterns**: Clean RPC implementation
2. **Synchronization**: Wait for specific acknowledgments
3. **Erlang parity**: Matches `receive` semantics
4. **Flexibility**: Complex protocols become easier

---

## Combined Example: RPC with Deferred Reply

```rust
struct RpcServer;

#[async_trait]
impl GenServer for RpcServer {
    type State = HashMap<u64, ReplyHandle<RpcResult>>;
    type Call = RpcCall;
    type Cast = RpcResponse;
    type CallReply = RpcResult;
    
    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        let ref_id = rand::random();
        
        // Send to external service
        external_service.send_request(ref_id, call).await;
        
        // Create reply handle
        let handle = ReplyHandle::new();
        state.insert(ref_id, handle.clone());
        
        // Return noreply - will respond when we get response
        CallResponse::NoReply(handle)
    }
    
    async fn handle_cast(
        &mut self,
        cast: Self::Cast, // RpcResponse
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) {
        // Got response from external service
        if let Some(handle) = state.remove(&cast.ref_id) {
            handle.reply(cast.result).ok();
        }
    }
}
```

---

## Implementation Plan

### Phase 1: Deferred Reply (Simpler)

1. Add `CallResponse<T>` enum
2. Add `ReplyHandle<T>` struct
3. Modify `GenServerActor::handle_message` to check response type
4. Add tests for deferred reply
5. Update documentation

**Estimated effort**: 2-3 days

### Phase 2: Selective Receive (More Complex)

1. Add `pending: VecDeque<Envelope>` to `Mailbox`
2. Implement `recv_matching` with timeout
3. Modify `recv()` to check pending first
4. Add `ActorContext::receive` methods
5. Add comprehensive tests
6. Document performance implications

**Estimated effort**: 4-5 days

### Phase 3: Polish & Examples

1. Add combined examples
2. Property tests for selective receive ordering
3. Performance benchmarks
4. Update GETTING_STARTED guides

**Estimated effort**: 2-3 days

---

## Questions for Discussion

1. **API naming**: `receive` vs `select` vs `recv_matching`?
2. **Signals**: Should selective receive work on signals too?
3. **Feature flag**: Should selective receive be behind a feature flag?
4. **Default behavior**: Should we make `CallResponse::Reply` the default return type somehow?

---

## Conclusion

Both features are implementable and maintain the actor model guarantees:

- ✅ Sequential message processing per actor (preserved)
- ✅ No shared state between actors (preserved)
- ✅ Message ordering in mailbox (preserved)
- ✅ Erlang/OTP semantics (achieved)

The main tradeoff is complexity vs. flexibility. The pending queue for selective receive adds some overhead, but only when used.
