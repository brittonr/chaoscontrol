use chaoscontrol_vmm::devices::virtio_chain::{plan_descriptor_chain, VirtqDesc};
use chaoscontrol_vmm::devices::virtio_request::{
    plan_block_request, plan_entropy_request, plan_net_request, BlockRequestHeader, NetDirection,
};
use chaoscontrol_vmm::devices::virtio_types::{VirtioLimits, MAX_QUEUE_SIZE};
use chaoscontrol_vmm::devices::virtio_validation::{
    available_element_address, used_element_address, validate_available_delta,
    validate_queue_config, MemoryRegion, RawQueueConfig,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

const CORPUS_CASES: u64 = 4096;
const DESCRIPTOR_CASES: usize = 8;
const MEMORY_BYTES: u64 = 128 * 1024;
const DISK_BYTES: u64 = 1024 * 1024;
const LCG_MULTIPLIER: u64 = 6364136223846793005;
const LCG_INCREMENT: u64 = 1442695040888963407;

#[derive(Clone, Copy)]
struct CorpusValue(u64);

impl CorpusValue {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(LCG_MULTIPLIER)
            .wrapping_add(LCG_INCREMENT);
        self.0
    }

    fn next_u32(&mut self) -> u32 {
        let bytes = self.next().to_le_bytes();
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn next_u16(&mut self) -> u16 {
        let bytes = self.next().to_le_bytes();
        u16::from_le_bytes([bytes[0], bytes[1]])
    }
}

#[test]
fn generated_guest_inputs_never_panic_or_escape_limits() {
    let limits = VirtioLimits::default();
    let memory = [MemoryRegion {
        start: 0,
        length: MEMORY_BYTES,
    }];
    for seed in 0..CORPUS_CASES {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let mut corpus = CorpusValue(seed);
            let raw = RawQueueConfig {
                size: corpus.next_u32(),
                descriptor_address: corpus.next(),
                driver_address: corpus.next(),
                device_address: corpus.next(),
            };
            if let Ok(config) = validate_queue_config(raw, MAX_QUEUE_SIZE, &memory, limits) {
                assert!(config.size > 0);
                assert!(config.size <= limits.max_queue_size);
                assert!(available_element_address(config, corpus.next_u16()).is_some());
                assert!(used_element_address(config, corpus.next_u16()).is_some());
            }
            let available = corpus.next_u16();
            let last = corpus.next_u16();
            if let Ok(delta) = validate_available_delta(last, available, MAX_QUEUE_SIZE) {
                assert!(delta <= MAX_QUEUE_SIZE);
            }

            let mut descriptors = [VirtqDesc::default(); DESCRIPTOR_CASES];
            for descriptor in &mut descriptors {
                *descriptor = VirtqDesc {
                    addr: corpus.next(),
                    len: corpus.next_u32(),
                    flags: corpus.next_u16(),
                    next: corpus.next_u16(),
                };
            }
            let queue_size = corpus.next_u16() % (MAX_QUEUE_SIZE + 1);
            let head = corpus.next_u16();
            if let Ok(chain) =
                plan_descriptor_chain(&descriptors, head, queue_size, &memory, limits)
            {
                assert!(chain.count() <= limits.max_chain_descriptors);
                assert!(chain.aggregate_length() <= limits.max_aggregate_bytes);
                let header = BlockRequestHeader {
                    operation: corpus.next_u32(),
                    reserved: corpus.next_u32(),
                    sector: corpus.next(),
                };
                let _ = plan_block_request(&chain, header, DISK_BYTES, limits);
                let _ = plan_net_request(&chain, NetDirection::Transmit, corpus.next(), limits);
                let _ = plan_net_request(&chain, NetDirection::Receive, corpus.next(), limits);
                let _ = plan_entropy_request(&chain, limits);
            }
        }));
        assert!(outcome.is_ok(), "generated case {seed} panicked");
    }
}

#[test]
fn zero_sized_forged_validated_config_does_not_panic() {
    use chaoscontrol_vmm::devices::virtio_validation::{CheckedRange, ValidatedQueueConfig};
    let forged = ValidatedQueueConfig {
        size: 0,
        descriptors: CheckedRange::default(),
        available: CheckedRange::default(),
        used: CheckedRange::default(),
    };
    assert_eq!(available_element_address(forged, 0), None);
    assert_eq!(used_element_address(forged, 0), None);
}
