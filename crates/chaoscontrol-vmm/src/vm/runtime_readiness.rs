//! Exact-runtime regression scenarios; these use the real DeterministicVm shell.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use vm_memory::GuestMemory;

const MEMORY_BYTES: usize = 4 * 1024 * 1024;
const CODE_ADDRESS: u64 = 0x200;
const QUANTUM: u64 = 8;
const HORIZON: usize = 32;
const CODE: [u8; 3] = [0x40, 0xeb, 0xfd]; // inc ax; jmp 0x200 (real mode)

fn spin_vm() -> DeterministicVm {
    let mut vm = DeterministicVm::new(VmConfig {
        memory_size: MEMORY_BYTES,
        num_vcpus: 2,
        smp_instruction_quantum: QUANTUM,
        ..VmConfig::default()
    })
    .unwrap();
    vm.memory
        .inner()
        .write_slice(&CODE, GuestAddress(CODE_ADDRESS))
        .unwrap();
    for (index, vcpu) in vm.vcpus.iter().enumerate() {
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        vcpu.set_sregs(&sregs).unwrap();
        vcpu.set_mp_state(kvm_bindings::kvm_mp_state {
            mp_state: kvm_bindings::KVM_MP_STATE_RUNNABLE,
        })
        .unwrap();
        vcpu.set_regs(&kvm_regs {
            rip: CODE_ADDRESS,
            rflags: 2,
            rax: index as u64,
            ..Default::default()
        })
        .unwrap();
    }
    vm.scheduler = VcpuScheduler::try_new(
        &SchedulerConfig {
            num_vcpus: 2,
            quantum: QUANTUM,
            strategy: SchedulingStrategy::RoundRobin,
            seed: 42,
        },
        ProgressMode::ExactSingleStep,
        vec![true, true],
    )
    .unwrap();
    vm.active_vcpu = vm.scheduler.active();
    vm.configure_progress_source_for_current_turn().unwrap();
    vm
}

fn future(
    vm: &mut DeterministicVm,
    interrupted: bool,
    delay: Duration,
) -> (ScheduleTrace, Vec<(u64, u64, u64)>, u64, u64) {
    let exit_limit = vm.exit_count.checked_add(HORIZON as u64).unwrap();
    while vm.exit_count < exit_limit {
        if interrupted {
            let before = vm.scheduler.state_id();
            let counters = (vm.exit_count, vm.virtual_tsc.read());
            let active = vm.active_vcpu;
            vm.vcpus[active].set_kvm_immediate_exit(1);
            assert!(!vm.step().unwrap()); // Real KVM_RUN EINTR -> production Intr retry path.
            vm.vcpus[active].set_kvm_immediate_exit(0);
            assert_eq!(vm.scheduler.state_id(), before);
            assert_eq!((vm.exit_count, vm.virtual_tsc.read()), counters);
        }
        if !delay.is_zero() {
            thread::sleep(delay);
        }
        assert!(!vm.step().unwrap());
    }
    assert_eq!(vm.exit_count, exit_limit);
    let registers = vm
        .vcpus
        .iter()
        .map(|vcpu| {
            let regs = vcpu.get_regs().unwrap();
            (regs.rax, regs.rip, regs.rflags)
        })
        .collect();
    (
        vm.scheduler.drain_trace().unwrap(),
        registers,
        vm.exit_count,
        vm.virtual_tsc.read(),
    )
}

#[test]
// r[verify chaoscontrol.deterministic_smp.validation.snapshot]
// r[verify chaoscontrol.deterministic_smp.validation]
// r[verify chaoscontrol.deterministic_snapshots.validation.continuation]
fn whole_vm_partial_quantum_restore_and_real_eintr_match_control_future() {
    let mut vm = spin_vm();
    for _ in 0..3 {
        assert!(!vm.step().unwrap());
    }
    assert_eq!(vm.scheduler.remaining(), QUANTUM - 3);
    vm.scheduler.drain_trace().unwrap();
    let checkpoint = vm.snapshot().unwrap();
    let control = future(&mut vm, false, Duration::ZERO);
    vm.restore(&checkpoint).unwrap();
    assert_eq!(vm.scheduler.remaining(), QUANTUM - 3);
    let restored = future(&mut vm, true, Duration::from_micros(25));
    assert_eq!(restored, control);
}

struct Contender {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Contender {
    fn new(target: libc::pthread_t, cadence: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let thread = thread::spawn(move || {
            let mut value = 1u64;
            while !worker_stop.load(Ordering::Relaxed) {
                for _ in 0..10_000 {
                    value = std::hint::black_box(value.wrapping_mul(3).wrapping_add(1));
                }
                // SAFETY: target is the live test thread; Drop joins before its exit.
                assert_eq!(unsafe { libc::pthread_kill(target, libc::SIGALRM) }, 0);
                thread::sleep(cadence);
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Contender {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
// r[verify chaoscontrol.deterministic_smp.validation.host_perturbation]
// r[verify chaoscontrol.deterministic_smp.validation.spurious_interrupts]
// r[verify chaoscontrol.deterministic_smp.validation]
fn production_smp_trace_survives_actual_signals_cadence_and_concurrent_cpu_worker() {
    DeterministicVm::install_sigalrm_handler();
    let control = future(&mut spin_vm(), false, Duration::ZERO);
    for cadence in [Duration::from_micros(20), Duration::from_micros(200)] {
        let mut vm = spin_vm();
        // SAFETY: pthread_self names the calling live thread.
        let contender = Contender::new(unsafe { libc::pthread_self() }, cadence);
        let perturbed = future(&mut vm, true, Duration::from_micros(50));
        drop(contender);
        assert_eq!(perturbed, control);
    }
}

#[test]
// r[verify chaoscontrol.deterministic_snapshots.preflight]
fn pure_preflight_retains_capabilities_and_isolates_topology_rejections() {
    use std::os::fd::{FromRawFd, IntoRawFd};
    let mut vm = spin_vm();
    let checkpoint = vm.snapshot().unwrap();
    // Restore must not rewind this deliberately diverged destination.
    vm.memory
        .inner()
        .write_obj(0x5au8, GuestAddress(0x8000))
        .unwrap();
    vm.step().unwrap();
    let before_memory = vm.memory.dump();
    let before_registers: Vec<_> = vm
        .vcpus
        .iter()
        .map(|vcpu| vcpu.get_regs().unwrap())
        .collect();
    let before_devices = serde_json::to_value(vm.capture_virtio_snapshots().unwrap()).unwrap();
    let before_assertions = serde_json::to_value(vm.fault_engine.snapshot()).unwrap();
    let before_serial = vm.serial.state();
    // An accidental KVM capability ioctl during preflight would now fail ENOTTY.
    let not_kvm = std::fs::File::open("/dev/null").unwrap();
    // SAFETY: the file descriptor is uniquely owned and remains valid until Kvm drops it.
    vm.kvm = unsafe { Kvm::from_raw_fd(not_kvm.into_raw_fd()) };
    vm.validate_fault_engine_snapshot(&checkpoint).unwrap();
    let before = (
        vm.scheduler.state_id(),
        vm.exit_count,
        vm.virtual_tsc.read(),
    );
    for corruption in 0..7 {
        let mut invalid = checkpoint.clone();
        match corruption {
            0 => {
                invalid.virtio_snapshots.pop();
            }
            1 => {
                invalid.virtio_snapshots[1] = invalid.virtio_snapshots[0].clone();
            }
            2 => {
                invalid
                    .virtio_snapshots
                    .push(invalid.virtio_snapshots[0].clone());
            }
            3 => {
                invalid.virtio_snapshots[0].transport.base_addr += 1;
            }
            4 => {
                invalid.virtio_snapshots[0].backend = invalid.virtio_snapshots[1].backend.clone();
            }
            5 => {
                invalid.virtio_snapshots[0].transport.device_id = u32::MAX;
            }
            6 => {
                let transport = invalid.virtio_snapshots[0].transport.clone();
                invalid.virtio_snapshots[0].transport =
                    invalid.virtio_snapshots[1].transport.clone();
                invalid.virtio_snapshots[1].transport = transport;
            }
            _ => unreachable!(),
        }
        let error = vm.restore(&invalid).unwrap_err();
        assert!(matches!(error, VmError::Snapshot { .. }));
        assert!(error.to_string().contains("virtio"));
        assert_eq!(
            (
                vm.scheduler.state_id(),
                vm.exit_count,
                vm.virtual_tsc.read()
            ),
            before
        );
        assert!(vm.schedule_execution_poison().is_none());
        assert_eq!(vm.memory.dump(), before_memory);
        assert_eq!(
            vm.vcpus
                .iter()
                .map(|vcpu| vcpu.get_regs().unwrap())
                .collect::<Vec<_>>(),
            before_registers
        );
        assert_eq!(
            serde_json::to_value(vm.capture_virtio_snapshots().unwrap()).unwrap(),
            before_devices
        );
        assert_eq!(
            serde_json::to_value(vm.fault_engine.snapshot()).unwrap(),
            before_assertions
        );
        assert_eq!(vm.serial.state(), before_serial);
    }
    // Stable identities, not vector positions, define device matching.
    let mut reordered = checkpoint.clone();
    reordered.virtio_snapshots.reverse();
    vm.validate_fault_engine_snapshot(&reordered).unwrap();
}

#[test]
fn admitted_restore_ioctl_failure_reports_phase_and_permanently_blocks_resume() {
    use std::os::fd::{AsRawFd, FromRawFd};
    let mut vm = spin_vm();
    let checkpoint = vm.snapshot().unwrap();
    vm.validate_fault_engine_snapshot(&checkpoint).unwrap();
    let descriptor = vm.vcpus[0].as_raw_fd();
    // Retain the original KVM object while deliberately replacing this owned
    // descriptor with a real non-KVM fd. The next required KVM_RUN fails ENOTTY.
    // SAFETY: dup creates a new, uniquely owned file descriptor.
    let saved_raw = unsafe { libc::dup(descriptor) };
    assert!(saved_raw >= 0);
    // SAFETY: saved_raw is the unique live duplicate returned above.
    let saved = unsafe { std::fs::File::from_raw_fd(saved_raw) };
    let not_kvm = std::fs::File::open("/dev/null").unwrap();
    // SAFETY: descriptor remains owned by VcpuFd; dup2 changes its kernel target
    // without changing ownership or touching the retained KVM_RUN mmap.
    assert_eq!(
        unsafe { libc::dup2(not_kvm.as_raw_fd(), descriptor) },
        descriptor
    );
    let result = vm.restore(&checkpoint);
    // Restore the valid KVM fd before testing the permanent software latch.
    // A subsequent rejection must not merely be another bad-fd ioctl failure.
    assert_eq!(
        unsafe { libc::dup2(saved.as_raw_fd(), descriptor) },
        descriptor
    );
    assert!(matches!(
        result,
        Err(VmError::SnapshotRestoreFailed {
            phase: SnapshotRestorePhase::Quiesce,
            ..
        })
    ));
    let before = (
        vm.exit_count,
        vm.virtual_tsc.read(),
        vm.scheduler.state_id(),
    );
    assert!(matches!(
        vm.step(),
        Err(VmError::ScheduleExecutionPoisoned { .. })
    ));
    assert!(matches!(
        vm.snapshot(),
        Err(VmError::ScheduleExecutionPoisoned { .. })
    ));
    assert!(matches!(
        vm.restore(&checkpoint),
        Err(VmError::ScheduleExecutionPoisoned { .. })
    ));
    assert_eq!(
        (
            vm.exit_count,
            vm.virtual_tsc.read(),
            vm.scheduler.state_id()
        ),
        before
    );
}

#[test]
fn malicious_guest_configuration_and_descriptor_use_full_vm_dispatch_and_shared_memory() {
    use crate::devices::virtio_mmio::*;
    use crate::devices::virtio_types::{DescriptorViolation, TransportViolation, VirtioFailure};
    fn store(code: &mut Vec<u8>, address: u32, value: u32) {
        // Flat 32-bit protected mode: MOV [disp32], imm32.
        code.extend_from_slice(&[0xc7, 0x05]);
        code.extend_from_slice(&address.to_le_bytes());
        code.extend_from_slice(&value.to_le_bytes());
    }
    const PAYLOAD_ADDRESS: u64 = 0x5000;
    const OUTPUT_MARKER: u8 = 0x5a;
    const GDT_FLAGS_CODE32: u16 = 0xc09b; // present ring-0 code, 32-bit, 4 KiB granularity
    for malformed_descriptor in [false, true] {
        let mut vm = DeterministicVm::new(VmConfig {
            memory_size: MEMORY_BYTES,
            ..VmConfig::default()
        })
        .unwrap();
        vm.init_thread_timer().unwrap();
        let mut code = Vec::new();
        let base = VIRTIO_MMIO_BASE_2 as u32;
        let mut mmio_exits = 0;
        if malformed_descriptor {
            for (offset, value) in [
                (VIRTIO_MMIO_STATUS, 1),
                (VIRTIO_MMIO_STATUS, 3),
                (VIRTIO_MMIO_DRIVER_FEATURES_SEL, 1),
                (VIRTIO_MMIO_DRIVER_FEATURES, 1),
                (VIRTIO_MMIO_STATUS, 11),
                (VIRTIO_MMIO_QUEUE_SEL, 0),
                (VIRTIO_MMIO_QUEUE_NUM, 8),
                (VIRTIO_MMIO_QUEUE_DESC_LOW, 0x1000),
                (VIRTIO_MMIO_QUEUE_DRIVER_LOW, 0x2000),
                (VIRTIO_MMIO_QUEUE_DEVICE_LOW, 0x3000),
                (VIRTIO_MMIO_QUEUE_READY, 1),
                (VIRTIO_MMIO_STATUS, 15),
            ] {
                store(&mut code, base + offset as u32, value);
                mmio_exits += 1;
            }
            // These are guest instructions writing the same KVM memory the
            // production transport reads, not a second userspace queue fixture.
            store(&mut code, 0x1000, MEMORY_BYTES as u32); // invalid destination
            store(&mut code, 0x1004, 0);
            store(&mut code, 0x1008, 64);
            store(&mut code, 0x100c, 2); // writable
            store(&mut code, 0x2002, 1); // avail.idx=1; ring[0]=0
            store(&mut code, base + VIRTIO_MMIO_QUEUE_NOTIFY as u32, 0);
        } else {
            // MOV byte ptr [QUEUE_NUM], 0: production MMIO width rejection.
            code.extend_from_slice(&[0xc6, 0x05]);
            code.extend_from_slice(&(base + VIRTIO_MMIO_QUEUE_NUM as u32).to_le_bytes());
            code.push(0);
        }
        mmio_exits += 1;
        code.extend_from_slice(&[0xb0, OUTPUT_MARKER, 0xe6, 0x10, 0xf4]);
        vm.memory
            .inner()
            .write_slice(&code, GuestAddress(CODE_ADDRESS))
            .unwrap();
        vm.memory
            .inner()
            .write_slice(&[0xa5; 64], GuestAddress(PAYLOAD_ADDRESS))
            .unwrap();
        let mut sregs = vm.vcpus[0].get_sregs().unwrap();
        // A 32-bit address override does not enlarge a reset real-mode data
        // segment's 64 KiB limit. Install flat protected-mode segments so the
        // guest's high virtio MMIO addresses are valid architectural accesses.
        sregs.cs = memory::kvm_segment_from_gdt(
            memory::gdt_entry(GDT_FLAGS_CODE32, 0, 0xfffff),
            memory::GDT_INDEX_CODE,
        );
        sregs.ds = data_segment();
        sregs.es = data_segment();
        sregs.ss = data_segment();
        sregs.cr0 |= X86_CR0_PE;
        vm.vcpus[0].set_sregs(&sregs).unwrap();
        vm.vcpus[0]
            .set_regs(&kvm_regs {
                rip: CODE_ADDRESS,
                rflags: 2,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            vm.run_bounded(mmio_exits + 1).unwrap(),
            (mmio_exits + 1, false)
        );
        assert_eq!(vm.vcpus[0].get_regs().unwrap().rax as u8, OUTPUT_MARKER);
        let entropy = vm
            .virtio_devices
            .iter()
            .find(|device| device.base_addr() == VIRTIO_MMIO_BASE_2)
            .unwrap();
        let state = entropy.live_state();
        if malformed_descriptor {
            assert_eq!(
                state.failure,
                Some(VirtioFailure::Descriptor(
                    DescriptorViolation::OutsideMemory {
                        address: MEMORY_BYTES as u64,
                        length: 64
                    }
                ))
            );
        } else {
            assert_eq!(
                state.failure,
                Some(VirtioFailure::Transport(TransportViolation::MmioWidth {
                    actual: 1
                }))
            );
        }
        assert_eq!(state.queues[0].last_avail_idx, 0);
        assert_eq!(state.interrupt_status, 0);
        assert_eq!(
            vm.memory
                .inner()
                .read_obj::<u16>(GuestAddress(0x3002))
                .unwrap(),
            0
        );
        let mut payload = [0; 64];
        vm.memory
            .inner()
            .read_slice(&mut payload, GuestAddress(PAYLOAD_ADDRESS))
            .unwrap();
        assert_eq!(payload, [0xa5; 64]);
        assert_eq!(
            entropy
                .backend()
                .as_any()
                .downcast_ref::<crate::devices::virtio_entropy::VirtioEntropy>()
                .unwrap()
                .entropy()
                .bytes_generated(),
            0
        );
    }
}

#[test]
// r[verify chaoscontrol.deterministic_snapshots.validation.continuation]
fn whole_vm_nondefault_state_round_trip_and_all_backend_continuations_match() {
    use crate::devices::virtio_mmio::*;
    fn request(vm: &mut DeterministicVm, device_index: usize, configure: bool) {
        let device = &mut vm.virtio_devices[device_index];
        let memory = vm.memory.inner();
        let base = 0x1000 + device_index as u64 * 0x10000;
        let desc = base;
        let avail = base + 0x1000;
        let used = base + 0x2000;
        let header = base + 0x3000;
        let data = base + 0x4000;
        let status = base + 0x5000;
        let kind = device.backend().device_id();
        let queue = u32::from(kind == 1); // network TX queue
        if configure {
            for (offset, value) in [
                (VIRTIO_MMIO_STATUS, 1),
                (VIRTIO_MMIO_STATUS, 3),
                (VIRTIO_MMIO_DRIVER_FEATURES_SEL, 1),
                (VIRTIO_MMIO_DRIVER_FEATURES, 1),
                (VIRTIO_MMIO_STATUS, 11),
                (VIRTIO_MMIO_QUEUE_SEL, queue),
                (VIRTIO_MMIO_QUEUE_NUM, 8),
                (VIRTIO_MMIO_QUEUE_DESC_LOW, desc as u32),
                (VIRTIO_MMIO_QUEUE_DRIVER_LOW, avail as u32),
                (VIRTIO_MMIO_QUEUE_DEVICE_LOW, used as u32),
                (VIRTIO_MMIO_QUEUE_READY, 1),
                (VIRTIO_MMIO_STATUS, 15),
            ] {
                device.write(offset, &value.to_le_bytes(), memory).unwrap();
            }
        }
        let descriptor = |index: u64, address: u64, length: u32, flags: u16, next: u16| {
            let at = desc + index * 16;
            memory.write_obj(address, GuestAddress(at)).unwrap();
            memory.write_obj(length, GuestAddress(at + 8)).unwrap();
            memory.write_obj(flags, GuestAddress(at + 12)).unwrap();
            memory.write_obj(next, GuestAddress(at + 14)).unwrap();
        };
        match kind {
            2 => {
                memory
                    .write_obj(
                        crate::devices::virtio_request::VIRTIO_BLK_T_OUT,
                        GuestAddress(header),
                    )
                    .unwrap();
                memory.write_obj(0u32, GuestAddress(header + 4)).unwrap();
                memory.write_obj(0u64, GuestAddress(header + 8)).unwrap();
                memory
                    .write_slice(
                        &[if configure { 0x51 } else { 0xa2 }; 512],
                        GuestAddress(data),
                    )
                    .unwrap();
                descriptor(0, header, 16, 1, 1);
                descriptor(1, data, 512, 1, 2);
                descriptor(2, status, 1, 2, 0);
            }
            1 => {
                let header_bytes = crate::devices::virtio_request::NET_HEADER_BYTES;
                memory
                    .write_slice(
                        &[0; crate::devices::virtio_request::NET_HEADER_BYTES as usize],
                        GuestAddress(header),
                    )
                    .unwrap();
                memory
                    .write_slice(
                        &[if configure { 0x63 } else { 0xb4 }; 64],
                        GuestAddress(data),
                    )
                    .unwrap();
                descriptor(0, header, header_bytes as u32, 1, 1);
                descriptor(1, data, 64, 0, 0);
            }
            4 => descriptor(0, data, 64, 2, 0),
            _ => unreachable!(),
        }
        let consumed = if configure { 0u16 } else { 1u16 };
        memory
            .write_obj(0u16, GuestAddress(avail + 4 + u64::from(consumed) * 2))
            .unwrap();
        memory
            .write_obj(consumed + 1, GuestAddress(avail + 2))
            .unwrap();
        assert!(matches!(
            device
                .write(VIRTIO_MMIO_QUEUE_NOTIFY, &queue.to_le_bytes(), memory)
                .unwrap(),
            MmioWriteEffect::NotifyQueue(_)
        ));
        assert!(device.process_queue(queue as usize, memory));
        deliver_virtio_interrupt_with(device, queue as usize, |irq, asserted| {
            vm.vm.set_irq_line(irq, asserted)
        })
        .unwrap();
    }
    let mut vm = spin_vm();
    for _ in 0..3 {
        vm.step().unwrap();
    }
    const SYSENTER_ESP: u32 = 0x175;
    const REQUIRED_MSR_MARKER: u64 = 0x1234_5000;
    assert!(vm.snapshot_msr_indices.contains(&SYSENTER_ESP));
    for vcpu in &vm.vcpus {
        let msrs = kvm_bindings::Msrs::from_entries(&[kvm_bindings::kvm_msr_entry {
            index: SYSENTER_ESP,
            data: REQUIRED_MSR_MARKER,
            ..Default::default()
        }])
        .unwrap();
        assert_eq!(vcpu.set_msrs(&msrs).unwrap(), 1);
        let mut xcrs = vcpu.get_xcrs().unwrap();
        xcrs.xcrs[0].value |= 3; // Retain active x87/SSE state in the selected profile.
        vcpu.set_xcrs(&xcrs).unwrap();
        let mut xsave = vcpu.get_xsave().unwrap();
        const XMM3_WORD: usize = (160 + 3 * 16) / std::mem::size_of::<u32>();
        const XSTATE_BV_WORD: usize = 512 / std::mem::size_of::<u32>();
        xsave.region[XMM3_WORD..XMM3_WORD + 4].fill(0xa5a5_a5a5);
        xsave.region[XSTATE_BV_WORD] |= 1 << 1; // SSE is active, not init-state.
        vcpu.set_xsave(&xsave).unwrap();
        let mut events = vcpu.get_vcpu_events().unwrap();
        events.nmi.pending = 1;
        events.nmi.masked = 1;
        events.flags |= kvm_bindings::KVM_VCPUEVENT_VALID_NMI_PENDING;
        vcpu.set_vcpu_events(&events).unwrap();
    }
    vm.entropy.fill_bytes(&mut [0; 17]);
    vm.serial.write(7, 0xa5).unwrap(); // UART scratch register is part of exact state.
    for index in 0..vm.virtio_devices.len() {
        request(&mut vm, index, true);
    }
    let checkpoint = vm.snapshot().unwrap();
    assert_eq!(checkpoint.serial_state.scratch, 0xa5);
    assert!(checkpoint
        .virtio_snapshots
        .iter()
        .all(|device| device.transport.interrupt_status == 1));
    let encoded = serde_json::to_vec(&checkpoint).unwrap();
    let decoded: crate::snapshot::VmSnapshot = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        serde_json::to_value(&checkpoint).unwrap()
    );
    for index in 0..vm.virtio_devices.len() {
        request(&mut vm, index, false);
    }
    let control = vm.snapshot().unwrap();
    // Execute the continuation instead of comparing host-advancing IA32_TSC samples.
    // The complete architecture record already round-tripped without field loss.
    vm.scheduler.drain_trace().unwrap();
    let control_future = future(&mut vm, false, Duration::ZERO);
    vm.restore(&decoded).unwrap();
    for vcpu in &vm.vcpus {
        let mut msrs = kvm_bindings::Msrs::from_entries(&[kvm_bindings::kvm_msr_entry {
            index: SYSENTER_ESP,
            ..Default::default()
        }])
        .unwrap();
        assert_eq!(vcpu.get_msrs(&mut msrs).unwrap(), 1);
        assert_eq!(msrs.as_slice()[0].data, REQUIRED_MSR_MARKER);
        assert_eq!(vcpu.get_fpu().unwrap().xmm[3], [0xa5; 16]);
        let events = vcpu.get_vcpu_events().unwrap();
        assert_eq!((events.nmi.pending, events.nmi.masked), (1, 1));
    }
    for index in 0..vm.virtio_devices.len() {
        request(&mut vm, index, false);
    }
    let restored = vm.snapshot().unwrap();
    assert_eq!(restored.memory.materialize(), control.memory.materialize());
    assert_eq!(
        serde_json::to_value(&restored.virtio_snapshots).unwrap(),
        serde_json::to_value(&control.virtio_snapshots).unwrap()
    );
    assert_eq!(restored.scheduler_snapshot, control.scheduler_snapshot);
    assert_eq!(restored.hlt_latched_vcpus, control.hlt_latched_vcpus);
    assert_eq!(restored.pit_snapshot, control.pit_snapshot);
    assert_eq!(restored.last_kvm_pit_mode, control.last_kvm_pit_mode);
    assert_eq!(
        serde_json::to_value(&restored.fault_engine_snapshot).unwrap(),
        serde_json::to_value(&control.fault_engine_snapshot).unwrap()
    );
    assert_eq!(
        (
            restored.exits_since_last_sdk,
            restored.panic_detected,
            restored.panic_match_state,
            restored.coverage_active
        ),
        (
            control.exits_since_last_sdk,
            control.panic_detected,
            control.panic_match_state,
            control.coverage_active
        )
    );
    assert_eq!(
        (
            restored.virtual_tsc,
            restored.exit_count,
            restored.io_exit_count
        ),
        (
            control.virtual_tsc,
            control.exit_count,
            control.io_exit_count
        )
    );
    assert_eq!(
        serde_json::to_value(&restored.entropy).unwrap(),
        serde_json::to_value(&control.entropy).unwrap()
    );
    // Compare the UART and delivered IRQ state, not just transport pending flags.
    let shell_device_state = |snapshot: &crate::snapshot::VmSnapshot| {
        let json = serde_json::to_value(snapshot).unwrap();
        (
            json["pic_master"].clone(),
            json["pic_slave"].clone(),
            json["ioapic"].clone(),
            json["serial_state"].clone(),
        )
    };
    assert_eq!(shell_device_state(&restored), shell_device_state(&control));
    vm.scheduler.drain_trace().unwrap();
    assert_eq!(future(&mut vm, false, Duration::ZERO), control_future);
}

/// Keep live KVM objects owned while making every shell descriptor reject ioctls.
/// Drop repairs the descriptors before architectural unchanged-state observations.
struct UnavailableKvmDescriptors(Vec<(std::os::fd::RawFd, std::fs::File)>);

impl UnavailableKvmDescriptors {
    fn replace(vm: &DeterministicVm) -> Self {
        use std::os::fd::{AsRawFd, FromRawFd};
        let not_kvm = std::fs::File::open("/dev/null").unwrap();
        let mut retained = Self(Vec::new());
        for descriptor in [vm.kvm.as_raw_fd(), vm.vm.as_raw_fd()]
            .into_iter()
            .chain(vm.vcpus.iter().map(AsRawFd::as_raw_fd))
        {
            // SAFETY: dup retains this live descriptor as a fresh owned file.
            let duplicate = unsafe { libc::dup(descriptor) };
            assert!(duplicate >= 0);
            // SAFETY: duplicate is uniquely owned and live.
            let saved = unsafe { std::fs::File::from_raw_fd(duplicate) };
            retained.0.push((descriptor, saved));
            // SAFETY: dup2 preserves Rust ownership and the original KVM mmap.
            assert_eq!(
                unsafe { libc::dup2(not_kvm.as_raw_fd(), descriptor) },
                descriptor
            );
        }
        retained
    }
}

impl Drop for UnavailableKvmDescriptors {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        for (descriptor, saved) in &self.0 {
            // SAFETY: both descriptors remain owned and live until this drop ends.
            assert_eq!(
                unsafe { libc::dup2(saved.as_raw_fd(), *descriptor) },
                *descriptor
            );
        }
    }
}

#[test]
// r[verify chaoscontrol.deterministic_snapshots.preflight]
fn invalid_kernel_device_identity_and_clock_reject_before_any_restore_effect() {
    let mut vm = spin_vm();
    for _ in 0..3 {
        vm.step().unwrap();
    }
    let checkpoint = vm.snapshot().unwrap();
    // Keep the live destination observably different from the restore source.
    // A late rejection must not silently replace memory or rewind the vCPU.
    const MARKER_ADDRESS: u64 = 0x8000;
    const DESTINATION_MARKER: [u8; 64] = [0x5a; 64];
    vm.memory
        .inner()
        .write_slice(&DESTINATION_MARKER, GuestAddress(MARKER_ADDRESS))
        .unwrap();
    vm.step().unwrap();
    let before_memory = vm.memory.dump();
    let before_registers: Vec<_> = vm
        .vcpus
        .iter()
        .map(|vcpu| vcpu.get_regs().unwrap())
        .collect();
    let before_schedule = vm.scheduler.state_id();
    let before_counters = (vm.exit_count, vm.virtual_tsc.read());
    let before_devices = serde_json::to_value(vm.capture_virtio_snapshots().unwrap()).unwrap();
    let before_assertions = serde_json::to_value(vm.fault_engine.snapshot()).unwrap();
    let before_serial = vm.serial.state();
    let unavailable = UnavailableKvmDescriptors::replace(&vm);
    vm.validate_fault_engine_snapshot(&checkpoint).unwrap();
    let mut boundary = checkpoint.clone();
    boundary.pit.channels[1].read_state = 4; // Partially read word, not a boolean.
    boundary.pit.channels[1].write_state = 4;
    boundary.pit.channels[1].mode = u8::MAX; // Admitted KVM reset/unprogrammed mode.
    boundary.pit.channels[1].count = 65_536;
    vm.validate_fault_engine_snapshot(&boundary).unwrap();
    let mut two_pending_nmis = checkpoint.clone();
    two_pending_nmis.vcpu_snapshots[0].events.nmi.pending = 2;
    two_pending_nmis.vcpu_snapshots[0].events.nmi.masked = 0;
    vm.validate_fault_engine_snapshot(&two_pending_nmis)
        .unwrap();
    drop(unavailable);
    for corruption in 0..31 {
        let mut invalid = checkpoint.clone();
        let component = match corruption {
            0 => {
                invalid.pic_master.chip_id = kvm_bindings::KVM_IRQCHIP_PIC_SLAVE;
                "pic_master"
            }
            1 => {
                invalid.pic_slave.chip_id = kvm_bindings::KVM_IRQCHIP_IOAPIC;
                "pic_slave"
            }
            2 => {
                invalid.ioapic.chip_id = u32::MAX;
                "ioapic"
            }
            3 => {
                invalid.pit_snapshot.tsc_khz = 0;
                "PIT frequency"
            }
            4 => {
                invalid.pit_snapshot.tsc_khz += 1;
                "PIT frequency"
            }
            5 => {
                invalid.last_kvm_pit_mode = 8;
                "PIT mirror mode"
            }
            6 => {
                invalid.vcpu_snapshots[0].mp_state.mp_state = u32::MAX;
                "mp_state"
            }
            7 => {
                invalid.vcpu_snapshots[0].events.nmi.pending = 3;
                "vcpu_events"
            }
            8 => {
                invalid.vcpu_snapshots[0].sregs.cs.present = 2;
                "sregs.segment"
            }
            9 => {
                invalid.vcpu_snapshots[0].xcrs.nr_xcrs = u32::MAX;
                "xcrs.inventory"
            }
            10 => {
                invalid.vcpu_snapshots[0].msrs.pop();
                "MSR"
            }
            11 => {
                invalid.vcpu_snapshots[0].xsave.pop();
                "XSAVE"
            }
            12 => {
                // SAFETY: the checkpoint carries the admitted PIC identity.
                let mut pic = unsafe { invalid.pic_master.chip.pic };
                pic.priority_add = 8;
                invalid.pic_master.chip.pic = pic;
                "pic_master"
            }
            13 => {
                // SAFETY: the checkpoint carries the admitted PIC identity.
                let mut pic = unsafe { invalid.pic_slave.chip.pic };
                pic.auto_eoi = 2;
                invalid.pic_slave.chip.pic = pic;
                "pic_slave"
            }
            14 => {
                // SAFETY: the checkpoint carries the admitted IOAPIC identity.
                let mut ioapic = unsafe { invalid.ioapic.chip.ioapic };
                ioapic.id = 16;
                invalid.ioapic.chip.ioapic = ioapic;
                "ioapic"
            }
            15 => {
                // SAFETY: the checkpoint carries the admitted IOAPIC identity.
                let mut ioapic = unsafe { invalid.ioapic.chip.ioapic };
                ioapic.ioregsel = 256;
                invalid.ioapic.chip.ioapic = ioapic;
                "ioapic"
            }
            16 => {
                invalid.pit.flags |= 1 << 31;
                "pit.flags"
            }
            17 => {
                invalid.pit.channels[0].count = 65_537;
                "pit.channel"
            }
            18 => {
                invalid.pit.channels[0].read_state = 5;
                "pit.channel"
            }
            19 => {
                invalid.pit.channels[0].gate = 2;
                "pit.channel"
            }
            20 => {
                invalid.clock.flags |= 1 << 31;
                "clock"
            }
            21 => {
                invalid.clock.pad[0] = 1;
                "clock"
            }
            22 => {
                invalid.vcpu_snapshots[0].sregs.cr8 = 16;
                "sregs.cr8"
            }
            23 => {
                invalid.vcpu_snapshots[0].fpu.mxcsr = 1 << 16;
                "fpu.mxcsr"
            }
            24 => {
                invalid.vcpu_snapshots[0].xsave[26] = 1;
                "fpu.mxcsr"
            }
            25 => {
                invalid.vcpu_snapshots[0].xsave[520] = 1;
                "xsave.header"
            }
            26 => {
                invalid.vcpu_snapshots[0].xsave[528] = 1;
                "xsave.header"
            }
            27 => {
                invalid.pit.channels[1].mode = 6;
                "pit.channel"
            }
            28 => {
                invalid.vcpu_snapshots[0].regs.rflags = 0;
                "regs.rflags"
            }
            29 => {
                invalid.vcpu_snapshots[0].regs.rflags |= 1 << 3;
                "regs.rflags"
            }
            30 => {
                invalid.vcpu_snapshots[0].debug_regs.flags = 1;
                "debug_regs.flags"
            }
            _ => unreachable!(),
        };
        let unavailable = UnavailableKvmDescriptors::replace(&vm);
        let raw_results = (corruption >= 6).then(|| {
            (
                invalid.restore(&vm.vcpus, &vm.vm, vm.memory.inner()),
                invalid.restore_devices_only(&vm.vcpus, &vm.vm),
            )
        });
        let result = vm.restore(&invalid);
        drop(unavailable);
        if let Some((full, devices)) = raw_results {
            for result in [full, devices] {
                let error = result.unwrap_err();
                assert!(error.to_string().contains(component), "{error}");
            }
        }
        let error = result.unwrap_err();
        assert!(matches!(error, VmError::Snapshot { .. }));
        assert!(error.to_string().contains(component), "{error}");
        assert_eq!(vm.memory.dump(), before_memory);
        let registers: Vec<_> = vm
            .vcpus
            .iter()
            .map(|vcpu| vcpu.get_regs().unwrap())
            .collect();
        assert_eq!(registers, before_registers);
        assert_eq!(vm.scheduler.state_id(), before_schedule);
        assert_eq!((vm.exit_count, vm.virtual_tsc.read()), before_counters);
        assert_eq!(
            serde_json::to_value(vm.capture_virtio_snapshots().unwrap()).unwrap(),
            before_devices
        );
        assert!(vm.schedule_execution_poison().is_none());
        assert_eq!(
            serde_json::to_value(vm.fault_engine.snapshot()).unwrap(),
            before_assertions
        );
        assert_eq!(vm.serial.state(), before_serial);
    }
    vm.restore(&checkpoint).unwrap();
    assert_eq!(vm.memory.dump(), checkpoint.memory.materialize());
    assert!(!vm.step().unwrap());
}

// r[verify chaoscontrol.deterministic_snapshots.restore_atomicity]
#[test]
fn required_pit_restore_ioctl_failure_after_memory_write_permanently_blocks_resume() {
    use std::os::fd::{AsRawFd, FromRawFd};

    const MARKER_ADDRESS: u64 = 0x8000;
    const SOURCE_MARKER: [u8; 64] = [0xa5; 64];
    const DESTINATION_MARKER: [u8; 64] = [0x5a; 64];
    let mut vm = spin_vm();
    vm.memory
        .inner()
        .write_slice(&SOURCE_MARKER, GuestAddress(MARKER_ADDRESS))
        .unwrap();
    let checkpoint = vm.snapshot().unwrap();
    vm.memory
        .inner()
        .write_slice(&DESTINATION_MARKER, GuestAddress(MARKER_ADDRESS))
        .unwrap();
    vm.step().unwrap();
    vm.validate_fault_engine_snapshot(&checkpoint).unwrap();
    vm.complete_pending_kvm_exits_for_snapshot().unwrap();

    // This real VM admits the required dirty-log read but intentionally lacks
    // a PIT. The original vCPUs retain their original kernel VM, so quiesce
    // succeeds; the first required device-restore ioctl then fails after the
    // destination's guest memory has actually been replaced.
    let without_pit = vm.kvm.create_vm().unwrap();
    let memory_region = kvm_bindings::kvm_userspace_memory_region {
        slot: 0,
        guest_phys_addr: 0,
        memory_size: MEMORY_BYTES as u64,
        userspace_addr: vm.memory.inner().get_host_address(GuestAddress(0)).unwrap() as u64,
        flags: kvm_bindings::KVM_MEM_LOG_DIRTY_PAGES,
    };
    // SAFETY: vm.memory owns the mapping until after without_pit drops. This
    // fixture performs no KVM_RUN against the replacement VM.
    unsafe { without_pit.set_user_memory_region(memory_region) }.unwrap();
    let descriptor = vm.vm.as_raw_fd();
    // SAFETY: dup returns a fresh owned descriptor retaining the original VM.
    let saved_raw = unsafe { libc::dup(descriptor) };
    assert!(saved_raw >= 0);
    // SAFETY: saved_raw is uniquely owned and live.
    let saved = unsafe { std::fs::File::from_raw_fd(saved_raw) };
    // SAFETY: dup2 replaces the kernel target without closing an independently
    // owned Rust object or changing descriptor ownership. Both VMs remain live.
    assert_eq!(
        unsafe { libc::dup2(without_pit.as_raw_fd(), descriptor) },
        descriptor
    );
    // Preflight remains pure and succeeds even with a different shell fd.
    vm.validate_fault_engine_snapshot(&checkpoint).unwrap();
    let result = vm.restore(&checkpoint);
    // Repair the descriptor before checking the software poison: rejection
    // must not be merely another required ioctl failing on the fixture VM.
    assert_eq!(
        unsafe { libc::dup2(saved.as_raw_fd(), descriptor) },
        descriptor
    );
    match result {
        Err(VmError::SnapshotRestoreFailed {
            phase: SnapshotRestorePhase::MemoryAndKernelState,
            detail,
        }) => assert!(detail.contains("Failed to set PIT"), "{detail}"),
        other => panic!("expected required PIT restore failure, got {other:?}"),
    }
    assert_eq!(vm.memory.dump(), checkpoint.memory.materialize());
    assert!(!vm.panic_detected, "a restore failure is not a guest panic");
    let before = (
        vm.exit_count,
        vm.virtual_tsc.read(),
        vm.scheduler.state_id(),
    );
    assert!(matches!(
        vm.step(),
        Err(VmError::ScheduleExecutionPoisoned { .. })
    ));
    assert!(matches!(
        vm.snapshot(),
        Err(VmError::ScheduleExecutionPoisoned { .. })
    ));
    assert!(matches!(
        vm.restore(&checkpoint),
        Err(VmError::ScheduleExecutionPoisoned { .. })
    ));
    assert_eq!(
        (
            vm.exit_count,
            vm.virtual_tsc.read(),
            vm.scheduler.state_id()
        ),
        before
    );
    assert_eq!(vm.memory.dump(), checkpoint.memory.materialize());
}
