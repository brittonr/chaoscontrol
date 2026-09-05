//! Guest-visible resource observations supplied by the VMM.

/// Return the current admitted memory ceiling in bytes.
///
/// The VMM returns the baseline guest-memory size when no pressure window is
/// active. A disabled SDK returns `None` and does not invent an observation.
pub fn memory_ceiling_bytes() -> Option<::core::num::NonZeroU64> {
    let (bytes, status) =
        crate::transport::hypercall_simple(chaoscontrol_protocol::CMD_RESOURCE_MEMORY_CEILING, 0);
    if status != chaoscontrol_protocol::STATUS_OK {
        return None;
    }
    ::core::num::NonZeroU64::new(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_sdk_does_not_invent_a_memory_ceiling() {
        if !crate::is_in_vm() {
            assert_eq!(memory_ceiling_bytes(), None);
        }
    }
}
