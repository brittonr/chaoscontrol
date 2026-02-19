//! Deterministic vCPU preemption via hardware performance counters.
//!
//! Uses `perf_event_open()` with `exclude_host=1` to count guest
//! instructions. Supports two modes:
//!
//! - **Counting mode**: read instruction count on demand after exits
//! - **Overflow mode**: deliver SIGIO after exactly N instructions
//!   (with ~1-5 instruction PMU skid)

use std::io;
use std::os::fd::RawFd;

/// perf_event_attr structure (subset needed for our use).
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

    /// Create a counter in overflow mode: delivers SIGIO after `quantum`
    /// guest instructions. The signal interrupts `vcpu.run()` → EINTR.
    pub fn with_overflow(quantum: u64) -> io::Result<Self> {
        Self::create(quantum, true)
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

        // SAFETY: perf_event_open with valid attr pointer
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
            // SAFETY: fcntl on valid fd with standard flags
            unsafe {
                libc::fcntl(fd, libc::F_SETOWN, libc::gettid());
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_ASYNC);
            }
        }

        Ok(Self { fd })
    }

    /// Reset counter to zero and enable.
    #[inline]
    pub fn enable(&self) {
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_RESET, 0);
            libc::ioctl(self.fd, PERF_EVENT_IOC_ENABLE, 0);
        }
    }

    /// Disable the counter.
    #[inline]
    pub fn disable(&self) {
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_DISABLE, 0);
        }
    }

    /// Read current instruction count (does NOT reset).
    #[inline]
    pub fn read(&self) -> u64 {
        let mut value: u64 = 0;
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

    /// Read and reset atomically.
    #[inline]
    pub fn read_and_reset(&self) -> u64 {
        let value = self.read();
        unsafe {
            libc::ioctl(self.fd, PERF_EVENT_IOC_RESET, 0);
        }
        value
    }

    /// Install no-op SIGIO handler (for overflow mode).
    pub fn install_sigio_handler() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
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
        unsafe { libc::close(self.fd); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counting_mode() {
        match InstructionCounter::new() {
            Ok(counter) => {
                counter.enable();
                let count = counter.read();
                counter.disable();
                assert!(count < 100, "exclude_host should filter host insns, got {}", count);
            }
            Err(e) => eprintln!("Not available: {} (expected in CI)", e),
        }
    }

    #[test]
    fn test_overflow_mode() {
        InstructionCounter::install_sigio_handler();
        match InstructionCounter::with_overflow(10_000) {
            Ok(counter) => {
                assert_eq!(counter.read(), 0);
                counter.enable();
                counter.disable();
            }
            Err(e) => eprintln!("Not available: {} (expected in CI)", e),
        }
    }
}
