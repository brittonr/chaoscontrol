//! Exact-name compatibility surface for code ported from `antithesis_sdk`.
//!
//! The renamed items keep the Antithesis spelling where the signature and
//! semantics match. Raw assertions must be declared before the catalog is
//! emitted; new raw sites after initialization are rejected.

pub use crate::assert::assert_raw;
pub use crate::assert::AssertType;

/// Antithesis-compatible `assert` submodule paths.
pub mod assert {
    pub use crate::assert::assert_raw;
    pub use crate::assert::{AssertType, AssertionKind};
    #[cfg(feature = "full")]
    pub use crate::guidance;
}

pub use crate::chaoscontrol_init as antithesis_init;
pub use crate::random::AntithesisRng;
pub use crate::{
    cc_assert_always as assert_always, cc_assert_always_greater_than as assert_always_greater_than,
    cc_assert_always_greater_than_or_equal_to as assert_always_greater_than_or_equal_to,
    cc_assert_always_less_than as assert_always_less_than,
    cc_assert_always_less_than_or_equal_to as assert_always_less_than_or_equal_to,
    cc_assert_always_or_unreachable as assert_always_or_unreachable,
    cc_assert_always_some as assert_always_some, cc_assert_reachable as assert_reachable,
    cc_assert_sometimes as assert_sometimes, cc_assert_sometimes_all as assert_sometimes_all,
    cc_assert_sometimes_greater_than as assert_sometimes_greater_than,
    cc_assert_sometimes_greater_than_or_equal_to as assert_sometimes_greater_than_or_equal_to,
    cc_assert_sometimes_less_than as assert_sometimes_less_than,
    cc_assert_sometimes_less_than_or_equal_to as assert_sometimes_less_than_or_equal_to,
    cc_assert_unreachable as assert_unreachable,
};
pub use crate::{lifecycle, random};
