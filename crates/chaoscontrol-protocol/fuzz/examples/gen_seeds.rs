use std::fs;
use std::path::Path;

use chaoscontrol_protocol::identity::{
    AssertionDescriptor, AssertionFingerprint, AssertionKind, AssertionLogicalKey,
    ASSERTION_IDENTITY_VERSION,
};
use chaoscontrol_protocol::transport::{
    encode_catalog_complete, encode_descriptor_frame, encode_event_frame, EventFrame,
    ASSERTION_WIRE_VERSION,
};

fn main() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let frames = corpus.join("catalog_frames");
    let payloads = corpus.join("payload");
    fs::create_dir_all(&frames).expect("corpus dir");
    fs::create_dir_all(&payloads).expect("payload dir");

    fs::write(
        frames.join("catalog_begin"),
        [ASSERTION_WIRE_VERSION, ASSERTION_IDENTITY_VERSION],
    )
    .expect("begin seed");

    let mut complete = [0u8; 64];
    let written = encode_catalog_complete(AssertionFingerprint([7; 32]), &mut complete)
        .expect("complete frame");
    fs::write(frames.join("catalog_complete"), &complete[..written]).expect("complete seed");

    let descriptor = AssertionDescriptor {
        identity_version: ASSERTION_IDENTITY_VERSION,
        namespace: "build:fuzz:v1".to_string(),
        logical_key: AssertionLogicalKey::Automatic {
            source_site: "src/fuzz.rs:1:1".to_string(),
        },
        compatibility_id: Some(1),
        kind: AssertionKind::Always,
        message: "fuzz descriptor".to_string(),
        source_file: "src/fuzz.rs".to_string(),
        source_line: 1,
        source_column: 1,
        guest: "fuzz".to_string(),
        category: "invariant".to_string(),
    };
    let mut descriptor_frame = vec![0u8; 64 + 4096];
    let written =
        encode_descriptor_frame(&descriptor, &mut descriptor_frame).expect("descriptor frame");
    fs::write(frames.join("descriptor"), &descriptor_frame[..written]).expect("descriptor seed");

    for (name, kind) in [
        ("event_always", AssertionKind::Always),
        ("event_sometimes", AssertionKind::Sometimes),
        ("event_reachable", AssertionKind::Reachable),
    ] {
        let mut event = [0u8; 128];
        let written = encode_event_frame(
            &EventFrame {
                catalog_token: AssertionFingerprint([7; 32]),
                fingerprint: descriptor.fingerprint().expect("fingerprint"),
                kind,
                details: Vec::new(),
            },
            &mut event,
        )
        .expect("event frame");
        fs::write(frames.join(name), &event[..written]).expect("event seed");
    }

    for (name, message, details) in [
        ("payload_json", "assertion failed", &b"{\"k\":1}"[..]),
        ("payload_empty", "", &b""[..]),
        ("payload_utf8", "é preuve", &b"[1,2,3]"[..]),
    ] {
        let mut payload = vec![0u8; 4 + message.len() + details.len()];
        let written = chaoscontrol_protocol::encode_payload(&mut payload, message, details)
            .expect("payload frame");
        fs::write(payloads.join(name), &payload[..written]).expect("payload seed");
    }
}
