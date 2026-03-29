# Assertion Catalog Design

## Context

ChaosControl's SDK provides assertion functions (always, sometimes, reachable, unreachable, always_or_unreachable) that generate IDs via FNV-1a hash of message strings. The PropertyOracle in chaoscontrol-fault tracks assertion records keyed by these IDs but only discovers assertions when they fire, creating coverage gaps for unexercised code paths.

## Goals / Non-Goals

**Goals:**
- Compile-time registry of all assertions in guest binary
- Runtime transmission of catalog to VMM at startup
- Oracle pre-population for complete assertion tracking
- Coverage reporting for exercised and unexercised assertions

**Non-Goals:**
- Dynamic assertion registration during execution
- Catalog transmission during runtime (only at startup)
- Breaking backward compatibility with existing assertion API

## Decisions

### 1. Linkme Distributed Slice

**Choice:** Use `linkme` crate's `#[distributed_slice]` for compile-time registration

**Rationale:** Linkme provides zero-cost compile-time collection of static data across compilation units. No runtime overhead, works in no_std environments, and handles the complex linker section management automatically.

**Alternative:** Custom linker sections were considered but linkme abstracts away platform-specific complexities and provides a clean Rust API.

**Implementation:** 
```rust
use linkme::distributed_slice;

#[distributed_slice]
pub static ASSERTION_CATALOG: [CatalogEntry] = [..];

pub struct CatalogEntry {
    pub id: u32,
    pub message: &'static str,
    pub kind: AssertKind,
    pub file: &'static str,
    pub line: u32,
}
```

### 2. Catalog Transmission Protocol

**Choice:** New hypercall CMD_SEND_CATALOG with serialized catalog entries

**Rationale:** Reuses existing hypercall infrastructure. Sending at setup_complete time ensures catalog is available before any assertions fire. Serialization keeps protocol simple and extensible.

**Alternative:** Embedding catalog in guest binary metadata was considered but hypercall approach integrates cleanly with existing SDK patterns.

**Implementation:**
- Serialize catalog entries to bytes
- Send via hypercall during SDK setup phase
- VMM deserializes and forwards to PropertyOracle

### 3. Macro Registration Strategy

**Choice:** Assertion macros register catalog entry AND call function

**Rationale:** Preserves existing function API for non-macro usage while adding catalog registration transparently. Macro expansion handles both registration and execution atomically.

**Alternative:** Function-only approach would require manual catalog management and risk registration/execution mismatches.

**Implementation:**
```rust
macro_rules! always {
    ($msg:expr) => {{
        #[distributed_slice(ASSERTION_CATALOG)]
        static ENTRY: CatalogEntry = CatalogEntry {
            id: location_id!($msg),
            message: $msg,
            kind: AssertKind::Always,
            file: file!(),
            line: line!(),
        };
        crate::assert::always($msg)
    }};
}
```

### 4. Oracle Integration

**Choice:** Pre-populate PropertyOracle with catalog entries at VMM startup

**Rationale:** Ensures oracle knows about all assertions before execution begins. Allows distinction between never-executed and never-reached assertions.

**Alternative:** Post-execution analysis was considered but requires tracking execution coverage separately from assertion states.

**Implementation:**
- PropertyOracle accepts catalog during initialization
- Creates assertion records for all catalog entries
- Marks records as "unexercised" by default
- Updates to "exercised" when assertions fire

## Risks / Trade-offs

**Larger guest binaries** → Catalog entries add static data to binary. Mitigated by using &'static str (single copy of string literals) and minimal metadata per entry.

**Startup overhead** → Catalog serialization and transmission adds startup time. Mitigated by performing once at setup_complete rather than per assertion.

**Dependency on linkme** → Adds external dependency to chaoscontrol-sdk. Mitigated by linkme's stability and no_std compatibility.

**Backward compatibility** → Guests without catalog still work but miss unexercised assertion reporting. This is acceptable degradation for old binaries.