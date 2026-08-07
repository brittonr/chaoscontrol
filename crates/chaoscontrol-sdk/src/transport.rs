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

#[cfg(feature = "full")]
pub(crate) fn hypercall_raw(command: u8, flags: u8, id: u32, payload: &[u8]) -> (u64, u8) {
    if payload.len() > chaoscontrol_protocol::PAYLOAD_MAX {
        return (0, chaoscontrol_protocol::STATUS_ASSERTION_LIMIT_EXCEEDED);
    }
    if let Some(page_ptr) = crate::internal::vm_page_ptr() {
        unsafe {
            let page = &mut *page_ptr;
            page.command = command;
            page.flags = flags;
            page.id = id;
            page.payload_len = payload.len() as u16;
            page.result = 0;
            page.status = 0;
            page.payload[..payload.len()].copy_from_slice(payload);
            crate::internal::vm_trigger();
            return (page.result, page.status);
        }
    }
    dispatch_local_raw(command, flags, id, payload);
    (0, 0)
}

#[cfg(feature = "full")]
pub(crate) fn hypercall_bound_assertion(
    command: u8,
    flags: u8,
    message: &str,
    json_details: &[u8],
    identity: crate::assertion_catalog::BoundIdentity,
) -> (u64, u8) {
    use chaoscontrol_protocol::identity::AssertionKind;
    use chaoscontrol_protocol::transport::{encode_event_frame, EventFrame};
    let kind = match command {
        chaoscontrol_protocol::CMD_ASSERT_ALWAYS => AssertionKind::Always,
        chaoscontrol_protocol::CMD_ASSERT_SOMETIMES => AssertionKind::Sometimes,
        chaoscontrol_protocol::CMD_ASSERT_REACHABLE => AssertionKind::Reachable,
        chaoscontrol_protocol::CMD_ASSERT_UNREACHABLE => AssertionKind::Unreachable,
        _ => return (0, chaoscontrol_protocol::STATUS_ERROR),
    };
    let details = match serde_json::from_slice::<serde_json::Value>(json_details) {
        Ok(details) if details.is_object() => details,
        Ok(_) | Err(_) => return (0, chaoscontrol_protocol::STATUS_ERROR),
    };
    let (wire_id, _) = bound_event_ids(&identity);
    let frame = EventFrame {
        catalog_token: identity.catalog_token,
        fingerprint: identity.fingerprint,
        kind,
        details: json_details.to_vec(),
    };
    let mut payload = [0_u8; chaoscontrol_protocol::PAYLOAD_MAX];
    let Ok(length) = encode_event_frame(&frame, &mut payload) else {
        return (0, chaoscontrol_protocol::STATUS_ASSERTION_LIMIT_EXCEEDED);
    };
    if crate::internal::vm_page_ptr().is_some() {
        return hypercall_raw(command, flags, wire_id, &payload[..length]);
    }
    dispatch_local_bound(command, flags, message, &details, identity);
    (0, 0)
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
                let choice = if n <= 1 {
                    0
                } else {
                    crate::internal::local_random_u64() % n
                };
                crate::internal::local_emit_random_choice(id, choice);
                (choice, 0)
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

#[cfg(feature = "full")]
fn dispatch_local_raw(command: u8, flags: u8, id: u32, payload: &[u8]) {
    use chaoscontrol_protocol::transport::{
        decode_catalog_begin, decode_catalog_complete, decode_descriptor_frame,
    };
    use chaoscontrol_protocol::*;
    let value = match command {
        CMD_ASSERT_CATALOG_BEGIN => serde_json::json!({
            "chaoscontrol_assertion_catalog": {
                "record": "begin",
                "catalog_version": chaoscontrol_protocol::admission::ASSERTION_CATALOG_VERSION,
                "expected_descriptors": id,
                "valid": decode_catalog_begin(payload).is_ok()
            }
        }),
        CMD_ASSERT_CATALOG_DESCRIPTOR => match decode_descriptor_frame(payload) {
            Ok(frame) => serde_json::json!({
                "chaoscontrol_assertion_catalog": {
                    "record": "descriptor",
                    "fingerprint": frame.fingerprint,
                    "descriptor": frame.descriptor,
                    "canonical_descriptor": encode_hex(&frame.canonical_bytes)
                }
            }),
            Err(error) => serde_json::json!({
                "chaoscontrol_assertion_catalog": {
                    "record": "conflict",
                    "error": error.to_string()
                }
            }),
        },
        CMD_ASSERT_CATALOG_COMPLETE => match decode_catalog_complete(payload) {
            Ok(token) => serde_json::json!({
                "chaoscontrol_assertion_catalog": {
                    "record": "complete",
                    "catalog_version": chaoscontrol_protocol::admission::ASSERTION_CATALOG_VERSION,
                    "descriptor_count": id,
                    "catalog_token": token
                }
            }),
            Err(error) => serde_json::json!({
                "chaoscontrol_assertion_catalog": {
                    "record": "conflict",
                    "error": error.to_string()
                }
            }),
        },
        CMD_ASSERT_ALWAYS
        | CMD_ASSERT_SOMETIMES
        | CMD_ASSERT_REACHABLE
        | CMD_ASSERT_UNREACHABLE => serde_json::json!({
            "chaoscontrol_assertion_catalog": {
                "record": "conflict",
                "error": "unbound strict assertion event",
                "command": command,
                "flags": flags,
                "compatibility_id": id
            }
        }),
        _ => return,
    };
    crate::internal::local_emit_value(&value);
}

#[cfg(feature = "full")]
fn bound_event_ids(identity: &crate::assertion_catalog::BoundIdentity) -> (u32, String) {
    let wire_id = identity.compatibility_id.unwrap_or(0);
    let report_id = identity
        .compatibility_id
        .map(|compatibility_id| format!("{compatibility_id:08x}"))
        .unwrap_or_else(|| identity.fingerprint.to_hex());
    (wire_id, report_id)
}

#[cfg(feature = "full")]
fn dispatch_local_bound(
    command: u8,
    flags: u8,
    message: &str,
    details: &serde_json::Value,
    identity: crate::assertion_catalog::BoundIdentity,
) {
    use chaoscontrol_protocol::*;
    let (assert_type, condition) = match command {
        CMD_ASSERT_ALWAYS => ("always", flags & 1 != 0),
        CMD_ASSERT_SOMETIMES => ("sometimes", flags & 1 != 0),
        CMD_ASSERT_REACHABLE => ("reachability", true),
        CMD_ASSERT_UNREACHABLE => ("reachability", false),
        _ => return,
    };
    let (_, report_id) = bound_event_ids(&identity);
    let value = serde_json::json!({
        "antithesis_assert": {
            "assert_type": assert_type,
            "condition": condition,
            "hit": true,
            "must_hit": matches!(assert_type, "sometimes" | "reachability"),
            "id": report_id,
            "message": message,
            "display_type": assert_type,
            "details": details,
            "identity_version": chaoscontrol_protocol::identity::ASSERTION_IDENTITY_VERSION,
            "catalog_token": identity.catalog_token,
            "assertion_fingerprint": identity.fingerprint,
            "catalog_status": "accepted"
        }
    });
    crate::internal::local_emit_value(&value);
}

#[cfg(feature = "full")]
fn encode_hex(input: &[u8]) -> String {
    chaoscontrol_protocol::identity::encode_lower_hex(input)
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

    #[cfg(feature = "full")]
    #[test]
    fn bound_event_ids_use_exact_alias_or_fingerprint_fallback() {
        const PRESENT_ALIAS: u32 = 7;
        const FINGERPRINT_BYTE: u8 = 0xab;
        let fingerprint = chaoscontrol_protocol::identity::AssertionFingerprint(
            [FINGERPRINT_BYTE; chaoscontrol_protocol::identity::ASSERTION_FINGERPRINT_BYTES],
        );
        let present = crate::assertion_catalog::BoundIdentity {
            catalog_token: chaoscontrol_protocol::identity::AssertionFingerprint::ZERO,
            fingerprint,
            compatibility_id: Some(PRESENT_ALIAS),
        };
        let absent = crate::assertion_catalog::BoundIdentity {
            compatibility_id: None,
            ..present
        };

        assert_eq!(
            super::bound_event_ids(&present),
            (PRESENT_ALIAS, "00000007".to_string())
        );
        assert_eq!(super::bound_event_ids(&absent), (0, fingerprint.to_hex()));
    }

    #[test]
    fn payload_max_fits_in_page() {
        const { assert!(PAYLOAD_MAX > 0) };
        const { assert!(PAYLOAD_MAX <= 4096) };
    }
}
