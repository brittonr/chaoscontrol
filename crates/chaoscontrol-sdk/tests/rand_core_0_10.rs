#![cfg(feature = "rand_core_v0_10")]

use chaoscontrol_sdk::random::ChaosControlRng;
use rand_core_v0_10::TryRng;

#[test]
fn rng_implements_rand_core_0_10_try_rng() {
    let mut rng = ChaosControlRng;

    let mut bytes = [0u8; 17];
    rng.try_fill_bytes(&mut bytes)
        .expect("guided randomness is infallible");
    assert_ne!(bytes, [0u8; 17], "try_fill_bytes must write guided bytes");

    let _: u32 = rng.try_next_u32().expect("u32 is infallible");
    let _: u64 = rng.try_next_u64().expect("u64 is infallible");
}
