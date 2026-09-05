pub(crate) fn exact_kind(kind: ::chaoscontrol_protocol::identity::AssertionKind) -> &'static str {
    match kind {
        ::chaoscontrol_protocol::identity::AssertionKind::Always => "always",
        ::chaoscontrol_protocol::identity::AssertionKind::Sometimes => "sometimes",
        ::chaoscontrol_protocol::identity::AssertionKind::Reachable => "reachable",
        ::chaoscontrol_protocol::identity::AssertionKind::Unreachable => "unreachable",
    }
}

const COMPATIBILITY_ID_HEX_DIGITS: usize = 8;

pub(crate) fn report_id(
    descriptor: &::chaoscontrol_protocol::identity::AssertionDescriptor,
    fingerprint: ::chaoscontrol_protocol::identity::AssertionFingerprint,
) -> String {
    descriptor
        .compatibility_id
        .map(|value| format!("{value:0width$x}", width = COMPATIBILITY_ID_HEX_DIGITS))
        .unwrap_or_else(|| fingerprint.to_hex())
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    chaoscontrol_protocol::identity::encode_lower_hex(bytes)
}
