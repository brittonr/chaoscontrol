# Virtio MMIO Transport Implementation

## Overview

This implementation adds a complete virtio MMIO transport layer to ChaosControl, enabling proper I/O between guest virtio drivers and deterministic device backends. This replaces the previous port-mapped device stubs with spec-compliant virtio 1.2 MMIO devices.

## Architecture

### Components

1. **Virtio MMIO Transport** (`devices/virtio_mmio.rs`)
   - Implements virtio 1.2 MMIO register interface (version 2, legacy-free)
   - Handles device discovery, feature negotiation, and virtqueue setup
   - Provides split virtqueue implementation with descriptor chains
   - Generic backend trait (`VirtioBackend`) for device implementations

2. **Virtio Block Backend** (`devices/virtio_block.rs`)
   - Device ID: 2 (virtio-blk)
   - Wraps `DeterministicBlock` for in-memory storage
   - Supports read/write operations through virtqueue descriptors
   - 16 MB disk by default
   - Single request queue

3. **Virtio Net Backend** (`devices/virtio_net.rs`)
   - Device ID: 1 (virtio-net)
   - Wraps `DeterministicNet` for packet buffering
   - MAC address: `52:54:00:12:34:56`
   - Two queues: RX (receive from host) and TX (transmit to host)

4. **Virtio Entropy Backend** (`devices/virtio_entropy.rs`)
   - Device ID: 4 (virtio-rng)
   - Wraps `DeterministicEntropy` for seeded PRNG
   - Fills guest buffers with deterministic pseudo-random bytes
   - Single request queue

### Memory Layout

Devices are placed in the guest physical address space at fixed locations:

```
0xD000_0000  ─┬─  Device 0: virtio-blk   (IRQ 5)
              │   4 KB MMIO region
0xD000_1000  ─┼─  Device 1: virtio-net   (IRQ 6)
              │   4 KB MMIO region
0xD000_2000  ─┼─  Device 2: virtio-rng   (IRQ 7)
              │   4 KB MMIO region
0xD000_3000  ─┴─  (Future devices)
```

## Virtio MMIO Register Interface

Each device exposes a standard set of registers at offsets within its 4 KB region:

| Offset | Name                    | Access | Description                          |
|--------|-------------------------|--------|--------------------------------------|
| 0x000  | MagicValue              | R      | 0x74726976 ("virt")                  |
| 0x004  | Version                 | R      | 2 (modern virtio)                    |
| 0x008  | DeviceID                | R      | Device type (1=net, 2=blk, 4=rng)    |
| 0x00C  | VendorID                | R      | 0x554D4551 (QEMU-compatible)         |
| 0x010  | DeviceFeatures          | R      | Feature bits (32-bit window)         |
| 0x014  | DeviceFeaturesSel       | W      | Select feature word (0 or 1)         |
| 0x020  | DriverFeatures          | W      | Negotiated features (32-bit window)  |
| 0x024  | DriverFeaturesSel       | W      | Select feature word (0 or 1)         |
| 0x030  | QueueSel                | W      | Select virtqueue index               |
| 0x034  | QueueNumMax             | R      | Max queue size (256)                 |
| 0x038  | QueueNum                | W      | Actual queue size                    |
| 0x044  | QueueReady              | RW     | Queue activation flag                |
| 0x050  | QueueNotify             | W      | Trigger queue processing             |
| 0x060  | InterruptStatus         | R      | Pending interrupt bits               |
| 0x064  | InterruptACK            | W      | Acknowledge interrupts               |
| 0x070  | Status                  | RW     | Device status byte                   |
| 0x080  | QueueDescLow            | W      | Descriptor table address (low 32)    |
| 0x084  | QueueDescHigh           | W      | Descriptor table address (high 32)   |
| 0x090  | QueueDriverLow          | W      | Available ring address (low 32)      |
| 0x094  | QueueDriverHigh         | W      | Available ring address (high 32)     |
| 0x0A0  | QueueDeviceLow          | W      | Used ring address (low 32)           |
| 0x0A4  | QueueDeviceHigh         | W      | Used ring address (high 32)          |
| 0x0FC  | ConfigGeneration        | R      | Config space version counter         |
| 0x100+ | DeviceConfig            | RW     | Device-specific config space         |

## Split Virtqueue Structure

Each virtqueue consists of three memory regions in guest physical memory:

### 1. Descriptor Table
Array of 16-byte descriptors:
```rust
struct VirtqDesc {
    addr: u64,    // Guest physical address
    len: u32,     // Buffer length
    flags: u16,   // NEXT, WRITE, INDIRECT
    next: u16,    // Next descriptor index
}
```

### 2. Available Ring (Driver → Device)
```
u16 flags
u16 idx           // Incremented when driver adds buffers
u16 ring[size]    // Descriptor chain heads
u16 used_event    // Optional (not implemented)
```

### 3. Used Ring (Device → Driver)
```
u16 flags
u16 idx           // Incremented when device completes buffers
struct {
    u32 id;       // Descriptor chain head
    u32 len;      // Total bytes written
} ring[size]
u16 avail_event   // Optional (not implemented)
```

## Device-Specific Behavior

### Virtio Block (virtio-blk)
- **Config space**: `u64 capacity` (in 512-byte sectors)
- **Features**: `VIRTIO_BLK_F_SIZE_MAX`, `VIRTIO_BLK_F_SEG_MAX`
- **Request format**:
  1. Header buffer (device reads): `type` (u32), `_reserved` (u32), `sector` (u64)
  2. Data buffers: read or write depending on request type
  3. Status buffer (device writes): `u8` status code (0=OK, 1=IOERR)

### Virtio Net (virtio-net)
- **Config space**: `u8[6] mac` address
- **Features**: `VIRTIO_NET_F_MAC`
- **RX queue**: Device writes incoming packets to guest buffers
- **TX queue**: Device reads outgoing packets from guest buffers
- Packets are raw Ethernet frames (no virtio-net header in this implementation)

### Virtio Entropy (virtio-rng)
- **Config space**: None
- **Features**: None
- **Request format**: Device fills write-only buffers with deterministic random bytes

## Guest Integration

The kernel is notified of virtio devices via the command line:
```
virtio_mmio.device=4K@0xd0000000:5
virtio_mmio.device=4K@0xd0001000:6
virtio_mmio.device=4K@0xd0002000:7
```

This tells the Linux `virtio_mmio` driver to probe the specified addresses and register the devices.

## VM Exit Handling

When the guest accesses a virtio device's MMIO region, KVM exits with `VcpuExit::MmioRead` or `VcpuExit::MmioWrite`:

1. **MmioRead**: Find device by address → call `device.read(offset, data)` → return data to guest
2. **MmioWrite**: Find device by address → call `device.write(offset, data, mem)` → process queues if notified → raise IRQ if work done

Queue processing:
- Backend's `process_queue()` walks the available ring
- Reads descriptor chains from guest memory
- Performs device-specific I/O (e.g., disk read/write, network TX/RX)
- Adds completed buffers to the used ring
- Returns `true` if an interrupt should be raised

Interrupt delivery:
```rust
if dev.process_queues(mem) {
    vm.set_irq_line(dev.irq(), true);   // Assert
    vm.set_irq_line(dev.irq(), false);  // Deassert (edge-triggered)
}
```

## Testing

### Unit Tests
- **virtio_mmio**: Register reads, feature negotiation, queue setup
- **virtio_block**: Device ID, features, config space
- **virtio_net**: MAC address config, queue count
- **virtio_entropy**: Deterministic output

### Integration Tests
- VM creation initializes 3 virtio devices
- Magic value reads return correct "virt" signature
- Device types match expected IDs (1=net, 2=blk, 4=rng)

### Running Tests
```bash
cargo test --lib -p chaoscontrol-vmm virtio
```

All 22 virtio-specific tests pass, plus 4 VM integration tests.

## Future Enhancements

### Short-term
1. **Virtio-net header support**: Add 12-byte virtio-net header for checksum offload
2. **Multi-descriptor chains**: Improve handling of long I/O requests
3. **Flush support**: Implement `VIRTIO_BLK_T_FLUSH` for fsync semantics

### Medium-term
1. **Event suppression**: Implement `used_event` and `avail_event` for notification batching
2. **Indirect descriptors**: Support `VIRTQ_DESC_F_INDIRECT` for large I/O
3. **Snapshots**: Serialize virtqueue state for checkpoint/restore

### Long-term
1. **Packed virtqueues**: Implement virtio 1.1+ packed queue format for better cache efficiency
2. **More devices**: virtio-console, virtio-balloon, virtio-scsi
3. **Device hotplug**: Dynamic device addition/removal

## References

- [Virtio 1.2 Specification](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)
- [Virtio MMIO Transport](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html#x1-1440002)
- [Split Virtqueues](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html#x1-430007)
- [Virtio Block Device](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html#x1-3740005)
- [Virtio Network Device](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html#x1-2350005)
- [Virtio Entropy Device](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html#x1-3020004)

## Implementation Notes

### Determinism
All device backends wrap deterministic storage/networking/entropy sources:
- `DeterministicBlock`: In-memory disk with fault injection
- `DeterministicNet`: Explicit RX/TX packet queues
- `DeterministicEntropy`: ChaCha20 PRNG with fixed seed

This ensures that identical guest workloads produce identical I/O patterns across runs.

### Performance
This is a **correctness-first** implementation optimized for deterministic replay, not throughput:
- Guest memory copies on every I/O (no zero-copy)
- Synchronous queue processing (no async workers)
- Single-threaded (no parallel I/O)

For production workloads, use KVM's vhost acceleration or Firecracker's virtio-vsock.

### Security
This implementation is **not hardened** against malicious guests:
- No bounds checking on descriptor chains (can cause DoS)
- No resource limits (guest can exhaust host memory)
- No isolation (devices share process address space)

Only use with trusted guest kernels in controlled environments.
