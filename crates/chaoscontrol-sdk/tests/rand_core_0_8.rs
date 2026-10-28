#![cfg(feature = "rand_core_v0_6")]

use chaoscontrol_sdk::random::ChaosControlRng;
use rand::RngCore;

#[test]
fn rng_implements_rand_0_8() {
    let mut rng = ChaosControlRng;

    let mut bytes = [0u8; 17];
    rng.fill_bytes(&mut bytes);
    #[cfg(feature = "full")]
    assert_ne!(bytes, [0u8; 17], "fill_bytes must write guided bytes");

    let _: u32 = rng.next_u32();
    let _: u64 = rng.next_u64();
}
