//! Trait-based message serialization for distributed actor systems.
//!
//! This module provides a flexible serialization framework that allows messages
//! to be serialized for network transmission, persistence, or other purposes.
//!
//! # Overview
//!
//! The serialization system consists of:
//! - [`SerializableMessage`]: Trait for messages that can be serialized
//! - [`MessageRegistry`]: Global registry mapping type IDs to deserializers
//! - [`SerializableEnvelope`]: Wire format for serialized messages
//!
//! # Examples
//!
//! ```rust
//! use joerl::serialization::{SerializableMessage, MessageRegistry, SerializationError};
//! use std::any::Any;
//!
//! #[derive(Debug, Clone, PartialEq)]
//! struct PingMessage {
//!     count: u32,
//! }
//!
//! impl SerializableMessage for PingMessage {
//!     fn message_type_id(&self) -> &'static str {
//!         "PingMessage"
//!     }
//!
//!     fn as_any(&self) -> &dyn Any {
//!         self
//!     }
//!
//!     fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
//!         Ok(self.count.to_le_bytes().to_vec())
//!     }
//! }
//!
//! // Deserializer function
//! fn deserialize_ping(data: &[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> {
//!     if data.len() != 4 {
//!         return Err(SerializationError::DeserializeFailed(
//!             "Invalid data length".to_string()
//!         ));
//!     }
//!     let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
//!     Ok(Box::new(PingMessage { count }))
//! }
//!
//! // Register the deserializer
//! let mut registry = MessageRegistry::new();
//! registry.register("PingMessage", Box::new(deserialize_ping));
//! ```

use crate::Message;
use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

/// Errors that can occur during serialization or deserialization.
#[derive(Debug, Clone)]
pub enum SerializationError {
    /// Failed to serialize a message.
    SerializeFailed(String),

    /// Failed to deserialize a message.
    DeserializeFailed(String),

    /// Message type is not registered in the registry.
    UnknownMessageType(String),

    /// Invalid data format.
    InvalidFormat(String),

    /// Type mismatch during deserialization.
    TypeMismatch { expected: String, found: String },
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializationError::SerializeFailed(msg) => {
                write!(f, "Serialization failed: {}", msg)
            }
            SerializationError::DeserializeFailed(msg) => {
                write!(f, "Deserialization failed: {}", msg)
            }
            SerializationError::UnknownMessageType(type_id) => {
                write!(f, "Unknown message type: {}", type_id)
            }
            SerializationError::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            SerializationError::TypeMismatch { expected, found } => {
                write!(f, "Type mismatch: expected {}, found {}", expected, found)
            }
        }
    }
}

impl std::error::Error for SerializationError {}

/// Trait for messages that can be serialized.
///
/// This trait extends the basic message functionality with serialization capabilities.
/// Each message type provides its own serialization logic and a unique type identifier.
///
/// # Type Safety
///
/// The type ID must be unique across all message types in your system. Use namespaced
/// identifiers to avoid collisions (e.g., "my_app::PingMessage").
///
/// # Examples
///
/// ```rust
/// use joerl::serialization::{SerializableMessage, SerializationError};
/// use std::any::Any;
///
/// struct CounterMsg {
///     value: i32,
/// }
///
/// impl SerializableMessage for CounterMsg {
///     fn message_type_id(&self) -> &'static str {
///         "CounterMsg"
///     }
///
///     fn as_any(&self) -> &dyn Any {
///         self
///     }
///
///     fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
///         Ok(self.value.to_le_bytes().to_vec())
///     }
/// }
/// ```
pub trait SerializableMessage: Any + Send + Sync {
    /// Returns a unique type identifier for this message type.
    ///
    /// This identifier is used to look up the appropriate deserializer
    /// when reconstructing messages from serialized data.
    ///
    /// # Important
    ///
    /// Type IDs must be:
    /// - Unique across all message types
    /// - Stable (don't change between versions)
    /// - Descriptive for debugging
    fn message_type_id(&self) -> &'static str;

    /// Returns a reference to self as `&dyn Any` for downcasting.
    ///
    /// This allows the message to be downcast to its concrete type
    /// after deserialization.
    fn as_any(&self) -> &dyn Any;

    /// Serializes the message into bytes.
    ///
    /// Implementations should ensure that the serialized format is:
    /// - Deterministic (same input always produces same output)
    /// - Versioned (handle format changes gracefully)
    /// - Compact (minimize overhead for network transmission)
    ///
    /// # Errors
    ///
    /// Returns `SerializationError::SerializeFailed` if serialization fails.
    fn serialize(&self) -> Result<Vec<u8>, SerializationError>;
}

/// Type alias for deserializer functions.
///
/// Deserializers take a byte slice and return a boxed SerializableMessage
/// or an error if deserialization fails.
pub type Deserializer =
    Box<dyn Fn(&[u8]) -> Result<Box<dyn SerializableMessage>, SerializationError> + Send + Sync>;

/// Registry mapping message type IDs to deserializers.
///
/// The registry provides a global mapping from type identifiers to functions
/// that can reconstruct messages from serialized bytes. This enables dynamic
/// deserialization without compile-time knowledge of all message types.
///
/// # Thread Safety
///
/// The registry is thread-safe and can be shared across actors and threads
/// using `Arc<MessageRegistry>`.
///
/// # Examples
///
/// ```rust
/// use joerl::serialization::{MessageRegistry, SerializableMessage, SerializationError};
/// use std::any::Any;
///
/// struct SimpleMsg(u32);
///
/// impl SerializableMessage for SimpleMsg {
///     fn message_type_id(&self) -> &'static str { "SimpleMsg" }
///     fn as_any(&self) -> &dyn Any { self }
///     fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
///         Ok(self.0.to_le_bytes().to_vec())
///     }
/// }
///
/// let mut registry = MessageRegistry::new();
/// registry.register("SimpleMsg", Box::new(|data| {
///     if data.len() != 4 {
///         return Err(SerializationError::InvalidFormat("Expected 4 bytes".into()));
///     }
///     let value = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
///     Ok(Box::new(SimpleMsg(value)))
/// }));
///
/// // Later, deserialize a message
/// let data = vec![42, 0, 0, 0];
/// let msg = registry.deserialize("SimpleMsg", &data).unwrap();
/// ```
#[derive(Clone)]
pub struct MessageRegistry {
    deserializers: Arc<RwLock<HashMap<String, Deserializer>>>,
}

impl MessageRegistry {
    /// Creates a new empty message registry.
    pub fn new() -> Self {
        Self {
            deserializers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a deserializer for a message type.
    ///
    /// # Arguments
    ///
    /// * `type_id` - Unique identifier for the message type
    /// * `deserializer` - Function to deserialize bytes into the message
    ///
    /// # Examples
    ///
    /// ```rust
    /// use joerl::serialization::{MessageRegistry, SerializableMessage, SerializationError};
    /// use std::any::Any;
    ///
    /// struct MyMsg(String);
    ///
    /// impl SerializableMessage for MyMsg {
    ///     fn message_type_id(&self) -> &'static str { "MyMsg" }
    ///     fn as_any(&self) -> &dyn Any { self }
    ///     fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
    ///         Ok(self.0.as_bytes().to_vec())
    ///     }
    /// }
    ///
    /// let mut registry = MessageRegistry::new();
    /// registry.register("MyMsg", Box::new(|data| {
    ///     let text = String::from_utf8(data.to_vec())
    ///         .map_err(|e| SerializationError::DeserializeFailed(e.to_string()))?;
    ///     Ok(Box::new(MyMsg(text)))
    /// }));
    /// ```
    pub fn register(&mut self, type_id: &str, deserializer: Deserializer) {
        self.deserializers
            .write()
            .unwrap()
            .insert(type_id.to_string(), deserializer);
    }

    /// Deserializes a message from bytes using the registered deserializer.
    ///
    /// # Arguments
    ///
    /// * `type_id` - The type identifier of the message
    /// * `data` - The serialized message bytes
    ///
    /// # Errors
    ///
    /// Returns `SerializationError::UnknownMessageType` if no deserializer
    /// is registered for the given type ID.
    ///
    /// Returns `SerializationError::DeserializeFailed` if deserialization fails.
    pub fn deserialize(
        &self,
        type_id: &str,
        data: &[u8],
    ) -> Result<Box<dyn SerializableMessage>, SerializationError> {
        let deserializers = self.deserializers.read().unwrap();
        let deserializer = deserializers
            .get(type_id)
            .ok_or_else(|| SerializationError::UnknownMessageType(type_id.to_string()))?;

        deserializer(data)
    }

    /// Returns true if a deserializer is registered for the given type ID.
    pub fn has_type(&self, type_id: &str) -> bool {
        self.deserializers.read().unwrap().contains_key(type_id)
    }

    /// Returns the number of registered message types.
    pub fn len(&self) -> usize {
        self.deserializers.read().unwrap().len()
    }

    /// Returns true if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.deserializers.read().unwrap().is_empty()
    }

    /// Clears all registered deserializers.
    pub fn clear(&mut self) {
        self.deserializers.write().unwrap().clear();
    }
}

impl Default for MessageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Envelope for serialized messages.
///
/// This structure represents a message in its serialized form, ready for
/// transmission over the network or storage to disk. It contains both the
/// type identifier and the serialized data.
///
/// # Wire Format
///
/// The envelope uses a simple format:
/// - Type ID (UTF-8 string with length prefix)
/// - Message data (opaque bytes)
///
/// # Examples
///
/// ```rust
/// use joerl::serialization::{SerializableEnvelope, MessageRegistry, SerializableMessage, SerializationError};
/// use std::any::Any;
///
/// struct TestMsg(u32);
///
/// impl SerializableMessage for TestMsg {
///     fn message_type_id(&self) -> &'static str { "TestMsg" }
///     fn as_any(&self) -> &dyn Any { self }
///     fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
///         Ok(self.0.to_le_bytes().to_vec())
///     }
/// }
///
/// // Wrap a message
/// let msg = TestMsg(42);
/// let envelope = SerializableEnvelope::wrap(&msg).unwrap();
///
/// // Convert to wire format
/// let wire_data = envelope.to_bytes();
///
/// // Reconstruct from wire format
/// let envelope2 = SerializableEnvelope::from_bytes(&wire_data).unwrap();
/// assert_eq!(envelope.type_id(), envelope2.type_id());
///
/// // Unwrap with registry
/// let mut registry = MessageRegistry::new();
/// registry.register("TestMsg", Box::new(|data| {
///     let value = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
///     Ok(Box::new(TestMsg(value)))
/// }));
///
/// let unwrapped = envelope2.unwrap(&registry).unwrap();
/// let test_msg = unwrapped.as_any().downcast_ref::<TestMsg>().unwrap();
/// assert_eq!(test_msg.0, 42);
/// ```
#[derive(Debug, Clone)]
pub struct SerializableEnvelope {
    type_id: String,
    data: Vec<u8>,
}

impl SerializableEnvelope {
    /// Wraps a serializable message in an envelope.
    ///
    /// # Errors
    ///
    /// Returns `SerializationError::SerializeFailed` if the message
    /// cannot be serialized.
    pub fn wrap(msg: &dyn SerializableMessage) -> Result<Self, SerializationError> {
        let type_id = msg.message_type_id().to_string();
        let data = msg.serialize()?;

        Ok(Self { type_id, data })
    }

    /// Returns the type ID of the wrapped message.
    pub fn type_id(&self) -> &str {
        &self.type_id
    }

    /// Returns the serialized message data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Unwraps the envelope, deserializing the message.
    ///
    /// # Arguments
    ///
    /// * `registry` - The message registry containing deserializers
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The message type is not registered
    /// - Deserialization fails
    pub fn unwrap(
        &self,
        registry: &MessageRegistry,
    ) -> Result<Box<dyn SerializableMessage>, SerializationError> {
        registry.deserialize(&self.type_id, &self.data)
    }

    /// Converts the envelope to a wire format (byte representation).
    ///
    /// The format is:
    /// - 4 bytes: type_id length (u32 little-endian)
    /// - N bytes: type_id (UTF-8)
    /// - 4 bytes: data length (u32 little-endian)
    /// - M bytes: data
    pub fn to_bytes(&self) -> Vec<u8> {
        let type_id_bytes = self.type_id.as_bytes();
        let type_id_len = type_id_bytes.len() as u32;
        let data_len = self.data.len() as u32;

        let mut result = Vec::with_capacity(8 + type_id_bytes.len() + self.data.len());
        result.extend_from_slice(&type_id_len.to_le_bytes());
        result.extend_from_slice(type_id_bytes);
        result.extend_from_slice(&data_len.to_le_bytes());
        result.extend_from_slice(&self.data);

        result
    }

    /// Reconstructs an envelope from wire format bytes.
    ///
    /// # Errors
    ///
    /// Returns `SerializationError::InvalidFormat` if the bytes don't
    /// represent a valid envelope.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SerializationError> {
        if bytes.len() < 8 {
            return Err(SerializationError::InvalidFormat(
                "Envelope too short".to_string(),
            ));
        }

        // Read type_id length
        let type_id_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if bytes.len() < 8 + type_id_len {
            return Err(SerializationError::InvalidFormat(
                "Invalid type_id length".to_string(),
            ));
        }

        // Read type_id
        let type_id_bytes = &bytes[4..4 + type_id_len];
        let type_id = String::from_utf8(type_id_bytes.to_vec())
            .map_err(|e| SerializationError::InvalidFormat(format!("Invalid UTF-8: {}", e)))?;

        // Read data length
        let data_len_offset = 4 + type_id_len;
        if bytes.len() < data_len_offset + 4 {
            return Err(SerializationError::InvalidFormat(
                "Missing data length".to_string(),
            ));
        }

        let data_len = u32::from_le_bytes([
            bytes[data_len_offset],
            bytes[data_len_offset + 1],
            bytes[data_len_offset + 2],
            bytes[data_len_offset + 3],
        ]) as usize;

        // Read data
        let data_offset = data_len_offset + 4;
        if bytes.len() < data_offset + data_len {
            return Err(SerializationError::InvalidFormat(
                "Invalid data length".to_string(),
            ));
        }

        let data = bytes[data_offset..data_offset + data_len].to_vec();

        Ok(Self { type_id, data })
    }

    /// Converts a SerializableMessage to a standard joerl Message.
    ///
    /// This allows serializable messages to be used with the regular
    /// actor system send operations.
    pub fn to_message(msg: Box<dyn SerializableMessage>) -> Message {
        Box::new(msg)
    }

    /// Attempts to convert a joerl Message to a SerializableMessage.
    ///
    /// Returns None if the message doesn't implement SerializableMessage.
    pub fn from_message(msg: &Message) -> Option<&dyn SerializableMessage> {
        msg.downcast_ref::<Box<dyn SerializableMessage>>()
            .map(|b| b.as_ref())
    }
}

/// Helper macro to implement SerializableMessage with less boilerplate.
///
/// # Examples
///
/// ```ignore
/// use joerl::impl_serializable;
///
/// #[derive(Debug, Clone)]
/// struct MyMessage {
///     value: i32,
/// }
///
/// impl_serializable!(MyMessage, "my_app::MyMessage", |msg: &MyMessage| {
///     Ok(msg.value.to_le_bytes().to_vec())
/// });
///
/// // Now MyMessage implements SerializableMessage
/// ```
#[macro_export]
macro_rules! impl_serializable {
    ($type:ty, $type_id:expr, $serialize_fn:expr) => {
        impl $crate::serialization::SerializableMessage for $type {
            fn message_type_id(&self) -> &'static str {
                $type_id
            }

            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }

            fn serialize(&self) -> Result<Vec<u8>, $crate::serialization::SerializationError> {
                let serialize: fn(
                    &Self,
                )
                    -> Result<Vec<u8>, $crate::serialization::SerializationError> = $serialize_fn;
                serialize(self)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestMessage {
        value: u32,
    }

    impl SerializableMessage for TestMessage {
        fn message_type_id(&self) -> &'static str {
            "TestMessage"
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
            Ok(self.value.to_le_bytes().to_vec())
        }
    }

    fn deserialize_test_message(
        data: &[u8],
    ) -> Result<Box<dyn SerializableMessage>, SerializationError> {
        if data.len() != 4 {
            return Err(SerializationError::DeserializeFailed(
                "Invalid length".to_string(),
            ));
        }
        let value = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        Ok(Box::new(TestMessage { value }))
    }

    #[test]
    fn test_message_serialization() {
        let msg = TestMessage { value: 42 };
        let data = msg.serialize().unwrap();
        assert_eq!(data, vec![42, 0, 0, 0]);
    }

    #[test]
    fn test_registry() {
        let mut registry = MessageRegistry::new();
        assert!(registry.is_empty());

        registry.register("TestMessage", Box::new(deserialize_test_message));
        assert_eq!(registry.len(), 1);
        assert!(registry.has_type("TestMessage"));
        assert!(!registry.has_type("UnknownType"));
    }

    #[test]
    fn test_registry_deserialize() {
        let mut registry = MessageRegistry::new();
        registry.register("TestMessage", Box::new(deserialize_test_message));

        let data = vec![42, 0, 0, 0];
        let msg = registry.deserialize("TestMessage", &data).unwrap();
        let test_msg = msg.as_any().downcast_ref::<TestMessage>().unwrap();
        assert_eq!(test_msg.value, 42);
    }

    #[test]
    fn test_registry_unknown_type() {
        let registry = MessageRegistry::new();
        let result = registry.deserialize("Unknown", &[]);
        assert!(matches!(
            result,
            Err(SerializationError::UnknownMessageType(_))
        ));
    }

    #[test]
    fn test_envelope_wrap_unwrap() {
        let msg = TestMessage { value: 123 };
        let envelope = SerializableEnvelope::wrap(&msg).unwrap();
        assert_eq!(envelope.type_id(), "TestMessage");

        let mut registry = MessageRegistry::new();
        registry.register("TestMessage", Box::new(deserialize_test_message));

        let unwrapped = envelope.unwrap(&registry).unwrap();
        let result = unwrapped.as_any().downcast_ref::<TestMessage>().unwrap();
        assert_eq!(result.value, 123);
    }

    #[test]
    fn test_envelope_to_from_bytes() {
        let msg = TestMessage { value: 999 };
        let envelope = SerializableEnvelope::wrap(&msg).unwrap();

        let bytes = envelope.to_bytes();
        let reconstructed = SerializableEnvelope::from_bytes(&bytes).unwrap();

        assert_eq!(envelope.type_id(), reconstructed.type_id());
        assert_eq!(envelope.data(), reconstructed.data());
    }

    #[test]
    fn test_envelope_from_bytes_invalid() {
        let result = SerializableEnvelope::from_bytes(&[1, 2, 3]);
        assert!(matches!(result, Err(SerializationError::InvalidFormat(_))));
    }

    #[test]
    fn test_registry_clone() {
        let mut registry = MessageRegistry::new();
        registry.register("TestMessage", Box::new(deserialize_test_message));

        let cloned = registry.clone();
        assert!(cloned.has_type("TestMessage"));
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = MessageRegistry::new();
        registry.register("TestMessage", Box::new(deserialize_test_message));
        assert_eq!(registry.len(), 1);

        registry.clear();
        assert!(registry.is_empty());
    }
}
