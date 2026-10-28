//! KVM race proof for the cooperative logical-thread runtime.

use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
use vm_memory::{ByteValued, Bytes, GuestAddress};

const GATE_ADDRESS: u64 = 0x6000;
const GATE_MAGIC: u64 = 0x4343_5448_5245_4144;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Gate {
    magic: u64,
    paused_ordinal: u64,
    release_tick: u64,
    logical_tick: u64,
    paused_steps: u64,
}

unsafe impl ByteValued for Gate {}

fn run(guest: &str, gate: Gate) -> String {
    let mut vm = DeterministicVm::new(VmConfig {
        memory_size: 16 * 1024 * 1024,
        ..VmConfig::default()
    })
    .expect("create KVM guest");
    vm.load_kernel(guest, None).expect("load cooperative guest");
    vm.memory()
        .inner()
        .write_obj(gate, GuestAddress(GATE_ADDRESS))
        .expect("write pause gate");
    let result = vm.run_bounded(100_000).expect("run cooperative guest");
    eprintln!("run result: {result:?}");
    vm.take_serial_output()
}

#[test]
#[ignore = "requires CHAOSCONTROL_THREAD_GUEST built for x86_64-unknown-linux-musl"]
fn paused_and_control_interleavings_differ_under_kvm() {
    let guest = std::env::var("CHAOSCONTROL_THREAD_GUEST")
        .expect("CHAOSCONTROL_THREAD_GUEST points at the cooperative guest ELF");
    let control = run(
        &guest,
        Gate {
            magic: GATE_MAGIC,
            paused_ordinal: u64::MAX,
            release_tick: 0,
            logical_tick: 0,
            paused_steps: 0,
        },
    );
    let paused = run(
        &guest,
        Gate {
            magic: GATE_MAGIC,
            paused_ordinal: 1,
            release_tick: 5,
            logical_tick: 0,
            paused_steps: 0,
        },
    );
    assert!(control.contains("R0a"), "control: {control}");
    assert!(paused.contains("R0b"), "paused: {paused}");
    assert!(paused.contains("P1"), "pause observed: {paused}");
}
