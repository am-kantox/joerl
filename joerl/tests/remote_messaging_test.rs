//! Integration tests for remote messaging between distributed nodes.

use async_trait::async_trait;
use joerl::distributed::DistributedSystem;
use joerl::epmd::EpmdServer;
use joerl::serialization::{SerializableMessage, SerializationError, register_message_type};
use joerl::{Actor, ActorContext, Message};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

/// Simple ping message
#[derive(Debug, Clone, PartialEq)]
struct PingMsg {
    count: u32,
    reply_to: joerl::Pid,
}

impl SerializableMessage for PingMsg {
    fn message_type_id(&self) -> &'static str {
        "remote_test::PingMsg"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.count.to_le_bytes());
        bytes.extend_from_slice(&self.reply_to.node().to_le_bytes());
        bytes.extend_from_slice(&self.reply_to.id().to_le_bytes());
        Ok(bytes)
    }
}

fn deserialize_ping(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() != 16 {
        return Err(SerializationError::InvalidFormat(
            "PingMsg requires 16 bytes".into(),
        ));
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let node = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let id = u64::from_le_bytes([
        data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
    ]);
    let reply_to = joerl::Pid::with_node(node, id);
    Ok(Box::new(PingMsg { count, reply_to }))
}

/// Simple pong message
#[derive(Debug, Clone, PartialEq)]
struct PongMsg {
    count: u32,
}

impl SerializableMessage for PongMsg {
    fn message_type_id(&self) -> &'static str {
        "remote_test::PongMsg"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        Ok(self.count.to_le_bytes().to_vec())
    }
}

fn deserialize_pong(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() != 4 {
        return Err(SerializationError::InvalidFormat(
            "PongMsg requires 4 bytes".into(),
        ));
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    Ok(Box::new(PongMsg { count }))
}

/// Command message to trigger sending
#[derive(Debug, Clone, PartialEq)]
struct SendPingCmd;

impl SerializableMessage for SendPingCmd {
    fn message_type_id(&self) -> &'static str {
        "remote_test::SendPingCmd"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        Ok(vec![]) // Empty payload
    }
}

fn deserialize_send_ping(_data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    Ok(Box::new(SendPingCmd))
}

/// Actor that responds to pings
struct PongActor {
    received: Arc<Mutex<Vec<u32>>>,
    dist_system: Arc<DistributedSystem>,
}

#[async_trait]
impl Actor for PongActor {
    async fn started(&mut self, _ctx: &mut ActorContext) {
        // Register message types lazily
        register_message_type("remote_test::PingMsg", Box::new(deserialize_ping));
        register_message_type("remote_test::PongMsg", Box::new(deserialize_pong));
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(boxed) = msg.downcast_ref::<Box<dyn SerializableMessage>>()
            && let Some(ping) = boxed.as_ref().as_any().downcast_ref::<PingMsg>()
        {
            // Record received ping
            self.received.lock().await.push(ping.count);

            // Send pong back
            let pong = PongMsg { count: ping.count };
            let pong_msg: Message = Box::new(Box::new(pong) as Box<dyn SerializableMessage>);

            let _ = self
                .dist_system
                .send(ctx.pid(), ping.reply_to, pong_msg)
                .await;
        }
    }
}

/// Actor that sends pings and waits for pongs
struct PingActor {
    target: joerl::Pid,
    received_pongs: Arc<Mutex<Vec<u32>>>,
    dist_system: Arc<DistributedSystem>,
}

#[async_trait]
impl Actor for PingActor {
    async fn started(&mut self, _ctx: &mut ActorContext) {
        register_message_type("remote_test::PingMsg", Box::new(deserialize_ping));
        register_message_type("remote_test::PongMsg", Box::new(deserialize_pong));
        register_message_type("remote_test::SendPingCmd", Box::new(deserialize_send_ping));
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(boxed) = msg.downcast_ref::<Box<dyn SerializableMessage>>() {
            if let Some(pong) = boxed.as_ref().as_any().downcast_ref::<PongMsg>() {
                // Record received pong
                self.received_pongs.lock().await.push(pong.count);
            } else if boxed
                .as_ref()
                .as_any()
                .downcast_ref::<SendPingCmd>()
                .is_some()
            {
                // Send ping to remote actor
                let ping = PingMsg {
                    count: 1,
                    reply_to: ctx.pid(),
                };
                let ping_msg: Message = Box::new(Box::new(ping) as Box<dyn SerializableMessage>);

                let _ = self
                    .dist_system
                    .send(ctx.pid(), self.target, ping_msg)
                    .await;
            }
        }
    }
}

#[tokio::test]
async fn test_remote_ping_pong() {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt::try_init();

    // Start EPMD server for this test on unique port
    let epmd_server = EpmdServer::new("127.0.0.1:14369") // Use unique port for this test
        .with_keep_alive_timeout(Duration::from_secs(60));
    tokio::spawn(async move {
        let _ = epmd_server.run().await;
    });

    // Give EPMD time to start
    sleep(Duration::from_millis(200)).await;

    // Register message types globally
    register_message_type("remote_test::PingMsg", Box::new(deserialize_ping));
    register_message_type("remote_test::PongMsg", Box::new(deserialize_pong));
    register_message_type("remote_test::SendPingCmd", Box::new(deserialize_send_ping));

    // Create two distributed systems on different ports (using same EPMD)
    let node_a = DistributedSystem::new("node_a", "127.0.0.1:15000", "127.0.0.1:14369")
        .await
        .expect("Failed to create node_a");

    let node_b = DistributedSystem::new("node_b", "127.0.0.1:15001", "127.0.0.1:14369")
        .await
        .expect("Failed to create node_b");

    // Give nodes time to register with EPMD
    sleep(Duration::from_millis(500)).await;

    // Connect nodes bidirectionally
    node_a
        .connect_to_node("node_b")
        .await
        .expect("Failed to connect node_a to node_b");

    node_b
        .connect_to_node("node_a")
        .await
        .expect("Failed to connect node_b to node_a");

    sleep(Duration::from_millis(200)).await;

    // Create shared state for tracking messages
    let pong_received = Arc::new(Mutex::new(Vec::new()));
    let ping_received_pongs = Arc::new(Mutex::new(Vec::new()));

    // Spawn PongActor on node_b
    let pong_actor = PongActor {
        received: Arc::clone(&pong_received),
        dist_system: Arc::clone(&node_b),
    };
    let pong_ref = node_b.system().spawn(pong_actor);

    // Create remote Pid for pong actor (with node_b's node_id)
    let pong_remote_pid = joerl::Pid::with_node(node_b.node_id(), pong_ref.pid().id());

    // Spawn PingActor on node_a
    let ping_actor = PingActor {
        target: pong_remote_pid,
        received_pongs: Arc::clone(&ping_received_pongs),
        dist_system: Arc::clone(&node_a),
    };
    let ping_ref = node_a.system().spawn(ping_actor);

    // Give actors time to start
    sleep(Duration::from_millis(200)).await;

    // Trigger ping
    let trigger_msg: Message = Box::new(Box::new(SendPingCmd) as Box<dyn SerializableMessage>);
    ping_ref
        .send(trigger_msg)
        .await
        .expect("Failed to send trigger");

    // Wait for round-trip (longer to ensure messages are delivered)
    sleep(Duration::from_millis(1500)).await;

    // Verify messages were received
    let pongs = pong_received.lock().await;
    let pings = ping_received_pongs.lock().await;

    println!("PongActor received {} pings: {:?}", pongs.len(), *pongs);
    println!("PingActor received {} pongs: {:?}", pings.len(), *pings);

    assert_eq!(pongs.len(), 1, "PongActor should receive 1 ping");
    assert_eq!(pongs[0], 1, "Ping count should be 1");

    assert_eq!(pings.len(), 1, "PingActor should receive 1 pong");
    assert_eq!(pings[0], 1, "Pong count should be 1");

    // Cleanup
    node_a.shutdown().await.ok();
    node_b.shutdown().await.ok();
}

#[tokio::test]
async fn test_remote_actor_not_found() {
    // Start EPMD server for this test
    let epmd_server = EpmdServer::new("127.0.0.1:4370") // Use different port to avoid conflict
        .with_keep_alive_timeout(Duration::from_secs(60));
    tokio::spawn(async move {
        let _ = epmd_server.run().await;
    });

    // Give EPMD time to start
    sleep(Duration::from_millis(100)).await;

    // Register message types
    register_message_type("remote_test::PingMsg", Box::new(deserialize_ping));

    // Create a distributed system (use same EPMD port as spawned above)
    let node_a = DistributedSystem::new("node_test_a", "127.0.0.1:15100", "127.0.0.1:4370")
        .await
        .expect("Failed to create node");

    sleep(Duration::from_millis(500)).await;

    // Try to send to non-existent remote Pid
    let fake_pid = joerl::Pid::with_node(99999, 42);
    let ping = PingMsg {
        count: 1,
        reply_to: joerl::Pid::new(),
    };
    let msg: Message = Box::new(Box::new(ping) as Box<dyn SerializableMessage>);

    let result = node_a.send(joerl::Pid::new(), fake_pid, msg).await;

    // Should fail with node not found
    assert!(result.is_err());

    node_a.shutdown().await.ok();
}
