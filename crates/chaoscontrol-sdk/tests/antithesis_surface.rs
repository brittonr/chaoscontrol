//! Compile-time conformance check for the ported Antithesis surface.

use chaoscontrol_sdk::assert::TrackingInfo;
use chaoscontrol_sdk::compat::{assert, lifecycle, random, AntithesisRng};
use chaoscontrol_sdk::prelude::*;

#[test]
fn upstream_surface_paths_resolve() {
    // Types and constants.
    let _: AssertType = AssertType::Always;
    let _: assert::AssertType = assert::AssertType::Reachability;
    let _: TrackingInfo = TrackingInfo::new();
    let _: random::AntithesisRng = AntithesisRng;
    let _: assert::TrackingInfo = assert::TrackingInfo::new();
    assert_eq!(
        chaoscontrol_sdk::LOCAL_OUTPUT,
        "CHAOSCONTROL_SDK_LOCAL_OUTPUT"
    );
    assert_eq!(
        chaoscontrol_sdk::ANTITHESIS_LOCAL_OUTPUT,
        "ANTITHESIS_SDK_LOCAL_OUTPUT"
    );

    // Guidance type paths.
    let _ = assert::guidance::GuidanceType::Json;
    let _: assert::guidance::NumericGuard<true> = assert::guidance::NumericGuard::new();

    // Module functions and macros resolve without invocation side effects.
    let _ = lifecycle::send_event;
    let _ = lifecycle::setup_complete;
    let _ = random::random_choice::<u8>;
    let _ = random::random_choice_index;
    let _ = random::get_random;
    let _ = chaoscontrol_sdk::assert::assert_raw;
    let _: fn(bool, &str) = |condition, _| {
        assert_always!(condition, "surface check");
    };
    let _ = antithesis_init;
}
