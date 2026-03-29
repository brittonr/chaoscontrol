# Determinism Logging Design

## Architecture Overview

The determinism logging system consists of four main components:

1. **Binary Log Format** - Compact event encoding for high throughput
2. **Ring Buffer Writer** - In-memory circular buffer with async flush
3. **Event Capture** - Integration points across VMM, fault injection, and scheduler
4. **Diff Analysis** - Tool for comparing logs and finding divergence points

## Binary Log Format

### Header Structure
```
Magic: 0xD10C (determinism log)
Version: u16
VM ID: u32  
Start TSC: u64
Entry Count: u64
```

### Event Types
Each event starts with a 1-byte type tag followed by type-specific payload:

```rust
enum LogEvent {
    VmExit(ExitType, u64 tsc, u64 exit_count),        // Type 0x01
    RngDraw(RngDomain, u64 value),                    // Type 0x02  
    FaultDispatch(FaultType, u64 tick),               // Type 0x03
    SdkCall(SdkCommand, u32 payload_hash),            // Type 0x04
    SchedulerDecision(u16 vcpu_id, SchedReason),      // Type 0x05
    Timestamp(u64 tsc),                               // Type 0xFF (periodic sync)
}
```

### Encoding Details
- All multi-byte values in little-endian format
- Variable-length encoding for common fields (exit_count, tick)
- Enum variants packed into single bytes where possible
- Timestamp sync events every 1000 entries to handle TSC wraparound

## Ring Buffer Architecture

### Memory Layout
```
Ring Buffer (per VM):
- Capacity: 64MB default (configurable)
- Entry size: Variable (8-32 bytes typical)
- Writer: Single producer (VMM thread)
- Reader: Async flush thread
```

### Write Path
1. VMM thread writes to ring buffer head
2. Atomic increment of write pointer
3. If buffer > 80% full, signal flush thread
4. If buffer full, either drop events (fast mode) or block (safe mode)

### Flush Strategy
- **Lazy**: Flush when ring buffer fills or VM terminates
- **Periodic**: Flush every N seconds in background thread  
- **Immediate**: Flush after every write (debug mode only)

## Event Capture Integration

### VM Exit Logging (chaoscontrol-vmm)
```rust
// In run_bounded() main loop
if vm_config.paranoid_log {
    log_writer.write_vm_exit(exit_reason, virtual_tsc, vm.exit_count);
}
```

### RNG Logging (chaoscontrol-rng)
```rust
impl RngProvider {
    fn next(&mut self, domain: RngDomain) -> u64 {
        let value = self.inner_next();
        if let Some(logger) = &self.logger {
            logger.write_rng_draw(domain, value);
        }
        value
    }
}
```

### Fault Dispatch Logging (chaoscontrol-fault)
```rust
fn dispatch_fault(&mut self, fault: &Fault) {
    if let Some(logger) = &self.logger {
        logger.write_fault_dispatch(fault.fault_type, self.current_tick);
    }
    // ... existing dispatch logic
}
```

### Scheduler Logging (chaoscontrol-vmm)
```rust
fn schedule_next_vcpu(&mut self) -> Option<VcpuId> {
    let decision = self.scheduler.next_vcpu();
    if let Some(logger) = &self.logger {
        logger.write_scheduler_decision(decision.vcpu_id, decision.reason);
    }
    Some(decision.vcpu_id)
}
```

## Diff Algorithm

### Stream Comparison
1. **Parallel Iteration** - Read both logs simultaneously 
2. **Event Matching** - Compare events by type and logical timestamp
3. **Divergence Detection** - First mismatch identifies divergence point
4. **Context Generation** - Show 10 events before/after divergence

### Diff Output Format
```
Divergence at event #42851:

Log A: VmExit(IoIn, tsc=1234567890, exit_count=42851)
Log B: VmExit(IoOut, tsc=1234567891, exit_count=42851)

Context (preceding 5 events):
  #42846: RngDraw(timer, 0xDEADBEEF)
  #42847: SchedulerDecision(vcpu=0, timeslice_expired)
  #42848: FaultDispatch(timer_irq, tick=1000)
  #42849: SdkCall(GetTime, hash=0x12345678)
  #42850: VmExit(CPUID, tsc=1234567889, exit_count=42850)
```

## CLI Integration

### VMM Configuration
```bash
# Enable via command line
chaoscontrol run --paranoid-log ./debug-logs vm.img

# Enable via config file
vm_config.paranoid_log = true
vm_config.log_output_dir = "./debug-logs"
```

### Log Analysis
```bash
# Compare two runs
chaoscontrol-replay diff run1/vm-0.dlog run2/vm-0.dlog

# Inspect single log
chaoscontrol-replay show run1/vm-0.dlog --events 1000

# Export to text format
chaoscontrol-replay export run1/vm-0.dlog --format=json
```

## Performance Considerations

### Throughput Targets
- 1M+ events/second sustained write rate
- < 5% overhead in typical workloads  
- < 100ms pause times for ring buffer flush

### Memory Usage
- Default 64MB ring buffer per VM
- ~1KB overhead per 1000 events
- Optional compression for long-term storage

### Configuration Tuning
- `log_buffer_size`: Ring buffer capacity
- `flush_threshold`: Trigger async flush at N% full
- `drop_policy`: Drop vs block when buffer full