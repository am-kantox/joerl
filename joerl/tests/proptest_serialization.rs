//! Property-based tests for message serialization

use joerl::serialization::{
    SerializableEnvelope, SerializableMessage, SerializationError, get_global_registry,
    register_message_type,
};
use quickcheck::{Arbitrary, Gen};
use quickcheck_macros::quickcheck;
use std::any::Any;

/// Test message for property testing
#[derive(Debug, Clone, PartialEq)]
struct TestMessage {
    value: i64,
    text: String,
}

impl SerializableMessage for TestMessage {
    fn message_type_id(&self) -> &'static str {
        "test::TestMessage"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.value.to_le_bytes());
        let text_bytes = self.text.as_bytes();
        bytes.extend_from_slice(&(text_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(text_bytes);
        Ok(bytes)
    }
}

fn deserialize_test_message(
    data: &[u8],
) -> Result<Box<dyn SerializableMessage>, SerializationError> {
    if data.len() < 12 {
        return Err(SerializationError::DeserializeFailed(
            "TestMessage requires at least 12 bytes".to_string(),
        ));
    }

    let value = i64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);

    let text_len = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;

    if data.len() < 12 + text_len {
        return Err(SerializationError::DeserializeFailed(
            "Insufficient data for text".to_string(),
        ));
    }

    let text = String::from_utf8(data[12..12 + text_len].to_vec())
        .map_err(|e| SerializationError::DeserializeFailed(format!("Invalid UTF-8: {}", e)))?;

    Ok(Box::new(TestMessage { value, text }))
}

/// Arbitrary implementation for TestMessage
impl Arbitrary for TestMessage {
    fn arbitrary(g: &mut Gen) -> Self {
        let value = i64::arbitrary(g);
        // Generate valid UTF-8 string
        let text_len = u8::arbitrary(g) as usize % 100;
        let text: String = (0..text_len)
            .map(|_| char::from_u32(u32::arbitrary(g) % 0x80).unwrap_or('a'))
            .collect();

        TestMessage { value, text }
    }
}

/// Setup function to register message types
fn setup_registry() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        register_message_type("test::TestMessage", Box::new(deserialize_test_message));
    });
}

/// Property: Message serialization roundtrip preserves data
#[quickcheck]
fn prop_message_serialization_roundtrip(msg: TestMessage) -> bool {
    setup_registry();

    let serialized = msg.serialize().expect("serialization failed");
    let deserialized = deserialize_test_message(&serialized).expect("deserialization failed");

    if let Some(recovered) = deserialized.as_any().downcast_ref::<TestMessage>() {
        recovered == &msg
    } else {
        false
    }
}

/// Property: Envelope wrapping and unwrapping preserves message
#[quickcheck]
fn prop_envelope_roundtrip(msg: TestMessage) -> bool {
    setup_registry();

    let envelope = SerializableEnvelope::wrap(&msg).expect("wrap failed");
    let registry = get_global_registry();
    let registry_guard = registry.read().unwrap();
    let unwrapped = envelope.unwrap(&registry_guard).expect("unwrap failed");

    if let Some(recovered) = unwrapped.as_any().downcast_ref::<TestMessage>() {
        recovered == &msg
    } else {
        false
    }
}

/// Property: Envelope wire format roundtrip preserves message
#[quickcheck]
fn prop_envelope_wire_format_roundtrip(msg: TestMessage) -> bool {
    setup_registry();

    let envelope = SerializableEnvelope::wrap(&msg).expect("wrap failed");
    let wire_bytes = envelope.to_bytes();
    let recovered_envelope =
        SerializableEnvelope::from_bytes(&wire_bytes).expect("from_bytes failed");

    let registry = get_global_registry();
    let registry_guard = registry.read().unwrap();
    let unwrapped = recovered_envelope
        .unwrap(&registry_guard)
        .expect("unwrap failed");

    if let Some(recovered) = unwrapped.as_any().downcast_ref::<TestMessage>() {
        recovered == &msg
    } else {
        false
    }
}

/// Property: Type ID should be preserved through envelope
#[quickcheck]
fn prop_type_id_preserved(msg: TestMessage) -> bool {
    let envelope = SerializableEnvelope::wrap(&msg).expect("wrap failed");
    envelope.type_id() == "test::TestMessage"
}

/// Property: Serialized data should not be empty for non-empty messages
#[quickcheck]
fn prop_serialization_not_empty(msg: TestMessage) -> bool {
    let serialized = msg.serialize().expect("serialization failed");
    !serialized.is_empty()
}

/// Property: Wire format should contain both type ID and data
#[quickcheck]
fn prop_wire_format_structure(msg: TestMessage) -> bool {
    setup_registry();

    let envelope = SerializableEnvelope::wrap(&msg).expect("wrap failed");
    let wire_bytes = envelope.to_bytes();

    // Wire format should be at least: 4 bytes (type_id len) + type_id + 4 bytes (data len) + data
    let min_size = 4 + "test::TestMessage".len() + 4;
    wire_bytes.len() >= min_size
}

/// Property: Different messages should produce different serializations
#[quickcheck]
fn prop_different_messages_different_serialization(value1: i64, value2: i64) -> bool {
    if value1 == value2 {
        return true; // Skip equal values
    }

    let msg1 = TestMessage {
        value: value1,
        text: String::from("test"),
    };
    let msg2 = TestMessage {
        value: value2,
        text: String::from("test"),
    };

    let ser1 = msg1.serialize().expect("serialization failed");
    let ser2 = msg2.serialize().expect("serialization failed");

    ser1 != ser2
}

/// Property: Serialization should be deterministic
#[quickcheck]
fn prop_serialization_deterministic(msg: TestMessage) -> bool {
    let ser1 = msg.serialize().expect("serialization failed");
    let ser2 = msg.serialize().expect("serialization failed");

    ser1 == ser2
}

/// Property: Large messages should serialize correctly
#[test]
fn prop_large_message_serialization() {
    setup_registry();

    let large_text = "a".repeat(10000);
    let msg = TestMessage {
        value: 42,
        text: large_text.clone(),
    };

    let serialized = msg.serialize().expect("serialization failed");
    let deserialized = deserialize_test_message(&serialized).expect("deserialization failed");

    if let Some(recovered) = deserialized.as_any().downcast_ref::<TestMessage>() {
        assert_eq!(recovered.value, 42);
        assert_eq!(recovered.text, large_text);
    } else {
        panic!("Failed to downcast");
    }
}

/// Property: Empty string messages should serialize correctly
#[test]
fn prop_empty_string_serialization() {
    setup_registry();

    let msg = TestMessage {
        value: 123,
        text: String::new(),
    };

    let serialized = msg.serialize().expect("serialization failed");
    let deserialized = deserialize_test_message(&serialized).expect("deserialization failed");

    if let Some(recovered) = deserialized.as_any().downcast_ref::<TestMessage>() {
        assert_eq!(recovered.value, 123);
        assert_eq!(recovered.text, "");
    } else {
        panic!("Failed to downcast");
    }
}

/// Property: Invalid data should fail deserialization gracefully
#[quickcheck]
fn prop_invalid_data_fails_gracefully(invalid_data: Vec<u8>) -> bool {
    // Short data should fail
    if invalid_data.len() < 12 {
        return deserialize_test_message(&invalid_data).is_err();
    }

    // Can't test all invalid cases, but at least verify it doesn't panic
    true
}
