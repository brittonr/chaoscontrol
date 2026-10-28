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
