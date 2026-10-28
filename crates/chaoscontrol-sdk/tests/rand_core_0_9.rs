#![cfg(feature = "rand_v0_9")]

use chaoscontrol_sdk::random::ChaosControlRng;
use rand_core_v0_9::RngCore;

#[test]
fn rng_implements_rand_core_0_9() {
    let mut rng = ChaosControlRng;

    let mut bytes = [0u8; 17];
    rng.fill_bytes(&mut bytes);
    assert_ne!(bytes, [0u8; 17], "fill_bytes must write guided bytes");

    let _: u32 = rng.next_u32();
    let _: u64 = rng.next_u64();
}
