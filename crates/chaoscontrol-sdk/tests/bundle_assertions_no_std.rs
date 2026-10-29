//! Compile and run the multi-condition bundle macros in both feature sets.
//!
//! The `full` build transports the assertions; the no-op build must still
//! accept the same call shape and evaluate every condition exactly once.

use chaoscontrol_sdk::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static EVALUATIONS: AtomicUsize = AtomicUsize::new(0);

fn check(value: bool) -> bool {
    EVALUATIONS.fetch_add(1, Ordering::SeqCst);
    value
}

#[test]
fn bundle_macros_accept_the_same_shape_without_details() {
    EVALUATIONS.store(0, Ordering::SeqCst);

    cc_assert_always_any!({first: check(true), second: check(false)}, "any");
    cc_assert_sometimes_all!({first: check(true), second: check(true)}, "all");

    assert_eq!(EVALUATIONS.load(Ordering::SeqCst), 4);
}

#[cfg(not(feature = "full"))]
#[test]
fn bundle_macros_accept_unit_details_in_noop_mode() {
    cc_assert_always_any!({first: true, second: false}, "any", &42_u8);
    cc_assert_sometimes_all!({first: true, second: true}, "all", &42_u8);
}

#[cfg(not(feature = "full"))]
#[test]
fn comparison_macros_accept_the_same_shape_without_details() {
    cc_assert_always_lt!(1, 2, "lt");
    cc_assert_always_lt!(1, 2, "lt details", &42_u8);
    cc_assert_sometimes_gt!(2, 1, "gt");
    cc_assert_sometimes_gt!(2, 1, "gt details", &42_u8);
}

#[cfg(not(feature = "full"))]
#[test]
fn local_output_constants_are_available_without_full() {
    assert_eq!(
        chaoscontrol_sdk::LOCAL_OUTPUT,
        "CHAOSCONTROL_SDK_LOCAL_OUTPUT"
    );
    assert_eq!(
        chaoscontrol_sdk::ANTITHESIS_LOCAL_OUTPUT,
        "ANTITHESIS_SDK_LOCAL_OUTPUT"
    );
}

#[cfg(not(feature = "full"))]
#[test]
fn random_choice_slice_is_available_without_full() {
    let empty: &[u8] = &[];
    assert_eq!(random_choice(empty), None);
    assert_eq!(random_choice(&[7]), Some(&7));
}

#[cfg(not(feature = "full"))]
#[test]
fn assertion_details_accept_arbitrary_references_in_noop_mode() {
    cc_assert_always!(true, "always details", &42_u8);
}

#[cfg(not(feature = "full"))]
#[test]
fn raw_assertion_noop_keeps_the_call_shape() {
    assert_raw(
        false,
        String::from("raw no-op"),
        &(),
        String::from("noop"),
        String::from("noop::caller"),
        String::from("src/noop.rs"),
        1,
        1,
        false,
        true,
        AssertType::Always,
        String::from("Always"),
        String::from("raw-noop"),
    );
}

#[cfg(not(feature = "full"))]
#[test]
fn lifecycle_noop_keeps_the_detail_call_shape() {
    setup_complete(&());
    setup_complete(&String::from("details"));
    send_event("event", &());
    send_event("event", &String::from("details"));
}

// r[verify chaoscontrol.bundle_details.order]
// r[verify chaoscontrol.bundle_details.preservation]
#[cfg(not(feature = "full"))]
#[test]
fn bundle_details_precede_conditions_without_json_or_clone() {
    struct NonJsonDetails;
    macro_rules! exercise {
        ($bundle:ident, $message:literal) => {{
            let first = NonJsonDetails;
            let stage = std::cell::Cell::new(0);
            $bundle!(
                {
                    first: { assert_eq!(stage.get(), 1); stage.set(2); true },
                    second: { assert_eq!(stage.get(), 2); stage.set(3); false }
                },
                $message,
                { assert_eq!(stage.get(), 0); stage.set(1); &first }
            );
            assert_eq!(stage.get(), 3);
        }};
    }
    exercise!(cc_assert_always_any, "no-full ordered any");
    exercise!(cc_assert_always_some, "no-full ordered some");
    exercise!(cc_assert_sometimes_all, "no-full ordered all");
}

// r[verify chaoscontrol.noop_assertion_details.optional]
#[cfg(not(feature = "full"))]
#[test]
fn optional_assertion_noop_evaluates_borrowed_details_once_in_order() {
    fn exercise<D: ?Sized>(details: &D) {
        for condition in [true, false] {
            let stage = std::cell::Cell::new(0);
            cc_assert_always_or_unreachable!(
                {
                    assert_eq!(stage.get(), 0);
                    stage.set(1);
                    condition
                },
                "no-op optional details",
                {
                    assert_eq!(stage.get(), 1);
                    stage.set(2);
                    details
                }
            );
            assert_eq!(stage.get(), 2);
        }
    }
    struct NonSerializableDetails;
    exercise(&NonSerializableDetails);
    exercise("borrowed unsized text");
    exercise([1_u8, 2, 3].as_slice());
}
