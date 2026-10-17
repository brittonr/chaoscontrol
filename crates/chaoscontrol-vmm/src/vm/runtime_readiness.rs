//! Exact-runtime regression scenarios; these use the real DeterministicVm shell.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

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
fn pure_preflight_retains_capabilities_and_isolates_topology_rejections() {
    use std::os::fd::{FromRawFd, IntoRawFd};
    let mut vm = spin_vm();
    let checkpoint = vm.snapshot().unwrap();
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
    for corruption in 0..5 {
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
