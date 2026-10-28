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
    cc_assert_always_any!({first: true, second: false}, "any", &());
    cc_assert_sometimes_all!({first: true, second: true}, "all", &());
}
