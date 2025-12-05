//! Integration tests for joerl actor system.

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, ExitReason, Message, Signal};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{Duration, sleep};

struct CountingActor {
    counter: Arc<AtomicUsize>,
}

#[async_trait]
impl Actor for CountingActor {
    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if msg.downcast_ref::<()>().is_some() {
            self.counter.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[tokio::test]
async fn test_actor_processes_messages() {
    let system = ActorSystem::new();
    let counter = Arc::new(AtomicUsize::new(0));

    let actor = system.spawn(CountingActor {
        counter: Arc::clone(&counter),
    });

    // Send messages
    for _ in 0..10 {
        actor.send(Box::new(())).await.unwrap();
    }

    sleep(Duration::from_millis(50)).await;
    assert_eq!(counter.load(Ordering::SeqCst), 10);
}

struct ExitActor {
    #[allow(dead_code)]
    should_panic: bool,
}

#[async_trait]
impl Actor for ExitActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "exit_normal" => ctx.stop(ExitReason::Normal),
                "exit_panic" => ctx.stop(ExitReason::Panic("intentional".to_string())),
                _ => {}
            }
        }
    }
}

#[tokio::test]
async fn test_actor_exit_normal() {
    let system = ActorSystem::new();
    let actor = system.spawn(ExitActor {
        should_panic: false,
    });
    let pid = actor.pid();

    actor.send(Box::new("exit_normal")).await.unwrap();
    sleep(Duration::from_millis(50)).await;

    assert!(!system.is_alive(pid));
}

#[tokio::test]
async fn test_actor_link_propagates_failure() {
    let system = ActorSystem::new();

    struct LinkedActor {
        exit_received: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Actor for LinkedActor {
        async fn started(&mut self, ctx: &mut ActorContext) {
            ctx.trap_exit(true);
        }

        async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
            if let Signal::Exit { .. } = signal {
                self.exit_received.fetch_add(1, Ordering::SeqCst);
            }
        }

        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    }

    let exit_counter = Arc::new(AtomicUsize::new(0));

    let actor1 = system.spawn(ExitActor { should_panic: true });
    let actor2 = system.spawn(LinkedActor {
        exit_received: Arc::clone(&exit_counter),
    });

    // Link the actors
    system.link(actor1.pid(), actor2.pid()).unwrap();

    // Make actor1 exit abnormally
    actor1.send(Box::new("exit_panic")).await.unwrap();

    sleep(Duration::from_millis(100)).await;

    // Actor2 should have received an EXIT signal
    assert_eq!(exit_counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_actor_monitor() {
    let system = ActorSystem::new();

    struct MonitoringActor {
        down_received: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Actor for MonitoringActor {
        async fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActorContext) {
            if let Signal::Down { .. } = signal {
                self.down_received.fetch_add(1, Ordering::SeqCst);
            }
        }

        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    }

    let down_counter = Arc::new(AtomicUsize::new(0));

    let actor1 = system.spawn(ExitActor { should_panic: true });
    let actor2 = system.spawn(MonitoringActor {
        down_received: Arc::clone(&down_counter),
    });

    // Monitor actor1 from actor2
    actor1.monitor(actor2.pid()).unwrap();

    // Make actor1 exit
    actor1.send(Box::new("exit_normal")).await.unwrap();

    sleep(Duration::from_millis(100)).await;

    // Actor2 should have received a DOWN signal
    assert_eq!(down_counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_mailbox_backpressure() {
    let system = ActorSystem::new();

    struct SlowActor;

    #[async_trait]
    impl Actor for SlowActor {
        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {
            // Process slowly
            sleep(Duration::from_millis(50)).await;
        }
    }

    let actor = system.spawn_with_capacity(SlowActor, 2);

    // First 2 sends should succeed immediately
    assert!(actor.try_send(Box::new(())).is_ok());
    assert!(actor.try_send(Box::new(())).is_ok());

    // Third should fail due to bounded mailbox
    assert!(actor.try_send(Box::new(())).is_err());

    sleep(Duration::from_millis(100)).await;

    // Now should succeed again
    assert!(actor.try_send(Box::new(())).is_ok());
}

#[tokio::test]
async fn test_actor_lifecycle_hooks() {
    let system = ActorSystem::new();
    let started = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(AtomicUsize::new(0));

    struct LifecycleActor {
        started: Arc<AtomicUsize>,
        stopped: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Actor for LifecycleActor {
        async fn started(&mut self, _ctx: &mut ActorContext) {
            self.started.fetch_add(1, Ordering::SeqCst);
        }

        async fn stopped(&mut self, _reason: &ExitReason, _ctx: &mut ActorContext) {
            self.stopped.fetch_add(1, Ordering::SeqCst);
        }

        async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
            if let Some(cmd) = msg.downcast_ref::<&str>()
                && *cmd == "stop"
            {
                ctx.stop(ExitReason::Normal);
            }
        }
    }

    let actor = system.spawn(LifecycleActor {
        started: Arc::clone(&started),
        stopped: Arc::clone(&stopped),
    });

    sleep(Duration::from_millis(50)).await;
    assert_eq!(started.load(Ordering::SeqCst), 1);

    actor.send(Box::new("stop")).await.unwrap();
    sleep(Duration::from_millis(50)).await;
    assert_eq!(stopped.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_multiple_actors_concurrently() {
    let system = ActorSystem::new();

    struct SimpleActor;

    #[async_trait]
    impl Actor for SimpleActor {
        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {
            // Just process the message
        }
    }

    // Spawn multiple actors and verify they all start
    let actors: Vec<_> = (0..10).map(|_| system.spawn(SimpleActor)).collect();

    sleep(Duration::from_millis(50)).await;

    // Verify all actors are alive
    for actor in &actors {
        assert!(system.is_alive(actor.pid()));
    }

    // Send a message to each actor
    for actor in &actors {
        actor.send(Box::new(())).await.unwrap();
    }

    sleep(Duration::from_millis(50)).await;

    // All actors should still be alive
    for actor in &actors {
        assert!(system.is_alive(actor.pid()));
    }
}
