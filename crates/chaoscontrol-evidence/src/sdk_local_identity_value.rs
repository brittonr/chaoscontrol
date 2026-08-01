use chaoscontrol_protocol::assertion_identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind,
};

pub(crate) fn exact_kind(kind: AssertionKind) -> &'static str {
    match kind {
        AssertionKind::Always => "always",
        AssertionKind::Sometimes => "sometimes",
        AssertionKind::Reachable => "reachable",
        AssertionKind::Unreachable => "unreachable",
    }
}

const COMPATIBILITY_ID_HEX_DIGITS: usize = 8;

pub(crate) fn report_id(
    descriptor: &AssertionDescriptor,
    fingerprint: AssertionFingerprint,
) -> String {
    descriptor
        .compatibility_id
        .map(|value| format!("{value:0width$x}", width = COMPATIBILITY_ID_HEX_DIGITS))
        .unwrap_or_else(|| fingerprint.to_hex())
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    chaoscontrol_protocol::assertion_identity::encode_lower_hex(bytes)
}
