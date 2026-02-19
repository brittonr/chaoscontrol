//! Deterministic vCPU preemption via hardware performance counters.
//!
//! Uses `perf_event_open()` with `exclude_host=1` to count only guest
//! instructions. Two modes are supported:
//!
//! - **Counting mode** (`new()`): read instruction count on demand.
//! - **Overflow mode** (`with_overflow(N)`): delivers SIGIO after N
//!   guest instructions, interrupting `vcpu.run()`.
//!
//! For deterministic SMP, we use overflow mode with a "margin" — the PMU
//! fires at `quantum - margin` instructions, then KVM single-stepping
//! finishes the exact remainder.

use std::io;
use std::os::fd::RawFd;

/// perf_event_attr layout (subset of fields we need).
#[repr(C)]
#[derive(Debug)]
struct PerfEventAttr {
    type_: u32,
    size: u32,
    config: u64,
    sample_period: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup_events: u32,
    bp_type: u32,
    config1: u64,
    config2: u64,
    branch_sample_type: u64,
    sample_regs_user: u64,
    sample_stack_user: u32,
    clockid: i32,
    sample_regs_intr: u64,
    aux_watermark: u32,
    sample_max_stack: u16,
    __reserved_2: u16,
    aux_sample_size: u32,
    __reserved_3: u32,
    sig_data: u64,
    config3: u64,
}

const FLAG_DISABLED: u64 = 1 << 0;
const FLAG_EXCLUDE_HOST: u64 = 1 << 26;
const PERF_TYPE_HARDWARE: u32 = 0;
const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
const PERF_EVENT_IOC_ENABLE: libc::c_ulong = 0x2400;
const PERF_EVENT_IOC_DISABLE: libc::c_ulong = 0x2401;
const PERF_EVENT_IOC_RESET: libc::c_ulong = 0x2403;

/// Hardware instruction counter for guest instruction counting/preemption.
#[derive(Debug)]
pub struct InstructionCounter {
    fd: RawFd,
}

impl InstructionCounter {
    /// Create a counter in counting mode (no signals).
    pub fn new() -> io::Result<Self> {
        Self::create(0, false)
    }

    /// Create a counter in overflow mode: delivers SIGIO after `period`
    /// guest instructions. The signal interrupts `vcpu.run()` → EINTR.
    pub fn with_overflow(period: u64) -> io::Result<Self> {
        debug_assert!(period > 0, "overflow period must be positive");
        Self::create(period, true)
    }

    fn create(sample_period: u64, async_signal: bool) -> io::Result<Self> {
        let mut attr = PerfEventAttr {
            type_: PERF_TYPE_HARDWARE,
            size: std::mem::size_of::<PerfEventAttr>() as u32,
            config: PERF_COUNT_HW_INSTRUCTIONS,
            sample_period,
            sample_type: 0,
            read_format: 0,
            flags: FLAG_DISABLED | FLAG_EXCLUDE_HOST,
            wakeup_events: if sample_period > 0 { 1 } else { 0 },
            bp_type: 0,
            config1: 0,
            config2: 0,
            branch_sample_type: 0,
            sample_regs_user: 0,
            sample_stack_user: 0,
            clockid: 0,
            sample_regs_intr: 0,
            aux_watermark: 0,
            sample_max_stack: 0,
            __reserved_2: 0,
            aux_sample_size: 0,
            __reserved_3: 0,
            sig_data: 0,
            config3: 0,
        };

        // SAFETY: perf_event_open syscall with valid attr pointer.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                &mut attr as *mut PerfEventAttr,
                0i32,  // pid = current thread
                -1i32, // cpu = any
                -1i32, // group_fd = none
                0u64,  // flags
            )
        } as RawFd;

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        if async_signal {
            // Set up async I/O so counter overflow delivers SIGIO.
            // SAFETY: fcntl on valid fd with standard flags.
            unsafe {
                libc::fcntl(fd, libc::F_SETOWN, libc::gettid());
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_ASYNC);
            }
        }

        Ok(Self { fd })
    }

    /// Reset counter to zero and enable counting.
    /// Use at the start of a new vCPU turn.
    #[inline]
    pub fn reset_and_enable(&self) {
        // SAFETY: ioctl on valid perf fd.
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_RESET, 0);
            libc::ioctl(self.fd, PERF_EVENT_IOC_ENABLE, 0);
        }
    }

    /// Enable counting without resetting.
    /// Use to resume counting after a pause (e.g., before vcpu.run()).
    #[inline]
    pub fn resume(&self) {
        // SAFETY: ioctl on valid perf fd.
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_ENABLE, 0);
        }
    }

    /// Disable counting (pause). Counter value is preserved.
    #[inline]
    pub fn disable(&self) {
        // SAFETY: ioctl on valid perf fd.
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_DISABLE, 0);
        }
    }

    /// Read current instruction count since last enable/reset.
    #[inline]
    pub fn read(&self) -> u64 {
        let mut value: u64 = 0;
        // SAFETY: read into valid u64 buffer from valid perf fd.
        let n = unsafe {
            libc::read(
                self.fd,
                &mut value as *mut u64 as *mut libc::c_void,
                std::mem::size_of::<u64>(),
            )
        };
        if n == std::mem::size_of::<u64>() as isize {
            value
        } else {
            0
        }
    }

    /// Read and reset atomically (read value, then reset counter to 0).
    #[inline]
    pub fn read_and_reset(&self) -> u64 {
        let value = self.read();
        // SAFETY: ioctl on valid perf fd.
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_RESET, 0);
        }
        value
    }

    /// Install no-op SIGIO handler (for overflow mode).
    /// Must NOT set SA_RESTART — we need the signal to interrupt vcpu.run().
    pub fn install_sigio_handler() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            // SAFETY: sigaction with valid handler function pointer.
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = noop_handler as *const () as usize;
                sa.sa_flags = 0; // NO SA_RESTART
                libc::sigaction(libc::SIGIO, &sa, std::ptr::null_mut());
            }
        });
        extern "C" fn noop_handler(_sig: libc::c_int) {}
    }
}

impl Drop for InstructionCounter {
    fn drop(&mut self) {
        // SAFETY: close valid fd.
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counting_mode() {
        match InstructionCounter::new() {
            Ok(counter) => {
                counter.reset_and_enable();
                let count = counter.read();
                counter.disable();
                // Counter works — in non-VM context, exclude_host doesn't
                // filter (no guest mode), so we see host instructions.
                // In real use, the count is dominated by guest instructions.
                assert!(count > 0, "counter should have counted something");
            }
            Err(e) => eprintln!("PMU not available: {} (expected in CI)", e),
        }
    }

    #[test]
    fn test_overflow_mode() {
        InstructionCounter::install_sigio_handler();
        match InstructionCounter::with_overflow(10_000) {
            Ok(counter) => {
                counter.reset_and_enable();
                // In non-VM context, the counter will quickly overflow
                // (counting host instructions). Just verify it doesn't panic.
                counter.disable();
            }
            Err(e) => eprintln!("PMU not available: {} (expected in CI)", e),
        }
    }

    #[test]
    fn test_read_and_reset() {
        match InstructionCounter::new() {
            Ok(counter) => {
                counter.reset_and_enable();
                let val = counter.read_and_reset();
                counter.disable();
                // read_and_reset should return a value and reset the counter.
                // The value includes host instructions (no VM context).
                assert!(val > 0, "should have counted something before reset");
            }
            Err(e) => eprintln!("PMU not available: {} (expected in CI)", e),
        }
    }
}
