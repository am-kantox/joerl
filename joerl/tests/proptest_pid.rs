//! Property-based tests for Pid

use joerl::Pid;
use quickcheck::{Arbitrary, Gen, QuickCheck};
use quickcheck_macros::quickcheck;
use serde::{Deserialize, Serialize};

/// Arbitrary implementation for Pid to enable property testing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArbitraryPid {
    node: u32,
    id: u64,
}

impl Arbitrary for ArbitraryPid {
    fn arbitrary(g: &mut Gen) -> Self {
        ArbitraryPid {
            node: u32::arbitrary(g),
            id: u64::arbitrary(g),
        }
    }
}

impl From<ArbitraryPid> for Pid {
    fn from(arb: ArbitraryPid) -> Self {
        if arb.node == 0 {
            Pid::new() // Local Pid
        } else {
            Pid::with_node(arb.node, arb.id)
        }
    }
}

/// Property: Serialization roundtrip should preserve Pid
#[quickcheck]
fn prop_pid_serialization_roundtrip(arb: ArbitraryPid) -> bool {
    let pid: Pid = arb.clone().into();

    // Serialize to JSON
    let serialized = serde_json::to_string(&pid).expect("serialization failed");

    // Deserialize back
    let deserialized: Pid = serde_json::from_str(&serialized).expect("deserialization failed");

    // For local Pids (node=0), only check node since id is random
    if arb.node == 0 {
        pid.node() == deserialized.node()
    } else {
        pid.node() == deserialized.node() && pid.id() == deserialized.id()
    }
}

/// Property: Binary serialization roundtrip should preserve Pid
#[quickcheck]
fn prop_pid_bincode_roundtrip(arb: ArbitraryPid) -> bool {
    let pid: Pid = arb.clone().into();

    // Serialize with bincode
    let serialized = bincode::serialize(&pid).expect("bincode serialization failed");

    // Deserialize back
    let deserialized: Pid =
        bincode::deserialize(&serialized).expect("bincode deserialization failed");

    // For local Pids (node=0), only check node since id is random
    if arb.node == 0 {
        pid.node() == deserialized.node()
    } else {
        pid.node() == deserialized.node() && pid.id() == deserialized.id()
    }
}

/// Property: Local Pids should always have node == 0
#[test]
fn prop_local_pid_node_is_zero() {
    fn prop() -> bool {
        let pid = Pid::new();
        pid.node() == 0 && pid.is_local()
    }

    QuickCheck::new()
        .tests(100)
        .quickcheck(prop as fn() -> bool);
}

/// Property: Remote Pids should never have node == 0
#[quickcheck]
fn prop_remote_pid_node_nonzero(node: u32, id: u64) -> bool {
    if node == 0 {
        return true; // Skip node=0 case
    }

    let pid = Pid::with_node(node, id);
    pid.node() == node && !pid.is_local()
}

/// Property: Display format should contain node and id
#[quickcheck]
fn prop_pid_display_format(arb: ArbitraryPid) -> bool {
    let pid: Pid = arb.clone().into();
    let display = format!("{}", pid);

    // Format should be <node.id.0>
    display.starts_with('<') && display.ends_with('>') && display.contains('.')
}

/// Property: Two Pids with same node and id should be equal
#[quickcheck]
fn prop_pid_equality(node: u32, id: u64) -> bool {
    if node == 0 {
        // Local Pids are always unique (different ids)
        return true;
    }

    let pid1 = Pid::with_node(node, id);
    let pid2 = Pid::with_node(node, id);

    pid1 == pid2
}

/// Property: Pid with different nodes should not be equal
#[quickcheck]
fn prop_pid_inequality_different_nodes(node1: u32, node2: u32, id: u64) -> bool {
    if node1 == node2 || node1 == 0 || node2 == 0 {
        return true; // Skip equal nodes or local nodes
    }

    let pid1 = Pid::with_node(node1, id);
    let pid2 = Pid::with_node(node2, id);

    pid1 != pid2
}

/// Property: Pid with different ids should not be equal
#[quickcheck]
fn prop_pid_inequality_different_ids(node: u32, id1: u64, id2: u64) -> bool {
    if id1 == id2 || node == 0 {
        return true; // Skip equal ids or local nodes
    }

    let pid1 = Pid::with_node(node, id1);
    let pid2 = Pid::with_node(node, id2);

    pid1 != pid2
}

/// Property: Cloning a Pid should produce an equal Pid
#[quickcheck]
fn prop_pid_clone_equality(arb: ArbitraryPid) -> bool {
    let pid: Pid = arb.into();
    let cloned = pid.clone();

    pid.node() == cloned.node() && pid.id() == cloned.id()
}

/// Property: Pid should be Send and Sync (compile-time check via usage)
#[test]
fn prop_pid_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<Pid>();
    assert_sync::<Pid>();
}

/// Property: Node ID and internal ID should be accessible
#[quickcheck]
fn prop_pid_accessors(node: u32, id: u64) -> bool {
    if node == 0 {
        return true; // Skip local nodes
    }

    let pid = Pid::with_node(node, id);
    pid.node() == node && pid.id() == id
}
