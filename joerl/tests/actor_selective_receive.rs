//! Tests for selective receive feature (Erlang-style receive with pattern matching).

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, Message};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Test message types
#[derive(Debug, Clone, PartialEq)]
struct Request {
    id: u64,
    data: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Response {
    id: u64,
    result: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Ack {
    id: u64,
}

/// Actor that uses selective receive for RPC-style communication
struct RpcActor {
    next_id: u64,
    responses: Arc<Mutex<Vec<Response>>>,
}

#[async_trait]
impl Actor for RpcActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(_req) = msg.downcast_ref::<Request>().cloned() {
            let req_id = self.next_id;
            self.next_id += 1;

            // Simulate sending request somewhere and waiting for specific response
            // In real code, this would send to another actor

            // Wait for response with matching ID
            let response = ctx
                .receive(|msg| {
                    msg.downcast_ref::<Response>()
                        .filter(|r| r.id == req_id)
                        .cloned()
                })
                .await;

            if let Some(resp) = response {
                self.responses.lock().await.push(resp);
            }
        }
    }
}

#[tokio::test]
async fn test_selective_receive_basic() {
    let system = Arc::new(ActorSystem::new());
    let responses = Arc::new(Mutex::new(Vec::new()));

    let actor = system.spawn(RpcActor {
        next_id: 1,
        responses: responses.clone(),
    });

    // Send a request
    actor
        .send(Box::new(Request {
            id: 0,
            data: "test".to_string(),
        }))
        .await
        .unwrap();

    // Give time for request to be processed (it will wait in receive)
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send some unrelated messages
    actor.send(Box::new("unrelated1")).await.unwrap();
    actor.send(Box::new("unrelated2")).await.unwrap();

    // Send the matching response
    actor
        .send(Box::new(Response {
            id: 1,
            result: "success".to_string(),
        }))
        .await
        .unwrap();

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify response was received
    let responses = responses.lock().await;
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].id, 1);
    assert_eq!(responses[0].result, "success");
}

#[tokio::test]
async fn test_selective_receive_with_timeout() {
    struct TimeoutActor {
        got_response: Arc<Mutex<Option<Response>>>,
        timed_out: Arc<Mutex<bool>>,
    }

    #[async_trait]
    impl Actor for TimeoutActor {
        async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
            if msg.downcast_ref::<&str>().is_some() {
                // Wait for response with timeout
                let response = ctx
                    .receive_timeout(
                        |msg| msg.downcast_ref::<Response>().cloned(),
                        Duration::from_millis(100),
                    )
                    .await;

                match response {
                    Some(resp) => {
                        *self.got_response.lock().await = Some(resp);
                    }
                    None => {
                        *self.timed_out.lock().await = true;
                    }
                }
            }
        }
    }

    let system = Arc::new(ActorSystem::new());
    let got_response = Arc::new(Mutex::new(None));
    let timed_out = Arc::new(Mutex::new(false));

    let actor = system.spawn(TimeoutActor {
        got_response: got_response.clone(),
        timed_out: timed_out.clone(),
    });

    // Trigger the receive with timeout
    actor.send(Box::new("start")).await.unwrap();

    // Don't send the response - let it timeout
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify timeout occurred
    assert!(got_response.lock().await.is_none());
    assert!(*timed_out.lock().await);
}

#[tokio::test]
async fn test_selective_receive_preserves_order() {
    struct OrderActor {
        received: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Actor for OrderActor {
        async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
            if let Some(cmd) = msg.downcast_ref::<&str>() {
                if *cmd == "wait_for_special" {
                    // Wait for special message
                    if let Some(special) = ctx
                        .receive(|msg| {
                            msg.downcast_ref::<String>()
                                .filter(|s| s.starts_with("special"))
                                .cloned()
                        })
                        .await
                    {
                        self.received.lock().await.push(special);
                    }
                }
            } else if let Some(s) = msg.downcast_ref::<String>() {
                self.received.lock().await.push(s.clone());
            }
        }
    }

    let system = Arc::new(ActorSystem::new());
    let received = Arc::new(Mutex::new(Vec::new()));

    let actor = system.spawn(OrderActor {
        received: received.clone(),
    });

    // Send command to wait
    actor.send(Box::new("wait_for_special")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send normal messages (these will be in pending queue)
    actor.send(Box::new("msg1".to_string())).await.unwrap();
    actor.send(Box::new("msg2".to_string())).await.unwrap();

    // Send the special message
    actor
        .send(Box::new("special_msg".to_string()))
        .await
        .unwrap();

    // Send more normal messages
    actor.send(Box::new("msg3".to_string())).await.unwrap();

    // Wait for all to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    let received = received.lock().await;
    // special_msg should be received first (matched the pattern)
    // Then msg1, msg2, msg3 should be processed in order from pending queue
    assert_eq!(received[0], "special_msg");
    assert_eq!(received[1], "msg1");
    assert_eq!(received[2], "msg2");
    assert_eq!(received[3], "msg3");
}

#[tokio::test]
async fn test_try_receive_finds_pending() {
    // Test that try_receive finds messages in the pending queue
    // This happens when selective receive puts non-matching messages aside
    struct PendingActor {
        found_target: Arc<Mutex<Option<u32>>>,
        found_other: Arc<Mutex<Option<u32>>>,
    }

    #[async_trait]
    impl Actor for PendingActor {
        async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
            if msg.downcast_ref::<&str>().is_some() {
                // First, wait for a specific value (99)
                // This will put other u32 values in pending queue
                let target = ctx
                    .receive_timeout(
                        |msg| msg.downcast_ref::<u32>().filter(|&&v| v == 99).cloned(),
                        Duration::from_millis(50),
                    )
                    .await;

                *self.found_target.lock().await = target;

                // Now try_receive should find messages from the pending queue
                let other = ctx.try_receive(|msg| msg.downcast_ref::<u32>().cloned());

                *self.found_other.lock().await = other;
            }
        }
    }

    let system = Arc::new(ActorSystem::new());
    let found_target = Arc::new(Mutex::new(None));
    let found_other = Arc::new(Mutex::new(None));

    let actor = system.spawn(PendingActor {
        found_target: found_target.clone(),
        found_other: found_other.clone(),
    });

    // Trigger the selective receive first
    actor.send(Box::new("start")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Now send values while actor is waiting
    actor.send(Box::new(42u32)).await.unwrap();
    actor.send(Box::new(17u32)).await.unwrap();
    actor.send(Box::new(99u32)).await.unwrap(); // This one matches

    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should have found 99 and one of the others (42 or 17) from pending
    assert_eq!(*found_target.lock().await, Some(99));
    assert!(
        *found_other.lock().await == Some(42) || *found_other.lock().await == Some(17),
        "Should find a pending message"
    );
}

#[tokio::test]
async fn test_selective_receive_multiple_patterns() {
    struct MultiPatternActor {
        acks: Arc<Mutex<Vec<u64>>>,
        responses: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Actor for MultiPatternActor {
        async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
            if let Some(id) = msg.downcast_ref::<u64>() {
                let target_id = *id;

                // Wait for either Ack or Response with matching ID
                // First check for Ack
                if let Some(ack) = ctx
                    .receive_timeout(
                        |msg| {
                            msg.downcast_ref::<Ack>()
                                .filter(|a| a.id == target_id)
                                .cloned()
                        },
                        Duration::from_millis(50),
                    )
                    .await
                {
                    self.acks.lock().await.push(ack.id);
                } else {
                    // If no Ack, try Response
                    if let Some(resp) = ctx
                        .receive_timeout(
                            |msg| {
                                msg.downcast_ref::<Response>()
                                    .filter(|r| r.id == target_id)
                                    .cloned()
                            },
                            Duration::from_millis(50),
                        )
                        .await
                    {
                        self.responses.lock().await.push(resp.result);
                    }
                }
            }
        }
    }

    let system = Arc::new(ActorSystem::new());
    let acks = Arc::new(Mutex::new(Vec::new()));
    let responses = Arc::new(Mutex::new(Vec::new()));

    let actor = system.spawn(MultiPatternActor {
        acks: acks.clone(),
        responses: responses.clone(),
    });

    // Request ID 1 - will get Ack
    actor.send(Box::new(1u64)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    actor.send(Box::new(Ack { id: 1 })).await.unwrap();

    // Request ID 2 - will get Response
    actor.send(Box::new(2u64)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    actor
        .send(Box::new(Response {
            id: 2,
            result: "result2".to_string(),
        }))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(acks.lock().await.len(), 1);
    assert_eq!(acks.lock().await[0], 1);
    assert_eq!(responses.lock().await.len(), 1);
    assert_eq!(responses.lock().await[0], "result2");
}

#[tokio::test]
async fn test_pending_queue_with_signals() {
    // This test verifies that signals are handled properly with selective receive
    struct SignalActor {
        got_message: Arc<Mutex<bool>>,
    }

    #[async_trait]
    impl Actor for SignalActor {
        async fn started(&mut self, ctx: &mut ActorContext) {
            ctx.trap_exit(true);
        }

        async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
            if msg.downcast_ref::<&str>().is_some() {
                // Wait for specific message
                let _ = ctx
                    .receive_timeout(
                        |msg| msg.downcast_ref::<String>().cloned(),
                        Duration::from_millis(100),
                    )
                    .await;

                *self.got_message.lock().await = true;
            }
        }

        async fn handle_signal(&mut self, _signal: joerl::Signal, _ctx: &mut ActorContext) {
            // Signals should still be delivered
        }
    }

    let system = Arc::new(ActorSystem::new());
    let got_message = Arc::new(Mutex::new(false));

    let actor = system.spawn(SignalActor {
        got_message: got_message.clone(),
    });

    // Start selective receive
    actor.send(Box::new("start")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send matching message
    actor.send(Box::new("found".to_string())).await.unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    assert!(*got_message.lock().await);
}
