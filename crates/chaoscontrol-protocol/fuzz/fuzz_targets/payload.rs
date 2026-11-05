#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let buffer = data.to_vec();
    let Some(decoded) = chaoscontrol_protocol::decode_payload(&buffer) else {
        return;
    };
    let mut reencoded = vec![0u8; 4 + decoded.message.len() + decoded.json_details.len()];
    let written = chaoscontrol_protocol::encode_payload(
        &mut reencoded,
        &decoded.message,
        &decoded.json_details,
    )
    .expect("a decoded payload always re-encodes");
    let again = chaoscontrol_protocol::decode_payload(&reencoded[..written])
        .expect("an encoded payload always decodes");
    assert_eq!(
        again, decoded,
        "payload round-trip changed the decoded value"
    );
});
