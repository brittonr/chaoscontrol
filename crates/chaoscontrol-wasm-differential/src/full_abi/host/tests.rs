use super::*;

const PAGE_BYTES: usize = WASM_PAGE_BYTES as usize;
const REQUEST_OFFSET: usize = 32;
const REPLY_OFFSET: usize = ABI_REPLY_OFFSET as usize;

fn profile() -> FullAbiProfile {
    serde_json::from_str(include_str!(
        "../../../../../contracts/evidence/examples/spacewasm-full-abi-differential-profile.json"
    ))
    .unwrap()
}

fn value(text: &str) -> preserves::IOValue {
    preserves::read_iovalue_text(text, false).unwrap()
}

fn request(memory: &mut [u8], value: &preserves::IOValue) -> i32 {
    let bytes = preserves::write_iovalue_packed(value, false).unwrap();
    memory[REQUEST_OFFSET..REQUEST_OFFSET + bytes.len()].copy_from_slice(&bytes);
    i32::try_from(bytes.len()).unwrap()
}

fn declared<'a>(
    name: &'a str,
    request: &'a preserves::IOValue,
    reply: &'a preserves::IOValue,
) -> DeclaredCall<'a> {
    DeclaredCall {
        name,
        request,
        reply,
        is_pause_requested: false,
    }
}

// r[verify chaoscontrol.spacewasm_full_abi.host]
#[test]
fn exact_requests_place_canonical_replies_in_order_without_resupplying_the_model() {
    let assertion = value("<assert-value 1>");
    let send = value("<send-value 2>");
    let accepted = value("<accepted 3>");
    let acknowledged = value("<acknowledged 4>");
    let pause = DeclaredCall {
        is_pause_requested: true,
        ..declared("send", &send, &acknowledged)
    };
    let script = Script::admit(
        &profile(),
        &[declared("assert", &assertion, &accepted), pause],
    )
    .unwrap();
    let mut left_core = script.start();
    let mut right_core = script.start();
    for run in [&mut left_core, &mut right_core] {
        let mut memory = vec![0; PAGE_BYTES];
        for (name, request_value, reply_value, pause) in [
            ("assert", &assertion, &accepted, false),
            ("send", &send, &acknowledged, true),
        ] {
            assert!(!run.is_complete());
            let length = request(&mut memory, request_value);
            let returned = run
                .invoke(name, REQUEST_OFFSET as i32, length, &mut memory)
                .unwrap();
            let expected = preserves::write_iovalue_packed(reply_value, false).unwrap();
            assert_eq!(returned.descriptor >> 32, ABI_REPLY_OFFSET as i64);
            assert_eq!(returned.descriptor as u32 as usize, expected.len());
            assert_eq!(returned.is_pause_requested, pause);
            assert_eq!(
                &memory[REPLY_OFFSET..REPLY_OFFSET + expected.len()],
                expected
            );
            let exchange = run.transcript().last().unwrap();
            let request_bytes = &memory[REQUEST_OFFSET..REQUEST_OFFSET + length as usize];
            assert_eq!(exchange.request_blake3, blake3::hash(request_bytes));
            assert_eq!(exchange.reply_blake3, blake3::hash(&expected));
        }
        assert!(run.is_complete());
        assert_eq!(
            run.transcript().map(|entry| entry.name).collect::<Vec<_>>(),
            ["assert", "send"]
        );
    }
    assert!(left_core.transcript().eq(right_core.transcript()));
}

// r[verify chaoscontrol.spacewasm_full_abi.host]
#[test]
fn a_changed_or_noncanonical_request_is_a_terminal_trap_without_memory_writes() {
    let message = value("<request 1>");
    let changed = value("<request 2>");
    let answer = value("<response 3>");
    let script = Script::admit(&profile(), &[declared("assert", &message, &answer)]).unwrap();
    for (input, trailing_byte) in [(&changed, 0), (&message, 1)] {
        let mut run = script.start();
        let mut memory = vec![0; PAGE_BYTES];
        let length = request(&mut memory, input) + trailing_byte;
        memory[REPLY_OFFSET] = 17;
        let before = memory.clone();
        assert_eq!(
            run.invoke("assert", REQUEST_OFFSET as i32, length, &mut memory),
            Err(RequestTrap::RequestBytes)
        );
        assert_eq!(memory, before);
        assert_eq!(run.trap(), Some(RequestTrap::RequestBytes));
        assert_eq!(run.transcript().len(), 0);
        let valid_length = request(&mut memory, &message);
        assert_eq!(
            run.invoke("assert", REQUEST_OFFSET as i32, valid_length, &mut memory),
            Err(RequestTrap::RequestBytes)
        );
        assert!(!run.is_complete());
        assert_eq!(memory[REPLY_OFFSET], 17);
    }
}

// r[verify chaoscontrol.spacewasm_full_abi.host]
#[test]
fn bounds_and_call_order_deny_before_reply() {
    let message = value("<request 1>");
    let script = Script::admit(&profile(), &[declared("assert", &message, &message)]).unwrap();
    for (name, pointer, short_memory, issue) in [
        (
            "send",
            REQUEST_OFFSET as i32,
            false,
            RequestTrap::UnexpectedCall,
        ),
        ("assert", -1, false, RequestTrap::RequestRange),
        ("assert", i32::MAX, false, RequestTrap::RequestRange),
        (
            "assert",
            REQUEST_OFFSET as i32,
            true,
            RequestTrap::ReplyRange,
        ),
    ] {
        let mut memory = vec![
            0;
            if short_memory {
                REPLY_OFFSET
            } else {
                PAGE_BYTES
            }
        ];
        let length = request(&mut memory, &message);
        let before = memory.clone();
        let mut run = script.start();
        assert_eq!(run.invoke(name, pointer, length, &mut memory), Err(issue));
        assert_eq!(run.trap(), Some(issue));
        assert_eq!(memory, before);
    }
    let mut limited = profile();
    limited.bounds.maximum_hostcalls_per_run = 1;
    assert_eq!(
        Script::admit(
            &limited,
            &[
                declared("assert", &message, &message),
                declared("send", &message, &message)
            ]
        )
        .err(),
        Some(ScriptIssue::Bounds)
    );
    limited.bounds.maximum_input_bytes = 1;
    assert_eq!(
        Script::admit(&limited, &[declared("assert", &message, &message)]).err(),
        Some(ScriptIssue::Encoding)
    );
    assert_eq!(
        Script::admit(&profile(), &[declared("environment", &message, &message)]).err(),
        Some(ScriptIssue::Name)
    );
}
