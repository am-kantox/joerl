//! Comprehensive example demonstrating trait-based message serialization in joerl.
//!
//! This example shows:
//! - How to implement SerializableMessage for custom types
//! - Setting up a MessageRegistry with deserializers
//! - Serializing and deserializing messages
//! - Using SerializableEnvelope for wire transmission
//! - Integrating with the actor system
//!
//! Run with: cargo run --example serialization_example

use async_trait::async_trait;
use joerl::serialization::{
    MessageRegistry, SerializableEnvelope, SerializableMessage, SerializationError,
};
use joerl::{Actor, ActorContext, ActorSystem, Message};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// Message Types
// ============================================================================

/// A simple ping message with a counter
#[derive(Debug, Clone, PartialEq)]
struct PingMessage {
    count: u32,
}

impl SerializableMessage for PingMessage {
    fn message_type_id(&self) -> &'static str {
        "example::PingMessage"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        Ok(self.count.to_le_bytes().to_vec())
    }
}

/// Deserializer for PingMessage
fn deserialize_ping(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() != 4 {
        return Err(SerializationError::DeserializeFailed(
            "PingMessage requires 4 bytes".to_string(),
        ));
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    Ok(Box::new(PingMessage { count }))
}

/// A text message with variable-length content
#[derive(Debug, Clone, PartialEq)]
struct TextMessage {
    sender: String,
    content: String,
}

impl SerializableMessage for TextMessage {
    fn message_type_id(&self) -> &'static str {
        "example::TextMessage"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        // Format: [sender_len(4)][sender][content_len(4)][content]
        let sender_bytes = self.sender.as_bytes();
        let content_bytes = self.content.as_bytes();

        let mut result = Vec::with_capacity(8 + sender_bytes.len() + content_bytes.len());
        result.extend_from_slice(&(sender_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(sender_bytes);
        result.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(content_bytes);

        Ok(result)
    }
}

/// Deserializer for TextMessage
fn deserialize_text(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() < 8 {
        return Err(SerializationError::DeserializeFailed(
            "TextMessage data too short".to_string(),
        ));
    }

    // Read sender length
    let sender_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 8 + sender_len {
        return Err(SerializationError::DeserializeFailed(
            "Invalid sender length".to_string(),
        ));
    }

    // Read sender
    let sender = String::from_utf8(data[4..4 + sender_len].to_vec())
        .map_err(|e| SerializationError::DeserializeFailed(format!("Invalid UTF-8: {}", e)))?;

    // Read content length
    let content_len_offset = 4 + sender_len;
    let content_len = u32::from_le_bytes([
        data[content_len_offset],
        data[content_len_offset + 1],
        data[content_len_offset + 2],
        data[content_len_offset + 3],
    ]) as usize;

    // Read content
    let content_offset = content_len_offset + 4;
    if data.len() < content_offset + content_len {
        return Err(SerializationError::DeserializeFailed(
            "Invalid content length".to_string(),
        ));
    }

    let content = String::from_utf8(data[content_offset..content_offset + content_len].to_vec())
        .map_err(|e| SerializationError::DeserializeFailed(format!("Invalid UTF-8: {}", e)))?;

    Ok(Box::new(TextMessage { sender, content }))
}

// ============================================================================
// Actors
// ============================================================================

/// Actor that handles serializable messages
struct MessageHandler {
    registry: Arc<Mutex<MessageRegistry>>,
    messages_received: usize,
}

impl MessageHandler {
    fn new(registry: Arc<Mutex<MessageRegistry>>) -> Self {
        Self {
            registry,
            messages_received: 0,
        }
    }
}

#[async_trait]
impl Actor for MessageHandler {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("✓ MessageHandler started with PID: {}", ctx.pid());
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        self.messages_received += 1;

        // Try to handle as SerializableEnvelope
        if let Some(envelope) = msg.downcast_ref::<SerializableEnvelope>() {
            println!(
                "\n→ Received serialized message #{} (type: {})",
                self.messages_received,
                envelope.type_id()
            );

            let registry = self.registry.lock().await;
            match envelope.unwrap(&registry) {
                Ok(msg) => {
                    // Downcast to specific types
                    if let Some(ping) = msg.as_any().downcast_ref::<PingMessage>() {
                        println!("  ├─ PingMessage {{ count: {} }}", ping.count);
                    } else if let Some(text) = msg.as_any().downcast_ref::<TextMessage>() {
                        println!(
                            "  ├─ TextMessage {{ sender: \"{}\", content: \"{}\" }}",
                            text.sender, text.content
                        );
                    }
                }
                Err(e) => {
                    println!("  └─ Failed to deserialize: {}", e);
                }
            }
        }
        // Handle non-serialized messages
        else if let Some(cmd) = msg.downcast_ref::<&str>() {
            if *cmd == "stats" {
                println!("\n📊 Stats: {} messages received", self.messages_received);
            } else if *cmd == "stop" {
                println!("\n⏹  Stopping actor");
                ctx.stop(joerl::ExitReason::Normal);
            }
        }
    }

    async fn stopped(&mut self, reason: &joerl::ExitReason, ctx: &mut ActorContext) {
        println!(
            "\n✗ MessageHandler stopped: {} (PID: {})",
            reason,
            ctx.pid()
        );
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Sets up a message registry with all deserializers
fn setup_registry() -> MessageRegistry {
    let mut registry = MessageRegistry::new();

    // Register PingMessage
    registry.register("example::PingMessage", Box::new(deserialize_ping));

    // Register TextMessage
    registry.register("example::TextMessage", Box::new(deserialize_text));

    println!(
        "✓ Registry configured with {} message types",
        registry.len()
    );
    registry
}

/// Simulates network transmission by converting to/from bytes
fn simulate_network_transmission(envelope: &SerializableEnvelope) -> SerializableEnvelope {
    let wire_bytes = envelope.to_bytes();
    println!("  ├─ Wire format: {} bytes", wire_bytes.len());

    // Reconstruct from wire format
    SerializableEnvelope::from_bytes(&wire_bytes).expect("Failed to reconstruct envelope")
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  joerl Trait-Based Serialization Example");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Step 1: Setup registry
    println!("1. Setting up MessageRegistry");
    let registry = Arc::new(Mutex::new(setup_registry()));
    println!();

    // Step 2: Create actor system
    println!("2. Creating ActorSystem");
    let system = ActorSystem::new();
    println!("✓ ActorSystem created\n");

    // Step 3: Spawn message handler
    println!("3. Spawning MessageHandler");
    let handler = system.spawn(MessageHandler::new(Arc::clone(&registry)));
    println!();

    // Step 4: Send serializable messages
    println!("4. Sending serializable messages");
    println!("─────────────────────────────────────────────────────────────");

    // Send PingMessage
    let ping_msg = PingMessage { count: 42 };
    println!("\n→ Sending PingMessage {{ count: 42 }}");
    let envelope = SerializableEnvelope::wrap(&ping_msg).unwrap();
    let envelope = simulate_network_transmission(&envelope);
    handler.send(Box::new(envelope)).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send TextMessage
    let text_msg = TextMessage {
        sender: "Alice".to_string(),
        content: "Hello, distributed world!".to_string(),
    };
    println!(
        "\n→ Sending TextMessage {{ sender: \"Alice\", content: \"Hello, distributed world!\" }}"
    );
    let envelope = SerializableEnvelope::wrap(&text_msg).unwrap();
    let envelope = simulate_network_transmission(&envelope);
    handler.send(Box::new(envelope)).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send another PingMessage
    let ping_msg2 = PingMessage { count: 999 };
    println!("\n→ Sending PingMessage {{ count: 999 }}");
    let envelope = SerializableEnvelope::wrap(&ping_msg2).unwrap();
    let envelope = simulate_network_transmission(&envelope);
    handler.send(Box::new(envelope)).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Step 5: Request stats
    println!("\n─────────────────────────────────────────────────────────────");
    println!("5. Requesting stats");
    handler.send(Box::new("stats")).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Step 6: Demonstrate round-trip serialization
    println!("\n─────────────────────────────────────────────────────────────");
    println!("6. Demonstrating round-trip serialization");

    let original = TextMessage {
        sender: "Bob".to_string(),
        content: "Testing serialization".to_string(),
    };
    println!("\n→ Original: {:?}", original);

    // Serialize
    let envelope = SerializableEnvelope::wrap(&original).unwrap();
    println!("  ├─ Type ID: {}", envelope.type_id());
    println!("  ├─ Data length: {} bytes", envelope.data().len());

    // To wire format
    let wire_bytes = envelope.to_bytes();
    println!("  ├─ Wire format: {} bytes", wire_bytes.len());

    // From wire format
    let reconstructed_envelope = SerializableEnvelope::from_bytes(&wire_bytes).unwrap();

    // Deserialize
    let registry = registry.lock().await;
    let reconstructed_msg = reconstructed_envelope.unwrap(&registry).unwrap();
    let reconstructed = reconstructed_msg
        .as_any()
        .downcast_ref::<TextMessage>()
        .unwrap();

    println!("  └─ Reconstructed: {:?}", reconstructed);
    assert_eq!(&original, reconstructed);
    println!("\n✓ Round-trip successful!\n");

    // Step 7: Shutdown
    println!("─────────────────────────────────────────────────────────────");
    println!("7. Shutting down");
    handler.send(Box::new("stop")).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("  Example completed successfully");
    println!("═══════════════════════════════════════════════════════════════\n");
}
