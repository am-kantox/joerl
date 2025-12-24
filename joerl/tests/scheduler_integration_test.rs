//! Integration tests for the message scheduler.

use async_trait::async_trait;
use joerl::scheduler::Destination;
use joerl::{Actor, ActorContext, ActorSystem, Message};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant, sleep};

struct CountingActor {
    counter: Arc<Mutex<usize>>,
}

#[async_trait]
impl Actor for CountingActor {
    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if msg.downcast_ref::<String>().is_some() {
            *self.counter.lock().unwrap() += 1;
        }
    }
}

#[tokio::test]
async fn test_send_after_basic() {
    let system = ActorSystem::new();
    let counter = Arc::new(Mutex::new(0));
    let actor = system.spawn(CountingActor {
        counter: Arc::clone(&counter),
    });

    // Schedule a message
    system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("test".to_string()),
        Duration::from_millis(100),
    );

    // Should not have received it yet
    assert_eq!(*counter.lock().unwrap(), 0);

    // Wait for delivery
    sleep(Duration::from_millis(200)).await;
    assert_eq!(*counter.lock().unwrap(), 1);
}

#[tokio::test]
async fn test_timer_cancellation() {
    let system = ActorSystem::new();
    let counter = Arc::new(Mutex::new(0));
    let actor = system.spawn(CountingActor {
        counter: Arc::clone(&counter),
    });

    // Schedule a message
    let timer_ref = system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("test".to_string()),
        Duration::from_secs(10),
    );

    // Cancel it immediately
    let cancelled = system.cancel_timer(timer_ref).unwrap();
    assert!(cancelled);

    // Wait to make sure it doesn't arrive
    sleep(Duration::from_millis(200)).await;
    assert_eq!(*counter.lock().unwrap(), 0);
}

#[tokio::test]
async fn test_cancel_already_fired_timer() {
    let system = ActorSystem::new();
    let counter = Arc::new(Mutex::new(0));
    let actor = system.spawn(CountingActor {
        counter: Arc::clone(&counter),
    });

    // Schedule a very short timer
    let timer_ref = system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("test".to_string()),
        Duration::from_millis(10),
    );

    // Wait for it to fire
    sleep(Duration::from_millis(100)).await;
    assert_eq!(*counter.lock().unwrap(), 1);

    // Try to cancel after it fired
    let cancelled = system.cancel_timer(timer_ref).unwrap();
    assert!(!cancelled);
}

#[tokio::test]
async fn test_scheduled_message_timing() {
    let system = ActorSystem::new();
    let counter = Arc::new(Mutex::new(0));
    let actor = system.spawn(CountingActor {
        counter: Arc::clone(&counter),
    });

    let start = Instant::now();
    let delay = Duration::from_millis(200);

    system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("test".to_string()),
        delay,
    );

    // Wait for message
    sleep(Duration::from_millis(300)).await;

    let elapsed = start.elapsed();
    assert_eq!(*counter.lock().unwrap(), 1);
    // Should have taken at least the delay time
    assert!(elapsed >= delay);
}

#[tokio::test]
async fn test_multiple_scheduled_messages() {
    let system = ActorSystem::new();
    let counter = Arc::new(Mutex::new(0));
    let actor = system.spawn(CountingActor {
        counter: Arc::clone(&counter),
    });

    // Schedule multiple messages with different delays
    for i in 1..=5 {
        system.send_after(
            Destination::Pid(actor.pid()),
            Box::new(format!("msg{}", i)),
            Duration::from_millis(i * 50),
        );
    }

    // Wait for all to be delivered
    sleep(Duration::from_millis(400)).await;
    assert_eq!(*counter.lock().unwrap(), 5);
}

#[tokio::test]
async fn test_scheduled_to_named_process() {
    let system = ActorSystem::new();
    let counter = Arc::new(Mutex::new(0));
    let actor = system.spawn(CountingActor {
        counter: Arc::clone(&counter),
    });

    // Register the actor
    system.register("target", actor.pid()).unwrap();

    // Schedule using name
    system.send_after(
        Destination::Name("target".to_string()),
        Box::new("test".to_string()),
        Duration::from_millis(100),
    );

    sleep(Duration::from_millis(200)).await;
    assert_eq!(*counter.lock().unwrap(), 1);
}

#[tokio::test]
async fn test_scheduled_to_nonexistent_named_process() {
    let system = ActorSystem::new();

    // Schedule to non-existent name (should not panic)
    system.send_after(
        Destination::Name("nonexistent".to_string()),
        Box::new("test".to_string()),
        Duration::from_millis(100),
    );

    // Just wait to ensure no panic
    sleep(Duration::from_millis(200)).await;
}

#[tokio::test]
async fn test_send_after_from_actor_context() {
    use std::sync::{Arc, Mutex};

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received);

    struct SelfSchedulingActor {
        received: Arc<Mutex<Vec<String>>>,
        scheduled: bool,
    }

    #[async_trait]
    impl Actor for SelfSchedulingActor {
        async fn started(&mut self, ctx: &mut ActorContext) {
            // Schedule a message to ourselves
            ctx.send_after(
                Destination::Pid(ctx.pid()),
                Box::new("scheduled".to_string()),
                Duration::from_millis(100),
            );
            self.scheduled = true;
        }

        async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
            if let Some(text) = msg.downcast_ref::<String>() {
                self.received.lock().unwrap().push(text.clone());
            }
        }
    }

    let system = ActorSystem::new();
    let _actor = system.spawn(SelfSchedulingActor {
        received: received_clone,
        scheduled: false,
    });

    sleep(Duration::from_millis(200)).await;
    let messages = received.lock().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], "scheduled");
}

#[tokio::test]
async fn test_message_ordering_with_scheduler() {
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = Arc::clone(&received);

    struct OrderingActor {
        received: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Actor for OrderingActor {
        async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
            if let Some(text) = msg.downcast_ref::<String>() {
                self.received.lock().unwrap().push(text.clone());
            }
        }
    }

    let system = ActorSystem::new();
    let actor = system.spawn(OrderingActor {
        received: received_clone,
    });

    // Schedule messages with specific order
    system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("third".to_string()),
        Duration::from_millis(150),
    );

    system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("first".to_string()),
        Duration::from_millis(50),
    );

    system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("second".to_string()),
        Duration::from_millis(100),
    );

    // Wait for all messages
    sleep(Duration::from_millis(250)).await;

    let messages = received.lock().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0], "first");
    assert_eq!(messages[1], "second");
    assert_eq!(messages[2], "third");
}

#[tokio::test]
async fn test_cancel_multiple_timers() {
    let system = ActorSystem::new();
    let counter = Arc::new(Mutex::new(0));
    let actor = system.spawn(CountingActor {
        counter: Arc::clone(&counter),
    });

    // Schedule multiple messages
    let timer1 = system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("msg1".to_string()),
        Duration::from_secs(10),
    );

    let timer2 = system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("msg2".to_string()),
        Duration::from_secs(10),
    );

    let _timer3 = system.send_after(
        Destination::Pid(actor.pid()),
        Box::new("msg3".to_string()),
        Duration::from_millis(100),
    );

    // Cancel first two
    assert!(system.cancel_timer(timer1).unwrap());
    assert!(system.cancel_timer(timer2).unwrap());

    // Wait for third to deliver
    sleep(Duration::from_millis(200)).await;

    // Only third should have been delivered
    assert_eq!(*counter.lock().unwrap(), 1);
}
