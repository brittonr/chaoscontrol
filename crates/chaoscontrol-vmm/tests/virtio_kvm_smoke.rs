mod virtio_support;

#[cfg(target_arch = "x86_64")]
mod x86_64_smoke {
    use crate::virtio_support::{memory, DEVICE_IRQ};
    use chaoscontrol_vmm::devices::block::DeterministicBlock;
    use chaoscontrol_vmm::devices::virtio_block::VirtioBlock;
    use chaoscontrol_vmm::devices::virtio_mmio::{VirtioMmioDevice, VIRTIO_MMIO_QUEUE_NUM};
    use chaoscontrol_vmm::devices::virtio_types::{TransportViolation, VirtioFailure};
    use kvm_bindings::kvm_userspace_memory_region;
    use kvm_ioctls::{Kvm, VcpuExit};
    use std::ptr::null_mut;
    use std::slice;

    const KVM_PATH: &str = "/dev/kvm";
    const GUEST_MEMORY_BYTES: usize = 0x4000;
    const GUEST_LOAD_ADDRESS: u64 = 0x1000;
    const MMIO_TARGET_ADDRESS: u64 = 0x8000;
    const MEMORY_SLOT: u32 = 0;
    const VCPU_INDEX: u64 = 0;
    const REQUIRED_RFLAGS: u64 = 2;
    const TEST_DISK_BYTES: usize = 64 * 1024;
    const EXPECTED_MMIO_BYTES: usize = 1;
    const DEVICE_BASE: u64 = MMIO_TARGET_ADDRESS - VIRTIO_MMIO_QUEUE_NUM;
    const MALICIOUS_GUEST_CODE: [u8; 6] = [
        0xC6, 0x06, 0x00, 0x80, 0x00, // movb $0, (0x8000)
        0xF4, // hlt
    ];

    struct KvmMemory {
        address: *mut libc::c_void,
        length: usize,
    }

    impl KvmMemory {
        fn new(length: usize) -> Self {
            let address = unsafe {
                libc::mmap(
                    null_mut(),
                    length,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_ANONYMOUS | libc::MAP_SHARED | libc::MAP_NORESERVE,
                    -1,
                    0,
                )
            };
            assert_ne!(address, libc::MAP_FAILED, "guest mmap failed");
            Self { address, length }
        }

        fn write_code(&self, code: &[u8]) {
            assert!(code.len() <= self.length);
            let memory =
                unsafe { slice::from_raw_parts_mut(self.address.cast::<u8>(), self.length) };
            memory[..code.len()].copy_from_slice(code);
        }
    }

    impl Drop for KvmMemory {
        fn drop(&mut self) {
            let result = unsafe { libc::munmap(self.address, self.length) };
            assert_eq!(result, 0, "guest munmap failed");
        }
    }

    #[test]
    #[ignore = "requires /dev/kvm"]
    fn malicious_guest_mmio_write_keeps_vmm_alive_and_fails_typed() {
        assert!(
            std::path::Path::new(KVM_PATH).exists(),
            "{KVM_PATH} is missing"
        );
        let kvm = Kvm::new().expect("open KVM");
        let vm = kvm.create_vm().expect("create KVM VM");
        let guest_memory = KvmMemory::new(GUEST_MEMORY_BYTES);
        guest_memory.write_code(&MALICIOUS_GUEST_CODE);
        let region = kvm_userspace_memory_region {
            slot: MEMORY_SLOT,
            guest_phys_addr: GUEST_LOAD_ADDRESS,
            memory_size: u64::try_from(GUEST_MEMORY_BYTES).expect("guest memory size"),
            userspace_addr: guest_memory.address as u64,
            flags: 0,
        };
        unsafe {
            vm.set_user_memory_region(region)
                .expect("register guest memory")
        };
        let mut vcpu = vm.create_vcpu(VCPU_INDEX).expect("create vCPU");
        let mut sregs = vcpu.get_sregs().expect("get sregs");
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        vcpu.set_sregs(&sregs).expect("set sregs");
        let mut regs = vcpu.get_regs().expect("get regs");
        regs.rip = GUEST_LOAD_ADDRESS;
        regs.rflags = REQUIRED_RFLAGS;
        vcpu.set_regs(&regs).expect("set regs");

        let transport_memory = memory();
        let block = VirtioBlock::new(DeterministicBlock::new(TEST_DISK_BYTES));
        let mut device = VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(block));
        assert!(device.handles(MMIO_TARGET_ADDRESS));

        match vcpu.run().expect("run malicious guest") {
            VcpuExit::MmioWrite(address, data) => {
                assert_eq!(address, MMIO_TARGET_ADDRESS);
                assert_eq!(data.len(), EXPECTED_MMIO_BYTES);
                let failure = device
                    .write_at(address, data, &transport_memory)
                    .expect_err("malformed MMIO must fail");
                assert_eq!(
                    failure,
                    VirtioFailure::Transport(TransportViolation::MmioWidth {
                        actual: EXPECTED_MMIO_BYTES,
                    })
                );
            }
            exit => panic!("unexpected first KVM exit: {exit:?}"),
        }
        assert!(matches!(vcpu.run().expect("resume guest"), VcpuExit::Hlt));
        assert!(!device.interrupt_pending());
        assert!(device.live_state().failure.is_some());
        assert!(!device.live_state().queues[0].ready);
    }
}
