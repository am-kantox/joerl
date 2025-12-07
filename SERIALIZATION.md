# Trait-Based Serialization in joerl

This document describes the trait-based serialization system implemented in joerl, which enables messages to be serialized for network transmission, persistence, or other purposes.

## Overview

The serialization system provides a flexible framework where each message type controls its own serialization logic. This approach offers several advantages:

- **Type Safety**: Each message type is uniquely identified
- **Flexibility**: Custom serialization logic per type
- **Performance**: No unnecessary overhead from generic serialization frameworks
- **Integration**: Seamlessly works with the existing actor system

## Core Components

### 1. SerializableMessage Trait

The `SerializableMessage` trait extends message functionality with serialization capabilities:

```rust
pub trait SerializableMessage: Any + Send + Sync {
    /// Returns a unique type identifier for this message type
    fn message_type_id(&self) -> &'static str;
    
    /// Returns self as &dyn Any for downcasting
    fn as_any(&self) -> &dyn Any;
    
    /// Serializes the message into bytes
    fn serialize(&self) -> Result<Vec<u8>, SerializationError>;
}
```

### 2. MessageRegistry

The `MessageRegistry` maintains a mapping from type IDs to deserializer functions:

```rust
pub struct MessageRegistry {
    // Internal: Arc<RwLock<HashMap<String, Deserializer>>>
}

impl MessageRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, type_id: &str, deserializer: Deserializer);
    pub fn deserialize(&self, type_id: &str, data: &[u8]) 
        -> Result<Box<dyn SerializableMessage>, SerializationError>;
}
```

### 3. SerializableEnvelope

The `SerializableEnvelope` wraps serialized messages for transmission:

```rust
pub struct SerializableEnvelope {
    type_id: String,
    data: Vec<u8>,
}

impl SerializableEnvelope {
    pub fn wrap(msg: &dyn SerializableMessage) -> Result<Self, SerializationError>;
    pub fn unwrap(&self, registry: &MessageRegistry) 
        -> Result<Box<dyn SerializableMessage>, SerializationError>;
    pub fn to_bytes(&self) -> Vec<u8>;
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SerializationError>;
}
```

## Usage Example

### Step 1: Define a Message Type

```rust
use joerl::serialization::{SerializableMessage, SerializationError};
use std::any::Any;

#[derive(Debug, Clone, PartialEq)]
struct PingMessage {
    count: u32,
}

impl SerializableMessage for PingMessage {
    fn message_type_id(&self) -> &'static str {
        "my_app::PingMessage"  // Use namespaced IDs to avoid collisions
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        // Simple serialization: just the count as little-endian bytes
        Ok(self.count.to_le_bytes().to_vec())
    }
}
```

### Step 2: Create a Deserializer

```rust
fn deserialize_ping(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() != 4 {
        return Err(SerializationError::DeserializeFailed(
            "PingMessage requires 4 bytes".to_string()
        ));
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    Ok(Box::new(PingMessage { count }))
}
```

### Step 3: Register with the Registry

```rust
let mut registry = MessageRegistry::new();
registry.register("my_app::PingMessage", Box::new(deserialize_ping));
```

### Step 4: Serialize and Send

```rust
use joerl::serialization::SerializableEnvelope;

// Create a message
let msg = PingMessage { count: 42 };

// Wrap it in an envelope
let envelope = SerializableEnvelope::wrap(&msg)?;

// Convert to wire format for transmission
let wire_bytes = envelope.to_bytes();

// ... send wire_bytes over network ...
```

### Step 5: Receive and Deserialize

```rust
// ... receive wire_bytes from network ...

// Reconstruct envelope from wire format
let envelope = SerializableEnvelope::from_bytes(&wire_bytes)?;

// Deserialize using the registry
let msg = envelope.unwrap(&registry)?;

// Downcast to specific type
if let Some(ping) = msg.as_any().downcast_ref::<PingMessage>() {
    println!("Received ping with count: {}", ping.count);
}
```

### Step 6: Integration with Actor System

```rust
use joerl::{Actor, ActorContext, ActorSystem, Message};
use async_trait::async_trait;

struct MessageHandler {
    registry: Arc<Mutex<MessageRegistry>>,
}

#[async_trait]
impl Actor for MessageHandler {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        // Try to handle as SerializableEnvelope
        if let Some(envelope) = msg.downcast_ref::<SerializableEnvelope>() {
            let registry = self.registry.lock().await;
            match envelope.unwrap(&registry) {
                Ok(deserialized) => {
                    // Handle the deserialized message
                    if let Some(ping) = deserialized.as_any().downcast_ref::<PingMessage>() {
                        println!("Received: {:?}", ping);
                    }
                }
                Err(e) => eprintln!("Failed to deserialize: {}", e),
            }
        }
    }
}
```

## Wire Format

The `SerializableEnvelope` uses a simple binary format:

```
[type_id_len: u32][type_id: UTF-8 string][data_len: u32][data: bytes]
```

- **type_id_len**: 4 bytes, little-endian u32, length of type_id string
- **type_id**: Variable-length UTF-8 string
- **data_len**: 4 bytes, little-endian u32, length of serialized data
- **data**: Variable-length binary data (message-specific format)

### Example Wire Format

For `PingMessage { count: 42 }` with type ID "my_app::PingMessage":

```
[19, 0, 0, 0]                    // type_id_len = 19
[m, y, _, a, p, p, :, :, P, ...] // type_id = "my_app::PingMessage"
[4, 0, 0, 0]                     // data_len = 4
[42, 0, 0, 0]                    // data = 42 as u32 little-endian
```

## Design Patterns

### Pattern 1: Simple Fixed-Size Messages

For messages with fixed-size fields:

```rust
#[derive(Debug, Clone)]
struct Position {
    x: f32,
    y: f32,
}

impl SerializableMessage for Position {
    fn message_type_id(&self) -> &'static str { "Position" }
    fn as_any(&self) -> &dyn Any { self }
    
    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        let mut bytes = Vec::with_capacity(8);
        bytes.extend_from_slice(&self.x.to_le_bytes());
        bytes.extend_from_slice(&self.y.to_le_bytes());
        Ok(bytes)
    }
}
```

### Pattern 2: Variable-Length Messages

For messages with strings or dynamic-length data:

```rust
#[derive(Debug, Clone)]
struct TextMessage {
    sender: String,
    content: String,
}

impl SerializableMessage for TextMessage {
    fn message_type_id(&self) -> &'static str { "TextMessage" }
    fn as_any(&self) -> &dyn Any { self }
    
    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        let sender_bytes = self.sender.as_bytes();
        let content_bytes = self.content.as_bytes();
        
        let mut result = Vec::new();
        result.extend_from_slice(&(sender_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(sender_bytes);
        result.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(content_bytes);
        
        Ok(result)
    }
}
```

### Pattern 3: Nested Messages

For messages containing other serializable messages:

```rust
#[derive(Debug, Clone)]
struct Container {
    inner: Vec<PingMessage>,
}

impl SerializableMessage for Container {
    fn message_type_id(&self) -> &'static str { "Container" }
    fn as_any(&self) -> &dyn Any { self }
    
    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        let mut result = Vec::new();
        result.extend_from_slice(&(self.inner.len() as u32).to_le_bytes());
        
        for msg in &self.inner {
            let msg_bytes = msg.serialize()?;
            result.extend_from_slice(&(msg_bytes.len() as u32).to_le_bytes());
            result.extend_from_slice(&msg_bytes);
        }
        
        Ok(result)
    }
}
```

## Best Practices

### Type ID Naming

Use namespaced identifiers to avoid collisions:

```rust
// Good
"my_app::user::LoginRequest"
"my_app::chat::TextMessage"
"acme_corp::orders::CreateOrder"

// Avoid
"LoginRequest"  // Too generic
"TextMessage"   // Collision risk
```

### Version Handling

Include version information in your serialization format:

```rust
fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
    let mut result = Vec::new();
    result.push(1);  // Version byte
    // ... rest of serialization
    Ok(result)
}
```

### Error Handling

Provide descriptive error messages:

```rust
fn deserialize_example(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() < 4 {
        return Err(SerializationError::DeserializeFailed(
            format!("Expected at least 4 bytes, got {}", data.len())
        ));
    }
    // ...
}
```

### Thread Safety

The `MessageRegistry` is thread-safe and can be shared:

```rust
let registry = Arc::new(Mutex::new(MessageRegistry::new()));
// Clone the Arc when passing to actors
let handler = system.spawn(MessageHandler::new(Arc::clone(&registry)));
```

## Performance Considerations

1. **Serialization Overhead**: Custom serialization can be very efficient. For fixed-size types, serialization is essentially zero-copy.

2. **Wire Format**: The envelope format adds 8 bytes + type_id length overhead. For small messages, this can be significant.

3. **Registry Lookups**: The registry uses a `HashMap` with `RwLock`, providing fast concurrent reads.

4. **Zero-Copy Alternatives**: For local communication, consider passing the message directly without serialization.

## Comparison with Other Approaches

### vs. Serde

| Aspect | Trait-Based | Serde |
|--------|-------------|-------|
| **Flexibility** | Very high | Medium |
| **Boilerplate** | More | Less (with derive) |
| **Performance** | Can optimize per-type | Generally good |
| **Dynamic Dispatch** | Built-in | Requires extra work |
| **Format Support** | Custom only | Many formats |

### When to Use Trait-Based Serialization

- Distributed actor systems
- Custom wire protocols
- Performance-critical paths
- When you need fine-grained control
- When message types are dynamic

### When to Use Serde

- Standard formats (JSON, MessagePack, etc.)
- Rapid development
- Type-safe compile-time serialization
- When all types are known at compile time

## Examples

See `examples/serialization_example.rs` for a comprehensive demonstration.

## Future Enhancements

Possible future improvements to the serialization system:

1. **Compression**: Automatic compression for large messages
2. **Checksums**: Built-in integrity verification
3. **Schema Evolution**: Support for versioned message formats
4. **Codec Registry**: Pluggable compression/encryption codecs
5. **Async Serialization**: For very large messages
6. **Macro Support**: Reduce boilerplate with proc macros

## Related Modules

- [`distributed`]: Uses serialization for remote actors
- [`epmd`]: Network protocol implementation
- [`message`]: Base message types

## References

- [Erlang External Term Format](http://erlang.org/doc/apps/erts/erl_ext_dist.html)
- [Apache Thrift](https://thrift.apache.org/)
- [Protocol Buffers](https://developers.google.com/protocol-buffers)
