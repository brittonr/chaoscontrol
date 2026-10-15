//! KVM regressions for base-relative incremental memory across sibling branches.

use super::*;
use crate::snapshot::{SnapshotMemory, VmSnapshot, PAGE_SIZE};
use std::collections::BTreeMap;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress};

const TEST_MEMORY_BYTES: usize = 2 * 1024 * 1024;
const TEST_LINEAGE_DEPTH: usize = 4;
const DATA_FIRST_PAGE: usize = 2;
const OUTPUT_PORT: u16 = 0x10;
const GUEST_RFLAGS_RESERVED: u64 = 2;
// mov byte [bx], al; out 0x10, al; jmp 0. Only the guest writes data pages.
const STORE_AND_OUTPUT: [u8; 6] = [0x88, 0x07, 0xe6, 0x10, 0xeb, 0xfa];

fn harness() -> (DeterministicVm, VmSnapshot, Arc<Vec<u8>>) {
    let mut vm = DeterministicVm::new(VmConfig {
        memory_size: TEST_MEMORY_BYTES,
        ..VmConfig::default()
    })
    .expect("create incremental snapshot VM");
    vm.memory
        .inner()
        .write_slice(&STORE_AND_OUTPUT, GuestAddress(0))
        .expect("install fixed guest program");
    vm.memory
        .inner()
        .write_slice(&[0x3c], GuestAddress(PAGE_SIZE as u64))
        .expect("install unchanged base sentinel");
    let mut sregs = vm.vcpus[0].get_sregs().expect("read real-mode state");
    sregs.cs.base = 0;
    sregs.cs.selector = 0;
    sregs.ds.base = 0;
    sregs.ds.selector = 0;
    vm.vcpus[0].set_sregs(&sregs).expect("set real-mode state");
    let mut regs = vm.vcpus[0].get_regs().expect("read guest registers");
    regs.rip = 0;
    regs.rflags = GUEST_RFLAGS_RESERVED;
    vm.vcpus[0].set_regs(&regs).expect("set guest registers");
    let bootstrap = vm.snapshot().expect("capture bootstrap state");
    let base = Arc::new(bootstrap.memory.materialize());
    vm.read_guest_dirty_bitmap()
        .expect("drain initial dirty log before binding the base");
    (vm, bootstrap, base)
}

fn guest_store(vm: &mut DeterministicVm, page: usize, value: u8) {
    let mut regs = vm.vcpus[0].get_regs().expect("read guest registers");
    regs.rip = 0;
    regs.rbx = (page * PAGE_SIZE) as u64;
    regs.rax = u64::from(value);
    vm.vcpus[0].set_regs(&regs).expect("select guest store");
    match vm.vcpus[0].run().expect("execute guest store") {
        VcpuExit::IoOut(port, bytes) => {
            assert_eq!(port, OUTPUT_PORT);
            assert_eq!(bytes, &[value]);
        }
        other => panic!("expected store acknowledgement, got {other:?}"),
    }
    vm.vcpus[0].set_kvm_immediate_exit(1);
    let completed = match vm.vcpus[0].run() {
        Ok(VcpuExit::Intr) => true,
        Err(error) => error.errno() == libc::EINTR,
        Ok(_) => false,
    };
    vm.vcpus[0].set_kvm_immediate_exit(0);
    assert!(completed, "complete pending output before snapshot");
}

// r[verify chaoscontrol.deterministic_snapshots.incremental_lineage]
// r[verify chaoscontrol.deterministic_snapshots.incremental_validation]
#[test]
fn incremental_lineage_matches_full_memory_through_depth_four() {
    let (mut vm, bootstrap, base) = harness();
    let mut parent = bootstrap;
    for depth in 1..=TEST_LINEAGE_DEPTH {
        vm.restore_incremental(&parent, &base)
            .expect("restore incremental parent");
        guest_store(&mut vm, DATA_FIRST_PAGE + depth - 1, depth as u8);
        let full = vm.snapshot().expect("capture full reference");
        let (incremental, _) = vm
            .snapshot_incremental(&base)
            .expect("capture complete incremental lineage");
        let actual = incremental.memory.materialize();
        let expected = full.memory.materialize();
        assert!(
            actual == expected,
            "incremental memory differs from full reference at depth {depth}"
        );
        parent = incremental;
    }
}

// r[verify chaoscontrol.deterministic_snapshots.incremental_revert]
#[test]
fn sibling_restore_reverts_guest_writes_without_an_intervening_capture() {
    let (mut vm, mut parent, base) = harness();
    parent.memory = SnapshotMemory::Overlay {
        base: Arc::clone(&base),
        dirty_pages: BTreeMap::new(),
    };
    vm.restore_incremental(&parent, &base)
        .expect("restore empty parent");
    guest_store(&mut vm, DATA_FIRST_PAGE, 0xa5);
    vm.restore_incremental(&parent, &base)
        .expect("restore sibling from same parent");
    let actual = vm.snapshot().expect("capture restored sibling");
    assert!(
        actual.memory.materialize() == *base,
        "sibling retained a guest-written page outside its parent overlay"
    );
}

#[test]
fn rejected_base_sizes_preserve_guest_memory_and_pending_dirty_pages() {
    let (mut vm, parent, base) = harness();
    guest_store(&mut vm, DATA_FIRST_PAGE, 0xa5);
    let short_base = Arc::new(base[..base.len() - PAGE_SIZE].to_vec());
    assert!(matches!(
        vm.snapshot_incremental(&short_base),
        Err(VmError::Snapshot { .. })
    ));
    let (captured, _) = vm
        .snapshot_incremental(&base)
        .expect("capture after rejection");
    assert_eq!(
        captured.memory.materialize()[DATA_FIRST_PAGE * PAGE_SIZE],
        0xa5
    );
    assert!(matches!(
        vm.restore_incremental(&parent, &short_base),
        Err(VmError::Snapshot { .. })
    ));
    let unchanged = vm.snapshot().expect("capture after rejected restore");
    assert!(unchanged.memory.materialize() == captured.memory.materialize());
    vm.restore_incremental(&parent, &base)
        .expect("valid restore remains admitted");
    assert!(
        vm.snapshot()
            .expect("capture restored base")
            .memory
            .materialize()
            == *base
    );
}

#[test]
fn full_restore_of_overlay_installs_its_base_on_a_fresh_vm() {
    let (mut source, _, base) = harness();
    guest_store(&mut source, DATA_FIRST_PAGE, 0xa5);
    let (parent, _) = source
        .snapshot_incremental(&base)
        .expect("capture overlay parent");
    let mut fresh = DeterministicVm::new(VmConfig {
        memory_size: TEST_MEMORY_BYTES,
        ..VmConfig::default()
    })
    .expect("create fresh restore target");
    fresh
        .restore(&parent)
        .expect("restore complete overlay on fresh VM");
    let actual = fresh.snapshot().expect("capture fresh restored memory");
    assert!(actual.memory.materialize() == parent.memory.materialize());
}

#[test]
fn incremental_capture_after_full_overlay_restore_retains_parent_pages() {
    let (mut vm, _, base) = harness();
    guest_store(&mut vm, DATA_FIRST_PAGE, 0xa5);
    let (parent, _) = vm
        .snapshot_incremental(&base)
        .expect("capture overlay parent");
    vm.restore(&parent).expect("full restore of overlay parent");
    let (child, _) = vm
        .snapshot_incremental(&base)
        .expect("capture unchanged child");
    assert!(child.memory.materialize() == parent.memory.materialize());
}

#[test]
fn incremental_capture_and_sibling_restore_after_full_image_keep_exact_memory() {
    let (mut vm, bootstrap, base) = harness();
    guest_store(&mut vm, DATA_FIRST_PAGE, 0xa5);
    let parent = vm.snapshot().expect("capture non-base full parent");
    vm.restore(&parent).expect("restore non-base full parent");
    let (child, _) = vm
        .snapshot_incremental(&base)
        .expect("capture full-parent child");
    assert!(child.memory.materialize() == parent.memory.materialize());
    vm.restore_incremental(&bootstrap, &base)
        .expect("restore bootstrap sibling");
    assert!(
        vm.snapshot()
            .expect("capture bootstrap sibling")
            .memory
            .materialize()
            == *base
    );
    vm.restore_incremental(&parent, &base)
        .expect("incremental restore of full parent");
    let (child, _) = vm
        .snapshot_incremental(&base)
        .expect("capture incrementally restored full parent");
    assert!(child.memory.materialize() == parent.memory.materialize());
}

#[test]
fn public_dirty_log_observation_preserves_incremental_memory() {
    let (mut vm, _, base) = harness();
    guest_store(&mut vm, DATA_FIRST_PAGE, 0xa5);
    let expected = vm
        .snapshot()
        .expect("capture full reference before dirty read");
    let bitmap = vm.get_dirty_bitmap().expect("observe guest dirty pages");
    assert_ne!(
        bitmap[DATA_FIRST_PAGE / 64] & (1 << (DATA_FIRST_PAGE % 64)),
        0
    );
    let (child, _) = vm
        .snapshot_incremental(&base)
        .expect("capture after public dirty read");
    assert!(child.memory.materialize() == expected.memory.materialize());
}
