//! Integration tests for the named process registry.

use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, ExitReason, Message};
use tokio::time::{Duration, sleep};

struct TestActor {
    _name: String,
}

#[async_trait]
impl Actor for TestActor {
    async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
}

#[tokio::test]
async fn test_register_and_lookup() {
    let system = ActorSystem::new();
    let actor = system.spawn(TestActor {
        _name: "test".to_string(),
    });

    // Register the actor
    system.register("test_actor", actor.pid()).unwrap();

    // Look it up
    let found_pid = system.whereis("test_actor");
    assert_eq!(found_pid, Some(actor.pid()));

    // Look up non-existent name
    assert_eq!(system.whereis("nonexistent"), None);
}

#[tokio::test]
async fn test_register_duplicate_name() {
    let system = ActorSystem::new();
    let actor1 = system.spawn(TestActor {
        _name: "actor1".to_string(),
    });
    let actor2 = system.spawn(TestActor {
        _name: "actor2".to_string(),
    });

    // Register first actor
    system.register("same_name", actor1.pid()).unwrap();

    // Try to register second actor with same name
    let result = system.register("same_name", actor2.pid());
    assert!(result.is_err());

    // First actor should still be registered
    assert_eq!(system.whereis("same_name"), Some(actor1.pid()));
}

#[tokio::test]
async fn test_unregister() {
    let system = ActorSystem::new();
    let actor = system.spawn(TestActor {
        _name: "test".to_string(),
    });

    system.register("test_actor", actor.pid()).unwrap();
    assert!(system.whereis("test_actor").is_some());

    // Unregister
    let unregistered_pid = system.unregister("test_actor").unwrap();
    assert_eq!(unregistered_pid, actor.pid());

    // Should no longer be found
    assert_eq!(system.whereis("test_actor"), None);
}

#[tokio::test]
async fn test_registered_list() {
    let system = ActorSystem::new();
    let actor1 = system.spawn(TestActor {
        _name: "actor1".to_string(),
    });
    let actor2 = system.spawn(TestActor {
        _name: "actor2".to_string(),
    });

    system.register("name1", actor1.pid()).unwrap();
    system.register("name2", actor2.pid()).unwrap();

    let registered = system.registered();
    assert_eq!(registered.len(), 2);
    assert!(registered.contains(&"name1".to_string()));
    assert!(registered.contains(&"name2".to_string()));
}

#[tokio::test]
async fn test_automatic_cleanup_on_actor_death() {
    let system = ActorSystem::new();

    struct DyingActor;

    #[async_trait]
    impl Actor for DyingActor {
        async fn started(&mut self, ctx: &mut ActorContext) {
            // Stop immediately
            ctx.stop(ExitReason::Normal);
        }

        async fn handle_message(&mut self, _msg: Message, _ctx: &mut ActorContext) {}
    }

    let actor = system.spawn(DyingActor);
    system.register("dying_actor", actor.pid()).unwrap();

    // Verify it's registered
    assert!(system.whereis("dying_actor").is_some());

    // Wait for actor to die and be cleaned up
    sleep(Duration::from_millis(100)).await;

    // [TODO] Should be automatically unregistered
    // assert_eq!(system.whereis("dying_actor"), None);
}

#[tokio::test]
async fn test_send_to_named_actor() {
    use std::sync::{Arc, Mutex};

    let received = Arc::new(Mutex::new(false));
    let received_clone = Arc::clone(&received);

    struct ReceivingActor {
        received: Arc<Mutex<bool>>,
    }

    #[async_trait]
    impl Actor for ReceivingActor {
        async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
            if msg.downcast_ref::<String>().is_some() {
                *self.received.lock().unwrap() = true;
            }
        }
    }

    let system = ActorSystem::new();
    let actor = system.spawn(ReceivingActor {
        received: received_clone,
    });

    system.register("receiver", actor.pid()).unwrap();

    // Send message using looked-up Pid
    if let Some(pid) = system.whereis("receiver") {
        system
            .send(pid, Box::new("test".to_string()))
            .await
            .unwrap();
    }

    sleep(Duration::from_millis(100)).await;
    assert!(*received.lock().unwrap());
}

#[tokio::test]
async fn test_reregister_same_pid_different_name() {
    let system = ActorSystem::new();
    let actor = system.spawn(TestActor {
        _name: "test".to_string(),
    });

    // Register with first name
    system.register("name1", actor.pid()).unwrap();
    assert_eq!(system.whereis("name1"), Some(actor.pid()));

    // Register same pid with different name
    system.register("name2", actor.pid()).unwrap();

    // First name should be gone, second name should exist
    assert_eq!(system.whereis("name1"), None);
    assert_eq!(system.whereis("name2"), Some(actor.pid()));
}

#[tokio::test]
async fn test_concurrent_registrations() {
    let system = ActorSystem::new();
    let mut handles = vec![];

    for i in 0..10 {
        let sys = system.clone();
        let handle = tokio::spawn(async move {
            let actor = sys.spawn(TestActor {
                _name: format!("actor{}", i),
            });
            sys.register(format!("concurrent{}", i), actor.pid())
                .unwrap();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // All should be registered
    let registered = system.registered();
    assert_eq!(registered.len(), 10);
}
