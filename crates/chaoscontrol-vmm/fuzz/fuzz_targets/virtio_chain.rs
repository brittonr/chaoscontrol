#![no_main]

use chaoscontrol_vmm_fuzz::{check_plan, parse_input, plan};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = parse_input(data);
    if let Some(plan) = plan(&input) {
        check_plan(&plan, &input);
    }
});
