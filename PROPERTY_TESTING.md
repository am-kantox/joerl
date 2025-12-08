# Property-Based Testing in joerl

joerl uses property-based testing with [QuickCheck](https://github.com/BurntSushi/quickcheck) to verify correctness properties of the library. This document describes the property testing approach and how to use it.

## What is Property-Based Testing?

Property-based testing generates random test cases to verify that certain properties hold true for all inputs. Instead of writing individual test cases, you define properties that should always be true, and the testing framework generates hundreds of random test cases to try to find counterexamples.

### Benefits

- **Comprehensive Coverage**: Tests many more cases than hand-written tests
- **Edge Case Discovery**: Finds edge cases you might not think of
- **Specification as Code**: Properties serve as executable specifications
- **Regression Prevention**: Random seeds can be saved to reproduce failures

## Running Property Tests

### Run All Property Tests

```bash
cargo test --tests proptest
```

### Run Specific Property Test Suites

```bash
# Pid property tests
cargo test --test proptest_pid

# Serialization property tests  
cargo test --test proptest_serialization

# EPMD protocol property tests
cargo test --test proptest_epmd
```

### Run Individual Property Test

```bash
cargo test --test proptest_pid prop_pid_serialization_roundtrip
```

### Adjust Number of Test Cases

By default, QuickCheck runs 100 random test cases per property. To run more:

```bash
QUICKCHECK_TESTS=1000 cargo test --test proptest_pid
```

## Property Test Suites

### 1. Pid Properties (`tests/proptest_pid.rs`)

Tests for process identifiers:

- **Serialization Roundtrip**: Pids serialize and deserialize correctly
  - JSON serialization preserves data
  - Bincode serialization preserves data
  
- **Node Semantics**:
  - Local Pids always have `node == 0`
  - Remote Pids never have `node == 0`
  - `is_local()` returns correct value

- **Display Format**: Pids display as `<node.id.0>`

- **Equality**:
  - Same node and id → equal
  - Different nodes → not equal
  - Different ids → not equal

- **Cloning**: Cloned Pids are equal to originals

**Example**:
```rust
/// Property: Serialization roundtrip should preserve Pid
#[quickcheck]
fn prop_pid_serialization_roundtrip(arb: ArbitraryPid) -> bool {
    let pid: Pid = arb.clone().into();
    let serialized = serde_json::to_string(&pid).unwrap();
    let deserialized: Pid = serde_json::from_str(&serialized).unwrap();
    
    // Check preservation based on node type
    if arb.node == 0 {
        pid.node() == deserialized.node()
    } else {
        pid.node() == deserialized.node() && pid.id() == deserialized.id()
    }
}
```

### 2. Message Serialization Properties (`tests/proptest_serialization.rs`)

Tests for the `SerializableMessage` trait and envelope system:

- **Message Roundtrip**: Message serialization is lossless
- **Envelope Wrapping**: Envelope wrapping/unwrapping preserves message
- **Wire Format**: Wire format roundtrip preserves message
- **Type ID Preservation**: Type IDs are preserved through envelope
- **Determinism**: Serialization is deterministic (same input → same output)
- **Uniqueness**: Different messages → different serializations
- **Edge Cases**:
  - Empty strings serialize correctly
  - Large messages (10KB+) serialize correctly
  - Invalid data fails gracefully

**Example**:
```rust
/// Property: Message serialization roundtrip preserves data
#[quickcheck]
fn prop_message_serialization_roundtrip(msg: TestMessage) -> bool {
    setup_registry();
    
    let serialized = msg.serialize().expect("serialization failed");
    let deserialized = deserialize_test_message(&serialized)
        .expect("deserialization failed");
    
    if let Some(recovered) = deserialized.as_any().downcast_ref::<TestMessage>() {
        recovered == &msg
    } else {
        false
    }
}
```

### 3. EPMD Protocol Properties (`tests/proptest_epmd.rs`)

Tests for the Erlang Port Mapper Daemon protocol:

- **Protocol Serialization**: All EPMD messages serialize/deserialize correctly
  - Register, Unregister, Lookup, ListNodes
  - Ping/Pong, KeepAlive
  
- **NodeInfo Properties**:
  - Address format is `host:port`
  - Node names are non-empty
  - Port numbers are valid `u16`
  
- **Determinism**: Serialization is deterministic
- **Uniqueness**: Different nodes → different serializations

**Example**:
```rust
/// Property: NodeInfo serialization roundtrip with bincode
#[quickcheck]
fn prop_node_info_bincode_roundtrip(node: NodeInfo) -> bool {
    let serialized = bincode::serialize(&node).unwrap();
    let deserialized: NodeInfo = bincode::deserialize(&serialized).unwrap();
    
    node.name == deserialized.name
        && node.host == deserialized.host
        && node.port == deserialized.port
}
```

## Writing New Property Tests

### 1. Create Arbitrary Implementation

For custom types, implement `quickcheck::Arbitrary`:

```rust
use quickcheck::{Arbitrary, Gen};

impl Arbitrary for MyType {
    fn arbitrary(g: &mut Gen) -> Self {
        MyType {
            field1: u32::arbitrary(g),
            field2: String::arbitrary(g),
        }
    }
}
```

### 2. Write Property Test

Use the `#[quickcheck]` attribute:

```rust
use quickcheck_macros::quickcheck;

#[quickcheck]
fn prop_my_property(input: MyType) -> bool {
    // Property that should always be true
    let result = my_function(&input);
    result.satisfies_property()
}
```

### 3. Handle Edge Cases

Skip invalid inputs rather than fail:

```rust
#[quickcheck]
fn prop_division(numerator: i32, denominator: i32) -> bool {
    if denominator == 0 {
        return true; // Skip division by zero
    }
    
    let result = numerator / denominator;
    result * denominator == numerator // May not hold due to rounding!
}
```

## Property Testing Best Practices

### 1. Properties Should Be Simple

Good property:
```rust
#[quickcheck]
fn prop_reverse_twice(xs: Vec<i32>) -> bool {
    let reversed_twice: Vec<_> = xs.iter().rev().rev().collect();
    xs.iter().eq(reversed_twice.iter())
}
```

### 2. Test Invariants, Not Implementations

Good (tests invariant):
```rust
#[quickcheck]
fn prop_sort_preserves_length(xs: Vec<i32>) -> bool {
    let mut sorted = xs.clone();
    sorted.sort();
    sorted.len() == xs.len()
}
```

Bad (tests implementation detail):
```rust
#[quickcheck]
fn prop_sort_uses_quicksort(xs: Vec<i32>) -> bool {
    // Don't test algorithm choice!
    false
}
```

### 3. Use Shrinking Effectively

QuickCheck automatically shrinks failing test cases to find minimal counterexamples. Make sure your `Arbitrary` implementation shrinks well:

```rust
impl Arbitrary for MyType {
    fn arbitrary(g: &mut Gen) -> Self {
        // Generate
    }
    
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        // Return simpler versions for minimal counterexample
        Box::new(std::iter::empty())
    }
}
```

### 4. Test Roundtrip Properties

Roundtrips are excellent properties:

- Serialization: `deserialize(serialize(x)) == x`
- Encoding: `decode(encode(x)) == x`
- Compression: `decompress(compress(x)) == x`

### 5. Test Algebraic Properties

For mathematical operations:

- **Associativity**: `(a + b) + c == a + (b + c)`
- **Commutativity**: `a + b == b + a`
- **Identity**: `x + 0 == x`
- **Inverse**: `x + (-x) == 0`

## Common Property Patterns

### Roundtrip Property

```rust
#[quickcheck]
fn prop_encode_decode_roundtrip(data: Vec<u8>) -> bool {
    let encoded = encode(&data);
    let decoded = decode(&encoded).unwrap();
    decoded == data
}
```

### Idempotence Property

```rust
#[quickcheck]
fn prop_normalize_idempotent(s: String) -> bool {
    let once = normalize(&s);
    let twice = normalize(&once);
    once == twice
}
```

### Commutative Property

```rust
#[quickcheck]
fn prop_addition_commutative(a: i32, b: i32) -> bool {
    a.saturating_add(b) == b.saturating_add(a)
}
```

### Invariant Property

```rust
#[quickcheck]
fn prop_set_maintains_uniqueness(items: Vec<i32>) -> bool {
    let set: HashSet<_> = items.into_iter().collect();
    set.len() <= items.len() // Set has at most as many items
}
```

## Integration with CI

Property tests run automatically in CI with the rest of the test suite:

```bash
cargo test  # Runs all tests including property tests
```

To run with more iterations in CI:

```yaml
# .github/workflows/test.yml
- name: Run property tests
  run: QUICKCHECK_TESTS=1000 cargo test --tests proptest
```

## Debugging Property Test Failures

### 1. Reproduce with Seed

QuickCheck prints the seed for failing tests:

```
thread 'prop_my_test' panicked at 'assertion failed'
QuickCheck: seed=0x1234567890abcdef
```

Reproduce with:

```rust
use quickcheck::QuickCheck;

#[test]
fn reproduce_failure() {
    QuickCheck::new()
        .gen(quickcheck::Gen::new(0x1234567890abcdef))
        .quickcheck(prop_my_test as fn() -> bool);
}
```

### 2. Examine Shrunk Input

QuickCheck shows the minimal failing case:

```
[quickcheck] TEST FAILED. Arguments: (42, "minimal")
```

This is the simplest input that causes failure.

### 3. Add Logging

```rust
#[quickcheck]
fn prop_debug(x: i32) -> bool {
    eprintln!("Testing with x = {}", x);
    my_function(x)
}
```

Run with `--nocapture` to see logs:

```bash
cargo test prop_debug -- --nocapture
```

## Further Reading

- [QuickCheck Documentation](https://docs.rs/quickcheck/)
- [QuickCheck Paper](https://www.cs.tufts.edu/~nr/cs257/archive/john-hughes/quick.pdf)
- [Property-Based Testing with PropEr, Erlang and Elixir](https://propertesting.com/)

## Summary

Property-based testing in joerl:

- ✅ Comprehensive coverage of core data structures
- ✅ Serialization correctness verified
- ✅ Protocol implementations tested
- ✅ Integration with standard test suite
- ✅ CI-ready with configurable iterations

To add more property tests, follow the patterns in existing test suites and use the QuickCheck patterns described above.
