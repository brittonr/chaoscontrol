# Design: Assertion JSON Details

## Architecture

### Helper API Design

New module `chaoscontrol_sdk::assert::details` with builder functions:

```rust
// Builder functions for common assertion patterns
pub fn node_details(id: impl Into<String>, term: Option<u64>, role: Option<&str>) -> Value
pub fn log_details(index: u64, term: Option<u64>, length: Option<usize>) -> Value  
pub fn network_details(from: impl Into<String>, to: impl Into<String>, delivered: bool) -> Value
pub fn fault_details(fault_type: &str, target: impl Into<String>) -> Value

// Extensible builder for custom details
pub fn custom_details() -> DetailsBuilder
```

### Standard Keys

Constants for consistent field naming:

```rust
pub mod keys {
    pub const NODE_ID: &str = "node_id";
    pub const TERM: &str = "term"; 
    pub const ROLE: &str = "role";
    pub const LOG_INDEX: &str = "log_index";
    pub const LOG_LENGTH: &str = "log_length";
    pub const COMMIT_INDEX: &str = "commit_index";
    pub const PEER_ID: &str = "peer_id";
    pub const MESSAGE_TYPE: &str = "message_type";
    pub const FAULT_TYPE: &str = "fault_type";
    pub const TARGET: &str = "target";
    pub const FROM: &str = "from";
    pub const TO: &str = "to"; 
    pub const DELIVERED: &str = "delivered";
}
```

## Oracle Integration

### Fault Storage

Extend `AssertionEvent` in chaoscontrol-fault:

```rust
pub struct AssertionEvent {
    pub kind: AssertionKind,
    pub message: String,
    pub details: serde_json::Value, // Already exists
    // ... existing fields
}
```

### Triage Reports

Extend `TriageReport` to include assertion details:

```rust
pub struct AssertionFailure {
    pub message: String,
    pub details: serde_json::Value,
    pub first_occurrence: LogicalTime,
    pub count: usize,
}

impl TriageReport {
    pub fn assertion_details(&self) -> &[AssertionFailure] { ... }
}
```

### Display Formatting

Update `format_report()` in chaoscontrol-replay to show assertion context:

```
Failed Assertions:
  ✗ Node n1 should be leader (occurred 3 times)
    Details: {"node_id": "n1", "term": 5, "role": "candidate"}
    First: @T123
```

## Migration Strategy

### Call Site Updates

Transform existing patterns:

```rust
// Before
assert_eventually(
    always!("leader election"), 
    &json!({"peer": peer_id, "election_term": term})
);

// After  
assert_eventually(
    always!("leader election"),
    &node_details(peer_id, Some(term), Some("candidate"))
);
```

### Backward Compatibility

Raw JSON usage continues to work:
```rust
// Still supported
assert_always(msg, &json!({"custom": "data"}));
```

## Implementation Phases

1. **SDK Module**: Add details module with helpers and constants
2. **Oracle Storage**: Ensure assertion details are properly stored (already works)
3. **Display Integration**: Update triage formatting to show details
4. **Call Site Migration**: Update existing assertions in guest crates
5. **Documentation**: Update assertion best practices

## Testing Strategy

- Unit tests for detail builder functions
- Integration tests verifying oracle stores details correctly
- Snapshot tests for triage report formatting
- Guest assertion tests using new helpers