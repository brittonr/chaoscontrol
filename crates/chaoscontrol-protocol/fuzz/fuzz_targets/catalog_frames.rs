#![no_main]
// r[impl chaoscontrol.fuzzing.guest_boundary_targets]

use chaoscontrol_protocol::identity::AssertionKind;
use chaoscontrol_protocol::transport::{
    decode_catalog_begin, decode_catalog_complete, decode_descriptor_frame, decode_event_frame,
    encode_catalog_complete, encode_descriptor_frame, encode_event_frame,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let buffer = data.to_vec();

    if decode_catalog_begin(&buffer).is_ok() {
        assert_eq!(
            buffer,
            [
                chaoscontrol_protocol::transport::ASSERTION_WIRE_VERSION,
                chaoscontrol_protocol::identity::ASSERTION_IDENTITY_VERSION,
            ],
            "catalog-begin accepted a non-canonical frame"
        );
    }

    if let Ok(token) = decode_catalog_complete(&buffer) {
        let mut reencoded = vec![0u8; 64];
        let written = encode_catalog_complete(token, &mut reencoded)
            .expect("a decoded catalog completion re-encodes");
        assert_eq!(
            &reencoded[..written],
            buffer.as_slice(),
            "non-canonical catalog completion accepted"
        );
    }

    if let Ok(frame) = decode_descriptor_frame(&buffer) {
        let mut reencoded = vec![0u8; 256 + frame.canonical_bytes.len()];
        let written = encode_descriptor_frame(&frame.descriptor, &mut reencoded)
            .expect("a decoded descriptor frame re-encodes");
        assert_eq!(
            &reencoded[..written],
            buffer.as_slice(),
            "non-canonical descriptor frame accepted"
        );
    }

    for kind in [
        AssertionKind::Always,
        AssertionKind::Sometimes,
        AssertionKind::Reachable,
        AssertionKind::Unreachable,
    ] {
        if let Ok(frame) = decode_event_frame(&buffer, kind) {
            let mut reencoded = vec![0u8; 256 + frame.details.len()];
            let written = encode_event_frame(&frame, &mut reencoded)
                .expect("a decoded event frame re-encodes");
            assert_eq!(
                &reencoded[..written],
                buffer.as_slice(),
                "non-canonical event frame accepted"
            );
        }
    }
});
