const EXPECTED_PROFILE: &[u8] = include_bytes!(env!("VM_COHORT_PROFILE"));
const OBSERVED_PROFILE: &[u8] = include_bytes!("../../../config/generated/profile.json");

fn main() {
    assert_eq!(OBSERVED_PROFILE, EXPECTED_PROFILE, "profile bytes changed");
}
