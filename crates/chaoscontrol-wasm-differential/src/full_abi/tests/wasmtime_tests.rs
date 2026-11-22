use super::{
    encode_with_run, hostcall, profile, roundtrip, AdmittedCase, CaseDenial, DiagnosticObservation,
    Terminal,
};
use crate::full_abi::host::{DeclaredCall, RequestTrap};
use crate::full_abi::wasmtime;

// local.get input pointer; local.get input length; call the admitted import.
const CALL_ASSERT: &[u8] = &[0x20, 0, 0x20, 1, 0x10, 0, 0x0b];

fn value(text: &str) -> preserves::IOValue {
    preserves::read_iovalue_text(text, false).unwrap()
}

fn assert_case(code: &[u8], expected: &str) -> DiagnosticObservation {
    let request = value("<sample 1>");
    let reply = value("<reply 2>");
    let calls = [DeclaredCall {
        name: "assert",
        request: &request,
        reply: &reply,
        is_pause_requested: true,
    }];
    let module = encode_with_run(&hostcall(), code);
    let profile = profile();
    let case = AdmittedCase::admit(&profile, &module, &request, &value(expected), &calls).unwrap();
    wasmtime::run(&case).unwrap()
}

// r[verify chaoscontrol.spacewasm_full_abi.host]
#[test]
fn selected_import_exchanges_canonical_bytes_with_the_reference_engine() {
    let observation = assert_case(CALL_ASSERT, "<reply 2>");
    let request = preserves::write_iovalue_packed(&value("<sample 1>"), false).unwrap();
    let output = preserves::write_iovalue_packed(&value("<reply 2>"), false).unwrap();
    let [exchange] = observation.transcript.as_slice() else {
        panic!("expected one executed hostcall")
    };
    assert_eq!(exchange.request_blake3, blake3::hash(&request));
    assert_eq!(exchange.reply_blake3, blake3::hash(&output));
    assert!(exchange.is_pause_requested);
    assert_eq!(
        observation.terminal,
        Terminal::Completed {
            output_blake3: blake3::hash(&output),
            output_bytes: output.len() as u32
        }
    );
    assert!(observation.fuel_consumed > 0);
}

#[test]
fn extra_request_byte_traps_without_a_reply_but_output_mismatch_retains_the_call() {
    // Add one to the exact request length before calling the real host import.
    let shifted = [0x20, 0, 0x20, 1, 0x41, 1, 0x6a, 0x10, 0, 0x0b];
    let denied = assert_case(&shifted, "<reply 2>");
    assert_eq!(
        denied.terminal,
        Terminal::Hostcall(RequestTrap::RequestBytes)
    );
    assert!(denied.transcript.is_empty());
    let wrong_output = assert_case(CALL_ASSERT, "<reply 3>");
    assert_eq!(wrong_output.terminal, Terminal::OutputMismatch);
    assert_eq!(
        wrong_output.transcript[0].reply_blake3,
        blake3::hash(&preserves::write_iovalue_packed(&value("<reply 2>"), false).unwrap())
    );
}

#[test]
fn bounded_engine_distinguishes_fuel_stack_memory_and_abi_outcomes() {
    // The diagnostic functions below respectively loop, recurse, load beyond
    // the single page, return a negative pointer, and execute unreachable.
    let cases: &[(&[u8], u64, Terminal)] = &[
        (
            &[0x03, 0x40, 0x0c, 0, 0x0b, 0, 0x0b],
            64,
            Terminal::FuelExhausted,
        ),
        (
            &[0x20, 0, 0x20, 1, 0x10, 2, 0x42, 1, 0x7c, 0x0b],
            10_000_000,
            Terminal::StackLimit,
        ),
        (
            &[0x41, 0x7f, 0x28, 2, 0, 0xad, 0x0b],
            1024,
            Terminal::MemoryTrap,
        ),
        (&[0x42, 0x7f, 0x0b], 1024, Terminal::OutputRange),
        (&[0, 0x0b], 1024, Terminal::GuestTrap),
    ];
    let data = value("true");
    for &(body, fuel, terminal) in cases {
        let mut profile = profile();
        profile.bounds.wasmtime_fuel = fuel;
        let module = encode_with_run(&roundtrip(), body);
        let case = AdmittedCase::admit(&profile, &module, &data, &data, &[]).unwrap();
        let result = wasmtime::run(&case).unwrap();
        assert_eq!(result.terminal, terminal);
        assert!((1..=fuel).contains(&result.fuel_consumed));
    }
}

#[test]
fn name_only_or_ambient_import_is_denied_before_reference_execution() {
    let mut fixture = hostcall();
    fixture.imports[0] = ("wasi_snapshot_preview1", "assert", 0);
    let module = encode_with_run(&fixture, CALL_ASSERT);
    let input = value("true");
    let profile = profile();
    let rejected = AdmittedCase::admit(&profile, &module, &input, &input, &[]);
    assert!(matches!(rejected, Err(CaseDenial::Module(_))));
}
