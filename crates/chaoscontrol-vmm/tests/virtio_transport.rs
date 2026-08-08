mod virtio_support;

use chaoscontrol_vmm::devices::virtio_chain::VirtqDesc;
use chaoscontrol_vmm::devices::virtio_mmio::{
    VirtQueue, VirtioBackend, VirtioMmioDevice, VIRTIO_MMIO_MAGIC_VALUE,
    VIRTIO_MMIO_QUEUE_DESC_LOW, VIRTIO_MMIO_QUEUE_DEVICE_LOW, VIRTIO_MMIO_QUEUE_DRIVER_LOW,
    VIRTIO_MMIO_QUEUE_NUM, VIRTIO_MMIO_QUEUE_READY, VIRTIO_MMIO_QUEUE_SEL, VIRTIO_MMIO_STATUS,
};
use chaoscontrol_vmm::devices::virtio_types::{
    QueueViolation, TransportViolation, VirtioFailure, MAX_QUEUE_SIZE,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use virtio_support::*;
use vm_memory::GuestMemoryMmap;

const DUMMY_DEVICE_ID: u32 = 99;
const ONE_QUEUE: usize = 1;
const NON_POWER_OF_TWO_QUEUE: u32 = 3;
const TRUNCATED_QUEUE: u32 = u16::MAX as u32 + 1;
const MAGIC_VALUE: u32 = 0x7472_6976;
const WIDE_READ_BYTES: usize = 8;
const MMIO_REGISTER_BYTES: usize = 4;
const INITIAL_READ_BYTE: u8 = 0xA5;
const UNKNOWN_REGISTER: u64 = 0x06C;
const AHEAD_CURSOR: u16 = 1;

struct DummyBackend {
    calls: Arc<AtomicUsize>,
}

struct CompletingBackend;

impl VirtioBackend for DummyBackend {
    fn device_id(&self) -> u32 {
        DUMMY_DEVICE_ID
    }

    fn device_features(&self) -> u64 {
        0
    }

    fn num_queues(&self) -> usize {
        ONE_QUEUE
    }

    fn process_queue(
        &mut self,
        _queue_idx: usize,
        _queue: &mut VirtQueue,
        _mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(false)
    }

    fn read_config(&self, _offset: u64, data: &mut [u8]) {
        data.fill(0);
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl VirtioBackend for CompletingBackend {
    fn device_id(&self) -> u32 {
        DUMMY_DEVICE_ID
    }

    fn device_features(&self) -> u64 {
        0
    }

    fn num_queues(&self) -> usize {
        ONE_QUEUE
    }

    fn process_queue(
        &mut self,
        _queue_idx: usize,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure> {
        let Some(available) = queue.plan_next(mem)? else {
            return Ok(false);
        };
        queue.stage_completion(available.head_index, 0)?;
        queue.mark_backend_started()?;
        queue.complete(mem, available.head_index, 0)?;
        Ok(true)
    }

    fn read_config(&self, _offset: u64, data: &mut [u8]) {
        data.fill(0);
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn device(calls: Arc<AtomicUsize>) -> VirtioMmioDevice {
    VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(DummyBackend { calls }))
}

fn completing_device() -> VirtioMmioDevice {
    VirtioMmioDevice::new(DEVICE_BASE, DEVICE_IRQ, Box::new(CompletingBackend))
}

#[test]
fn compliant_linux_sequence_activates_exact_queue() {
    let mem = memory();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut device = device(calls.clone());
    negotiate_features(&mut device, &mem);
    configure_queue(&mut device, &mem, 0);
    finish_driver(&mut device, &mem);

    let state = device.live_state();
    assert_eq!(
        state.queues[0].config.map(|config| config.size),
        Some(QUEUE_SIZE)
    );
    assert!(state.queues[0].ready);
    assert!(state.queues[0].failure.is_none());
    assert!(state.queues[0].pending_completion.is_none());
    assert!(!device.interrupt_pending());
    assert!(!device.process_queues(&mem));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn mmio_reads_zero_wide_and_unknown_bytes() {
    let device = device(Arc::new(AtomicUsize::new(0)));
    let mut wide = [INITIAL_READ_BYTE; WIDE_READ_BYTES];
    device.read(VIRTIO_MMIO_MAGIC_VALUE, &mut wide);
    assert_eq!(&wide[..MMIO_REGISTER_BYTES], &MAGIC_VALUE.to_le_bytes());
    assert_eq!(
        &wide[MMIO_REGISTER_BYTES..],
        &[0; WIDE_READ_BYTES - MMIO_REGISTER_BYTES]
    );

    wide.fill(INITIAL_READ_BYTE);
    device.read(UNKNOWN_REGISTER, &mut wide);
    assert_eq!(wide, [0; WIDE_READ_BYTES]);
}

#[test]
fn malformed_mmio_width_sets_typed_needs_reset() {
    let mem = memory();
    let mut device = device(Arc::new(AtomicUsize::new(0)));
    let error = device
        .write(VIRTIO_MMIO_QUEUE_SEL, &[0, 0, 0], &mem)
        .expect_err("short MMIO write");
    assert_eq!(
        error,
        VirtioFailure::Transport(TransportViolation::MmioWidth { actual: 3 })
    );
    let state = device.live_state();
    assert!(state.failure.is_some());
    assert!(!device.interrupt_pending());
}

#[test]
fn invalid_queue_sizes_never_activate_or_process() {
    let oversized_queue = u32::from(MAX_QUEUE_SIZE) * 2;
    for value in [0, NON_POWER_OF_TWO_QUEUE, oversized_queue, TRUNCATED_QUEUE] {
        let mem = memory();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut device = device(calls.clone());
        negotiate_features(&mut device, &mem);
        register(&mut device, &mem, VIRTIO_MMIO_QUEUE_SEL, 0).expect("select queue");
        let error = register(&mut device, &mem, VIRTIO_MMIO_QUEUE_NUM, value)
            .expect_err("invalid queue size");
        assert!(matches!(error, VirtioFailure::Queue(_)));
        assert!(!device.live_state().queues[0].ready);
        assert!(!device.process_queues(&mem));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!device.interrupt_pending());
    }
}

#[test]
fn invalid_geometry_fails_before_ready() {
    let cases = [
        (VIRTIO_MMIO_QUEUE_DESC_LOW, DESCRIPTOR_ADDRESS + 1),
        (VIRTIO_MMIO_QUEUE_DRIVER_LOW, MEMORY_BYTES as u64),
        (VIRTIO_MMIO_QUEUE_DEVICE_LOW, DESCRIPTOR_ADDRESS),
    ];
    for (register_offset, address) in cases {
        let mem = memory();
        let mut device = device(Arc::new(AtomicUsize::new(0)));
        negotiate_features(&mut device, &mem);
        register(&mut device, &mem, VIRTIO_MMIO_QUEUE_SEL, 0).expect("select queue");
        register(
            &mut device,
            &mem,
            VIRTIO_MMIO_QUEUE_NUM,
            u32::from(QUEUE_SIZE),
        )
        .expect("queue size");
        register(
            &mut device,
            &mem,
            VIRTIO_MMIO_QUEUE_DESC_LOW,
            DESCRIPTOR_ADDRESS as u32,
        )
        .expect("descriptor address");
        register(
            &mut device,
            &mem,
            VIRTIO_MMIO_QUEUE_DRIVER_LOW,
            AVAILABLE_ADDRESS as u32,
        )
        .expect("driver address");
        register(
            &mut device,
            &mem,
            VIRTIO_MMIO_QUEUE_DEVICE_LOW,
            USED_ADDRESS as u32,
        )
        .expect("device address");
        register(&mut device, &mem, register_offset, address as u32).expect("staged address");
        let error =
            register(&mut device, &mem, VIRTIO_MMIO_QUEUE_READY, 1).expect_err("invalid geometry");
        assert!(matches!(
            error,
            VirtioFailure::Queue(
                QueueViolation::AddressMisaligned { .. }
                    | QueueViolation::RingOutsideMemory { .. }
                    | QueueViolation::RingOverlap
            )
        ));
        assert!(!device.live_state().queues[0].ready);
        assert!(!device.interrupt_pending());
    }
}

#[test]
fn invalid_status_and_queue_selection_are_typed() {
    let mem = memory();
    let mut status_device = device(Arc::new(AtomicUsize::new(0)));
    let status_error = register(
        &mut status_device,
        &mem,
        chaoscontrol_vmm::devices::virtio_mmio::VIRTIO_MMIO_STATUS,
        15,
    )
    .expect_err("status order");
    assert!(matches!(
        status_error,
        VirtioFailure::Transport(TransportViolation::StatusTransition { .. })
    ));

    let mut queue_device = device(Arc::new(AtomicUsize::new(0)));
    negotiate_features(&mut queue_device, &mem);
    register(&mut queue_device, &mem, VIRTIO_MMIO_QUEUE_SEL, 1).expect("invalid selection staged");
    let queue_error = register(
        &mut queue_device,
        &mem,
        VIRTIO_MMIO_QUEUE_NUM,
        u32::from(QUEUE_SIZE),
    )
    .expect_err("selected queue absent");
    assert!(matches!(
        queue_error,
        VirtioFailure::Transport(TransportViolation::QueueSelection { .. })
    ));
}

#[test]
fn transport_snapshot_restores_negotiation_queue_geometry_and_cursors() {
    let mem = memory();
    let mut transport = device(Arc::new(AtomicUsize::new(0)));
    negotiate_features(&mut transport, &mem);
    configure_queue(&mut transport, &mem, 0);
    finish_driver(&mut transport, &mem);
    let snapshot = transport.snapshot().expect("capture quiescent transport");

    register(&mut transport, &mem, VIRTIO_MMIO_STATUS, 0).expect("reset transport");
    assert_ne!(transport.live_state().status, snapshot.status);

    transport
        .restore_snapshot(&snapshot, &mem)
        .expect("restore exact transport state");
    assert_eq!(transport.snapshot().unwrap(), snapshot);
}

#[test]
fn transport_snapshot_restores_nonzero_cursors_and_pending_interrupt() {
    let mem = memory();
    let mut transport = completing_device();
    negotiate_features(&mut transport, &mem);
    configure_queue(&mut transport, &mem, 0);
    finish_driver(&mut transport, &mem);
    write_descriptor(&mem, 0, VirtqDesc::default());
    publish_head(&mem, 0, AHEAD_CURSOR);
    assert!(transport.process_queues(&mem));
    assert!(transport.interrupt_pending());

    let snapshot = transport.snapshot().expect("capture completed request");
    assert_eq!(snapshot.queues[0].last_avail_idx, AHEAD_CURSOR);
    assert_eq!(snapshot.queues[0].next_used_idx, AHEAD_CURSOR);

    let mut restored = completing_device();
    restored
        .restore_snapshot(&snapshot, &mem)
        .expect("restore nonzero transport state");
    assert!(restored.interrupt_pending());
    assert_eq!(restored.snapshot().unwrap(), snapshot);
}

#[test]
fn transport_snapshot_rejects_queue_cursor_ahead_of_guest_ring() {
    let mem = memory();
    let mut transport = device(Arc::new(AtomicUsize::new(0)));
    negotiate_features(&mut transport, &mem);
    configure_queue(&mut transport, &mem, 0);
    finish_driver(&mut transport, &mem);
    let before = transport.snapshot().expect("capture quiescent transport");
    let mut corrupted = before.clone();
    corrupted.queues[0].last_avail_idx = AHEAD_CURSOR;

    assert!(matches!(
        transport.validate_snapshot(&corrupted, &mem),
        Err(VirtioFailure::Queue(QueueViolation::AvailableDelta { .. }))
    ));
    assert_eq!(transport.snapshot().unwrap(), before);
}
