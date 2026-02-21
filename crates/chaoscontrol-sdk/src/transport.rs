//! Low-level transport: write to hypercall page + trigger via VMCALL.
//!
//! ## Full mode (default)
//!
//! Auto-detects whether we're inside a ChaosControl VM:
//! - **In VM**: uses shared memory page at [`HYPERCALL_PAGE_ADDR`] + `vmcall`
//! - **Outside VM**: logs to `CHAOSCONTROL_SDK_LOCAL_OUTPUT` file (JSON), or
//!   silently discards if the env var is not set.
//!
//! ## No-op mode (`default-features = false`)
//!
//! All transport functions are no-ops. Arguments are evaluated but
//! discarded. Zero runtime cost.
//!
//! [`HYPERCALL_PAGE_ADDR`]: chaoscontrol_protocol::HYPERCALL_PAGE_ADDR

// ═══════════════════════════════════════════════════════════════════════
//  Full mode: auto-detecting transport
// ═══════════════════════════════════════════════════════════════════════

/// Issue a hypercall with the given command, flags, id, and payload.
///
/// `json_details` is pre-serialized compact JSON bytes (e.g. `b"{}"`).
///
/// In VM mode: writes request to shared page, triggers vmcall, returns
/// host-written result and status.
///
/// In local/noop mode: logs to file or discards; returns `(0, 0)`.
#[cfg(feature = "full")]
pub(crate) fn hypercall(
    command: u8,
    flags: u8,
    id: u32,
    message: &str,
    json_details: &[u8],
) -> (u64, u8) {
    if let Some(page_ptr) = crate::internal::vm_page_ptr() {
        // ── VM transport ────────────────────────────────────────
        unsafe {
            let page = &mut *page_ptr;
            page.command = command;
            page.flags = flags;
            page.id = id;
            page.result = 0;
            page.status = 0;

            let payload_len =
                chaoscontrol_protocol::encode_payload(&mut page.payload, message, json_details)
                    .unwrap_or(0);
            page.payload_len = payload_len as u16;

            crate::internal::vm_trigger();

            (page.result, page.status)
        }
    } else {
        // ── Local / noop fallback ───────────────────────────────
        dispatch_local(command, flags, id, message, json_details);
        (0, 0)
    }
}

/// Issue a minimal hypercall (no payload) and return the result.
#[cfg(feature = "full")]
pub(crate) fn hypercall_simple(command: u8, id: u32) -> (u64, u8) {
    if let Some(page_ptr) = crate::internal::vm_page_ptr() {
        unsafe {
            let page = &mut *page_ptr;
            page.command = command;
            page.flags = 0;
            page.id = id;
            page.payload_len = 0;
            page.result = 0;
            page.status = 0;

            crate::internal::vm_trigger();

            (page.result, page.status)
        }
    } else {
        // Random fallback: return local random
        use chaoscontrol_protocol::{CMD_RANDOM_CHOICE, CMD_RANDOM_GET};
        match command {
            CMD_RANDOM_GET => (crate::internal::local_random_u64(), 0),
            CMD_RANDOM_CHOICE => {
                let n = id as u64;
                if n <= 1 {
                    (0, 0)
                } else {
                    (crate::internal::local_random_u64() % n, 0)
                }
            }
            _ => (0, 0),
        }
    }
}

/// Dispatch assertion/lifecycle events to local output when outside a VM.
#[cfg(feature = "full")]
fn dispatch_local(command: u8, flags: u8, id: u32, message: &str, json_details: &[u8]) {
    use chaoscontrol_protocol::*;

    let condition = flags & 0x01 != 0;
    use crate::internal::LocalAssert;

    match command {
        CMD_ASSERT_CATALOG => {
            crate::internal::local_emit_assert(&LocalAssert {
                assert_type: "always",
                hit: false,
                condition,
                message,
                id,
                json_details,
            });
        }
        CMD_ASSERT_ALWAYS => {
            crate::internal::local_emit_assert(&LocalAssert {
                assert_type: "always",
                hit: true,
                condition,
                message,
                id,
                json_details,
            });
        }
        CMD_ASSERT_SOMETIMES => {
            crate::internal::local_emit_assert(&LocalAssert {
                assert_type: "sometimes",
                hit: true,
                condition,
                message,
                id,
                json_details,
            });
        }
        CMD_ASSERT_REACHABLE => {
            crate::internal::local_emit_assert(&LocalAssert {
                assert_type: "reachability",
                hit: true,
                condition: true,
                message,
                id,
                json_details,
            });
        }
        CMD_ASSERT_UNREACHABLE => {
            crate::internal::local_emit_assert(&LocalAssert {
                assert_type: "reachability",
                hit: true,
                condition: false,
                message,
                id,
                json_details,
            });
        }
        CMD_LIFECYCLE_SETUP_COMPLETE | CMD_LIFECYCLE_SEND_EVENT => {
            crate::internal::local_emit_lifecycle(message, json_details);
        }
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  No-op mode: stubs when `full` is disabled
// ═══════════════════════════════════════════════════════════════════════

/// No-op hypercall — evaluates args but does nothing.
#[cfg(not(feature = "full"))]
pub(crate) fn hypercall(
    _command: u8,
    _flags: u8,
    _id: u32,
    _message: &str,
    _json_details: &[u8],
) -> (u64, u8) {
    (0, 0)
}

/// No-op hypercall_simple — evaluates args but does nothing.
#[cfg(not(feature = "full"))]
pub(crate) fn hypercall_simple(_command: u8, _id: u32) -> (u64, u8) {
    (0, 0)
}

// ═══════════════════════════════════════════════════════════════════════
//  Compile-time transport validation
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use chaoscontrol_protocol::PAYLOAD_MAX;

    #[test]
    fn payload_max_fits_in_page() {
        const { assert!(PAYLOAD_MAX > 0) };
        const { assert!(PAYLOAD_MAX <= 4096) };
    }
}
