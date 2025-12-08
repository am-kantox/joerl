//! Property-based tests for EPMD protocol

use joerl::epmd::{EpmdMessage, NodeInfo, NodeMetadata};
use quickcheck::{Arbitrary, Gen};
use quickcheck_macros::quickcheck;
use std::collections::HashMap;

/// Newtype wrapper for NodeMetadata to implement Arbitrary
#[derive(Debug, Clone)]
struct ArbNodeMetadata(NodeMetadata);

impl Arbitrary for ArbNodeMetadata {
    fn arbitrary(g: &mut Gen) -> Self {
        let protocol_version = u32::arbitrary(g) % 10; // Reasonable version range
        let cap_count = u8::arbitrary(g) % 5; // Small number of capabilities
        let capabilities: Vec<String> = (0..cap_count).map(|i| format!("cap_{}", i)).collect();

        let attr_count = u8::arbitrary(g) % 3;
        let mut attributes = HashMap::new();
        for i in 0..attr_count {
            attributes.insert(format!("key_{}", i), format!("value_{}", i));
        }

        ArbNodeMetadata(NodeMetadata {
            protocol_version,
            capabilities,
            attributes,
        })
    }
}

/// Newtype wrapper for NodeInfo to implement Arbitrary
#[derive(Debug, Clone)]
struct ArbNodeInfo(NodeInfo);

impl Arbitrary for ArbNodeInfo {
    fn arbitrary(g: &mut Gen) -> Self {
        // Generate valid node name (alphanumeric + underscore)
        let name_len = (u8::arbitrary(g) % 20) + 1;
        let name: String = (0..name_len)
            .map(|_| {
                let chars = b"abcdefghijklmnopqrstuvwxyz0123456789_";
                chars[(u8::arbitrary(g) as usize) % chars.len()] as char
            })
            .collect();

        // Generate valid hostname
        let host = format!("host_{}.example.com", u16::arbitrary(g) % 1000);

        let port = u16::arbitrary(g);
        let ArbNodeMetadata(metadata) = ArbNodeMetadata::arbitrary(g);

        ArbNodeInfo(NodeInfo {
            name,
            host,
            port,
            metadata,
        })
    }
}

/// Property: NodeInfo serialization roundtrip with bincode
#[quickcheck]
fn prop_node_info_bincode_roundtrip(ArbNodeInfo(node): ArbNodeInfo) -> bool {
    let serialized = bincode::serialize(&node).expect("serialization failed");
    let deserialized: NodeInfo = bincode::deserialize(&serialized).expect("deserialization failed");

    node.name == deserialized.name
        && node.host == deserialized.host
        && node.port == deserialized.port
        && node.metadata.protocol_version == deserialized.metadata.protocol_version
}

/// Property: NodeInfo address format should be host:port
#[quickcheck]
fn prop_node_info_address_format(ArbNodeInfo(node): ArbNodeInfo) -> bool {
    let address = node.address();
    address.contains(':') && address.starts_with(&node.host)
}

/// Property: EpmdMessage::Register serialization roundtrip
#[quickcheck]
fn prop_register_message_roundtrip(ArbNodeInfo(node): ArbNodeInfo) -> bool {
    let msg = EpmdMessage::Register {
        name: node.name.clone(),
        host: node.host.clone(),
        port: node.port,
        metadata: node.metadata.clone(),
    };

    let serialized = bincode::serialize(&msg).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");

    matches!(deserialized, EpmdMessage::Register { .. })
}

/// Property: EpmdMessage::Lookup serialization roundtrip
#[quickcheck]
fn prop_lookup_message_roundtrip(name: String) -> bool {
    if name.is_empty() {
        return true; // Skip empty names
    }

    let msg = EpmdMessage::Lookup { name: name.clone() };

    let serialized = bincode::serialize(&msg).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");

    if let EpmdMessage::Lookup {
        name: recovered_name,
    } = deserialized
    {
        recovered_name == name
    } else {
        false
    }
}

/// Property: EpmdMessage::RegisterOk serialization roundtrip
#[test]
fn prop_register_ok_roundtrip() {
    let response = EpmdMessage::RegisterOk;
    let serialized = bincode::serialize(&response).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");

    assert!(matches!(deserialized, EpmdMessage::RegisterOk));
}

/// Property: EpmdMessage::LookupResult serialization roundtrip
#[quickcheck]
fn prop_lookup_result_roundtrip(maybe_node: Option<ArbNodeInfo>) -> bool {
    let node = maybe_node.map(|ArbNodeInfo(n)| n);
    let response = EpmdMessage::LookupResult { node };

    let serialized = bincode::serialize(&response).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");

    matches!(deserialized, EpmdMessage::LookupResult { .. })
}

/// Property: EpmdMessage::NodeList serialization roundtrip
#[quickcheck]
fn prop_node_list_roundtrip(arb_nodes: Vec<ArbNodeInfo>) -> bool {
    // Limit size to avoid huge test data
    let nodes: Vec<_> = arb_nodes
        .into_iter()
        .take(10)
        .map(|ArbNodeInfo(n)| n)
        .collect();

    let response = EpmdMessage::NodeList {
        nodes: nodes.clone(),
    };

    let serialized = bincode::serialize(&response).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");

    if let EpmdMessage::NodeList { nodes: recovered } = deserialized {
        recovered.len() == nodes.len()
    } else {
        false
    }
}

/// Property: EpmdMessage::Ping and Pong roundtrip
#[test]
fn prop_ping_pong_roundtrip() {
    let ping = EpmdMessage::Ping;
    let serialized = bincode::serialize(&ping).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");
    assert!(matches!(deserialized, EpmdMessage::Ping));

    let pong = EpmdMessage::Pong;
    let serialized = bincode::serialize(&pong).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");
    assert!(matches!(deserialized, EpmdMessage::Pong));
}

/// Property: EpmdMessage::KeepAlive serialization roundtrip
#[quickcheck]
fn prop_keep_alive_roundtrip(name: String) -> bool {
    if name.is_empty() {
        return true; // Skip empty names
    }

    let msg = EpmdMessage::KeepAlive { name: name.clone() };

    let serialized = bincode::serialize(&msg).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");

    if let EpmdMessage::KeepAlive {
        name: recovered_name,
    } = deserialized
    {
        recovered_name == name
    } else {
        false
    }
}

/// Property: EpmdMessage::Unregister serialization roundtrip
#[quickcheck]
fn prop_unregister_roundtrip(name: String) -> bool {
    if name.is_empty() {
        return true;
    }

    let msg = EpmdMessage::Unregister { name: name.clone() };

    let serialized = bincode::serialize(&msg).expect("serialization failed");
    let deserialized: EpmdMessage =
        bincode::deserialize(&serialized).expect("deserialization failed");

    if let EpmdMessage::Unregister {
        name: recovered_name,
    } = deserialized
    {
        recovered_name == name
    } else {
        false
    }
}

/// Property: NodeMetadata with empty collections should serialize
#[test]
fn prop_empty_metadata_serialization() {
    let metadata = NodeMetadata {
        protocol_version: 1,
        capabilities: vec![],
        attributes: HashMap::new(),
    };

    let serialized = bincode::serialize(&metadata).expect("serialization failed");
    let deserialized: NodeMetadata =
        bincode::deserialize(&serialized).expect("deserialization failed");

    assert_eq!(deserialized.protocol_version, 1);
    assert!(deserialized.capabilities.is_empty());
    assert!(deserialized.attributes.is_empty());
}

/// Property: Port numbers should be valid u16
#[quickcheck]
fn prop_valid_port_numbers(port: u16) -> bool {
    let node = NodeInfo {
        name: String::from("test"),
        host: String::from("localhost"),
        port,
        metadata: NodeMetadata {
            protocol_version: 1,
            capabilities: vec![],
            attributes: HashMap::new(),
        },
    };

    node.port == port
}

/// Property: Node names should not be empty in valid NodeInfo
#[quickcheck]
fn prop_node_names_not_empty(ArbNodeInfo(node): ArbNodeInfo) -> bool {
    !node.name.is_empty()
}

/// Property: Serialization should be deterministic
#[quickcheck]
fn prop_serialization_deterministic(ArbNodeInfo(node): ArbNodeInfo) -> bool {
    let ser1 = bincode::serialize(&node).expect("serialization failed");
    let ser2 = bincode::serialize(&node).expect("serialization failed");

    ser1 == ser2
}

/// Property: Different nodes should produce different serializations
#[quickcheck]
fn prop_different_nodes_different_serialization(port1: u16, port2: u16) -> bool {
    if port1 == port2 {
        return true; // Skip equal ports
    }

    let node1 = NodeInfo {
        name: String::from("node1"),
        host: String::from("localhost"),
        port: port1,
        metadata: NodeMetadata {
            protocol_version: 1,
            capabilities: vec![],
            attributes: HashMap::new(),
        },
    };

    let node2 = NodeInfo {
        name: String::from("node1"),
        host: String::from("localhost"),
        port: port2,
        metadata: NodeMetadata {
            protocol_version: 1,
            capabilities: vec![],
            attributes: HashMap::new(),
        },
    };

    let ser1 = bincode::serialize(&node1).expect("serialization failed");
    let ser2 = bincode::serialize(&node2).expect("serialization failed");

    ser1 != ser2
}
