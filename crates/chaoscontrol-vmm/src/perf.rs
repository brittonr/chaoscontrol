//! Guest-only instruction progress with attributed PMU overflow.
//!
//! Overflow mode attaches one perf event to one Linux execution thread. The
//! dedicated real-time signal uses `SA_SIGINFO`, `F_SETOWN_EX`, and
//! `F_OWNER_TID`. The handler attributes each signal to one registered perf fd.
//! The VMM blocks and drains this signal around every arm and disarm boundary.
//! Stale, cross-fd, repeated, and wrapped generations fail closed.

use std::io;
use std::os::fd::RawFd;
use std::sync::atomic::AtomicI32;

/// Maximum live perf fds that can own attributed overflow slots.
pub const MAX_PMU_EXECUTION_THREADS: usize = 256;

const PERF_ATTR_FLAG_DISABLED_BIT: u32 = 0;
const PERF_ATTR_FLAG_EXCLUDE_HOST_BIT: u32 = 19;
#[cfg(test)]
const PERF_ATTR_FLAG_EXCLUDE_GUEST_BIT: u32 = 20;
const FLAG_DISABLED: u64 = 1 << PERF_ATTR_FLAG_DISABLED_BIT;
const FLAG_EXCLUDE_HOST: u64 = 1 << PERF_ATTR_FLAG_EXCLUDE_HOST_BIT;
#[cfg(test)]
const FLAG_EXCLUDE_GUEST: u64 = 1 << PERF_ATTR_FLAG_EXCLUDE_GUEST_BIT;
const PERF_TYPE_HARDWARE: u32 = 0;
const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
const PERF_EVENT_IOC_ENABLE: libc::c_ulong = 0x2400;
const PERF_EVENT_IOC_DISABLE: libc::c_ulong = 0x2401;
const PERF_EVENT_IOC_RESET: libc::c_ulong = 0x2403;
const PMU_SIGNAL_OFFSET: libc::c_int = 4;
/// Linux `F_SETSIG`; libc does not expose it on all target definitions.
const FCNTL_SET_SIGNAL: libc::c_int = 10;
/// Linux `F_SETOWN_EX` command.
const FCNTL_SET_OWNER_EX: libc::c_int = 15;
/// Linux `F_GETOWN_EX` command.
#[cfg(test)]
const FCNTL_GET_OWNER_EX: libc::c_int = 16;
/// Linux `F_OWNER_TID` owner type for thread-directed delivery.
const FCNTL_OWNER_THREAD: libc::c_int = 0;
/// x86_64 Linux offset of `_sigpoll.si_fd` inside `siginfo_t`.
const SIGINFO_POLL_FD_OFFSET: usize = 24;

static PMU_SLOT_IN_USE: [std::sync::atomic::AtomicBool; MAX_PMU_EXECUTION_THREADS] =
    [const { std::sync::atomic::AtomicBool::new(false) }; MAX_PMU_EXECUTION_THREADS];
static PMU_SLOT_FDS: [AtomicI32; MAX_PMU_EXECUTION_THREADS] =
    [const { AtomicI32::new(-1) }; MAX_PMU_EXECUTION_THREADS];
static PMU_SLOT_TIDS: [AtomicI32; MAX_PMU_EXECUTION_THREADS] =
    [const { AtomicI32::new(0) }; MAX_PMU_EXECUTION_THREADS];
static PMU_OVERFLOW_GENERATIONS: [std::sync::atomic::AtomicU64; MAX_PMU_EXECUTION_THREADS] =
    [const { std::sync::atomic::AtomicU64::new(0) }; MAX_PMU_EXECUTION_THREADS];
static PMU_GENERATION_WRAPPED: [std::sync::atomic::AtomicBool; MAX_PMU_EXECUTION_THREADS] =
    [const { std::sync::atomic::AtomicBool::new(false) }; MAX_PMU_EXECUTION_THREADS];
static PMU_SIGNAL_INSTALL: std::sync::OnceLock<Result<(), libc::c_int>> =
    std::sync::OnceLock::new();
static PMU_SIGNAL_NUMBER: AtomicI32 = AtomicI32::new(0);
static PMU_ATTRIBUTION_POISONED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// `perf_event_attr` layout for the fields used by this module.
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

impl PerfEventAttr {
    /// Build a Linux perf request that counts guest instructions only.
    fn guest_instructions(sample_period: u64) -> Self {
        Self {
            type_: PERF_TYPE_HARDWARE,
            size: std::mem::size_of::<Self>() as u32,
            config: PERF_COUNT_HW_INSTRUCTIONS,
            sample_period,
            sample_type: 0,
            read_format: 0,
            flags: FLAG_DISABLED | FLAG_EXCLUDE_HOST,
            wakeup_events: u32::from(sample_period > 0),
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
        }
    }
}

/// Linux `f_owner_ex` ABI used with `F_SETOWN_EX`.
#[repr(C)]
struct FcntlOwner {
    owner_type: libc::c_int,
    pid: libc::pid_t,
}

/// Hardware instruction counter bound to one Linux execution thread.
#[derive(Debug)]
pub struct InstructionCounter {
    fd: RawFd,
    owner_tid: libc::pid_t,
    overflow_slot: Option<usize>,
}

impl InstructionCounter {
    /// Create a guest-only counter without overflow signaling.
    pub fn new() -> io::Result<Self> {
        Self::create(0, false)
    }

    /// Create a guest-only counter with exact fd-attributed overflow signaling.
    pub fn with_overflow(period: u64) -> io::Result<Self> {
        if period == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PMU overflow period must be positive",
            ));
        }
        ensure_attribution_healthy()?;
        install_pmu_signal_handler()?;
        Self::create(period, true)
    }

    fn create(sample_period: u64, async_signal: bool) -> io::Result<Self> {
        let owner_tid = current_tid();
        let mut attr = PerfEventAttr::guest_instructions(sample_period);
        // SAFETY: `attr` points to the Linux perf ABI structure above.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                &mut attr as *mut PerfEventAttr,
                0i32,
                -1i32,
                -1i32,
                0u64,
            )
        } as RawFd;
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let overflow_slot = if async_signal {
            match configure_overflow_fd(fd, owner_tid) {
                Ok(slot) => Some(slot),
                Err(error) => {
                    // SAFETY: `fd` was returned by `perf_event_open` above.
                    unsafe { libc::close(fd) };
                    return Err(error);
                }
            }
        } else {
            None
        };
        Ok(Self {
            fd,
            owner_tid,
            overflow_slot,
        })
    }

    /// Reset to zero and enable counting for a new vCPU turn.
    pub fn reset_and_enable(&self) -> io::Result<()> {
        self.ensure_owner_thread()?;
        checked_ioctl(self.fd, PERF_EVENT_IOC_RESET)?;
        checked_ioctl(self.fd, PERF_EVENT_IOC_ENABLE)
    }

    /// Resume counting without resetting the turn-relative count.
    pub fn resume(&self) -> io::Result<()> {
        self.ensure_owner_thread()?;
        checked_ioctl(self.fd, PERF_EVENT_IOC_ENABLE)
    }

    /// Pause counting while preserving the count.
    pub fn disable(&self) -> io::Result<()> {
        self.ensure_owner_thread()?;
        checked_ioctl(self.fd, PERF_EVENT_IOC_DISABLE)
    }

    /// Arm one overflow attempt after stale-signal drainage.
    pub fn arm_overflow(&self) -> io::Result<u64> {
        self.ensure_owner_thread()?;
        let slot = self.require_overflow_slot()?;
        with_pmu_signal_blocked(|| {
            drain_pending_pmu_signals()?;
            let baseline = checked_slot_generation(slot)?;
            checked_ioctl(self.fd, PERF_EVENT_IOC_ENABLE)?;
            Ok(baseline)
        })
    }

    /// Disable one attempt and accept at most one fd-attributed overflow.
    pub fn disarm_overflow(&self, baseline: u64) -> io::Result<bool> {
        self.ensure_owner_thread()?;
        let slot = self.require_overflow_slot()?;
        with_pmu_signal_blocked(|| {
            checked_ioctl(self.fd, PERF_EVENT_IOC_DISABLE)
                .map_err(|error| io::Error::other(format!("PMU disable failed: {error}")))?;
            drain_pending_pmu_signals().map_err(|error| {
                io::Error::other(format!("PMU generation drain failed: {error}"))
            })?;
            let current = checked_slot_generation(slot).map_err(|error| {
                io::Error::other(format!("PMU generation validation failed: {error}"))
            })?;
            let delta = current
                .checked_sub(baseline)
                .ok_or_else(|| io::Error::other("PMU overflow generation regressed or wrapped"))?;
            match delta {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(io::Error::other(format!(
                    "PMU attempt received {delta} overflow signals"
                ))),
            }
        })
    }

    /// Read the complete guest-only count. Short reads and failures fail.
    pub fn read(&self) -> io::Result<u64> {
        self.ensure_owner_thread()?;
        let mut value = 0u64;
        loop {
            // SAFETY: `value` is a valid writable `u64` buffer.
            let read_len = unsafe {
                libc::read(
                    self.fd,
                    &mut value as *mut u64 as *mut libc::c_void,
                    std::mem::size_of::<u64>(),
                )
            };
            if read_len == std::mem::size_of::<u64>() as isize {
                return Ok(value);
            }
            if read_len < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "short PMU read: expected {} bytes, got {read_len}",
                    std::mem::size_of::<u64>()
                ),
            ));
        }
    }

    /// Return this counter's checked fd-specific overflow generation.
    pub fn overflow_generation(&self) -> io::Result<u64> {
        self.ensure_owner_thread()?;
        checked_slot_generation(self.require_overflow_slot()?)
    }

    /// True only when exactly one attributed signal advanced after `baseline`.
    pub fn overflow_since(&self, baseline: u64) -> io::Result<bool> {
        let current = self.overflow_generation()?;
        let delta = current
            .checked_sub(baseline)
            .ok_or_else(|| io::Error::other("PMU overflow generation regressed or wrapped"))?;
        match delta {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(io::Error::other(format!(
                "PMU attempt received {delta} overflow signals"
            ))),
        }
    }

    /// Return the Linux thread that owns this counter.
    pub fn owner_tid(&self) -> libc::pid_t {
        self.owner_tid
    }

    fn require_overflow_slot(&self) -> io::Result<usize> {
        self.overflow_slot.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "counting-mode PMU has no overflow slot",
            )
        })
    }

    fn ensure_owner_thread(&self) -> io::Result<()> {
        ensure_attribution_healthy()?;
        let current = current_tid();
        if current != self.owner_tid {
            return Err(io::Error::other(format!(
                "PMU counter belongs to thread {}, current thread is {current}",
                self.owner_tid
            )));
        }
        Ok(())
    }
}

impl Drop for InstructionCounter {
    fn drop(&mut self) {
        if self.fd < 0 {
            return;
        }
        if self.overflow_slot.is_none() {
            // SAFETY: `fd` is uniquely owned by this value.
            unsafe { libc::close(self.fd) };
            self.fd = -1;
            return;
        }
        if current_tid() != self.owner_tid {
            PMU_ATTRIBUTION_POISONED.store(true, std::sync::atomic::Ordering::Release);
            // SAFETY: closing prevents more events. Global poison forbids reuse.
            unsafe { libc::close(self.fd) };
            if let Some(slot) = self.overflow_slot.take() {
                release_overflow_slot(slot);
            }
            self.fd = -1;
            return;
        }
        let cleanup = with_pmu_signal_blocked(|| {
            let _ = checked_ioctl(self.fd, PERF_EVENT_IOC_DISABLE);
            // SAFETY: close stops new overflow generation for this fd.
            unsafe { libc::close(self.fd) };
            drain_pending_pmu_signals()?;
            if let Some(slot) = self.overflow_slot.take() {
                release_overflow_slot(slot);
            }
            Ok(())
        });
        self.fd = -1;
        if cleanup.is_err() {
            PMU_ATTRIBUTION_POISONED.store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

fn ensure_attribution_healthy() -> io::Result<()> {
    if PMU_ATTRIBUTION_POISONED.load(std::sync::atomic::Ordering::Acquire) {
        Err(io::Error::other(
            "PMU signal attribution was poisoned by an unsafe teardown",
        ))
    } else {
        Ok(())
    }
}

fn checked_ioctl(fd: RawFd, request: libc::c_ulong) -> io::Result<()> {
    // SAFETY: ioctl receives an owned perf fd and a no-argument perf request.
    let result = unsafe { libc::ioctl(fd, request, 0) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn checked_fcntl(
    fd: RawFd,
    command: libc::c_int,
    argument: libc::c_int,
) -> io::Result<libc::c_int> {
    // SAFETY: `command` and `argument` use the Linux fcntl ABI.
    let result = unsafe { libc::fcntl(fd, command, argument) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

fn configure_overflow_fd(fd: RawFd, owner_tid: libc::pid_t) -> io::Result<usize> {
    with_pmu_signal_blocked(|| {
        drain_pending_pmu_signals()?;
        let slot = register_overflow_slot(fd, owner_tid)?;
        if let Err(error) = configure_async_signal(fd, owner_tid) {
            release_overflow_slot(slot);
            return Err(error);
        }
        Ok(slot)
    })
}

fn configure_async_signal(fd: RawFd, owner_tid: libc::pid_t) -> io::Result<()> {
    let owner = FcntlOwner {
        owner_type: FCNTL_OWNER_THREAD,
        pid: owner_tid,
    };
    // SAFETY: `owner` matches Linux `struct f_owner_ex` for this call.
    let owner_result = unsafe { libc::fcntl(fd, FCNTL_SET_OWNER_EX, &owner) };
    if owner_result < 0 {
        return Err(io::Error::last_os_error());
    }
    checked_fcntl(fd, FCNTL_SET_SIGNAL, pmu_overflow_signal())?;
    let flags = checked_fcntl(fd, libc::F_GETFL, 0)?;
    checked_fcntl(fd, libc::F_SETFL, flags | libc::O_ASYNC | libc::O_NONBLOCK)?;
    Ok(())
}

fn install_pmu_signal_handler() -> io::Result<()> {
    let installed = PMU_SIGNAL_INSTALL.get_or_init(|| {
        let signal = pmu_overflow_signal();
        if signal > libc::SIGRTMAX() {
            return Err(libc::EINVAL);
        }
        PMU_SIGNAL_NUMBER.store(signal, std::sync::atomic::Ordering::Release);
        // SAFETY: installs one process-wide handler for a dedicated RT signal.
        let result = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = pmu_overflow_signal_handler as *const () as usize;
            action.sa_flags = libc::SA_SIGINFO;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaddset(&mut action.sa_mask, signal);
            libc::sigaction(signal, &action, std::ptr::null_mut())
        };
        if result < 0 {
            PMU_SIGNAL_NUMBER.store(0, std::sync::atomic::Ordering::Release);
            Err(io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EINVAL))
        } else {
            Ok(())
        }
    });
    match installed {
        Ok(()) => Ok(()),
        Err(errno) => Err(io::Error::from_raw_os_error(*errno)),
    }
}

fn pmu_overflow_signal() -> libc::c_int {
    libc::SIGRTMIN() + PMU_SIGNAL_OFFSET
}

extern "C" fn pmu_overflow_signal_handler(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
) {
    record_pmu_signal(signal, info);
}

fn record_pmu_signal(signal: libc::c_int, info: *const libc::siginfo_t) {
    if signal != PMU_SIGNAL_NUMBER.load(std::sync::atomic::Ordering::Acquire) || info.is_null() {
        return;
    }
    // SAFETY: the kernel supplied `info` for SA_SIGINFO. This workspace targets
    // x86_64 Linux, whose `_sigpoll.si_fd` offset is fixed by the signal ABI.
    let fd = unsafe {
        std::ptr::read_unaligned(
            (info as *const u8)
                .add(SIGINFO_POLL_FD_OFFSET)
                .cast::<libc::c_int>(),
        )
    };
    let tid = current_tid();
    for slot in 0..MAX_PMU_EXECUTION_THREADS {
        if PMU_SLOT_IN_USE[slot].load(std::sync::atomic::Ordering::Acquire)
            && PMU_SLOT_FDS[slot].load(std::sync::atomic::Ordering::Relaxed) == fd
            && PMU_SLOT_TIDS[slot].load(std::sync::atomic::Ordering::Relaxed) == tid
        {
            let current = PMU_OVERFLOW_GENERATIONS[slot].load(std::sync::atomic::Ordering::Relaxed);
            if current == u64::MAX {
                PMU_GENERATION_WRAPPED[slot].store(true, std::sync::atomic::Ordering::Release);
            } else {
                PMU_OVERFLOW_GENERATIONS[slot]
                    .store(current + 1, std::sync::atomic::Ordering::Release);
            }
            return;
        }
    }
}

fn current_tid() -> libc::pid_t {
    // SAFETY: `gettid` has no pointer arguments and is signal-safe on Linux.
    unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t }
}

fn register_overflow_slot(fd: RawFd, tid: libc::pid_t) -> io::Result<usize> {
    for slot in 0..MAX_PMU_EXECUTION_THREADS {
        if PMU_SLOT_IN_USE[slot]
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
        {
            PMU_SLOT_FDS[slot].store(fd, std::sync::atomic::Ordering::Relaxed);
            PMU_SLOT_TIDS[slot].store(tid, std::sync::atomic::Ordering::Relaxed);
            PMU_OVERFLOW_GENERATIONS[slot].store(0, std::sync::atomic::Ordering::Relaxed);
            PMU_GENERATION_WRAPPED[slot].store(false, std::sync::atomic::Ordering::Relaxed);
            // Publish initialized fields after ownership was acquired.
            PMU_SLOT_IN_USE[slot].store(true, std::sync::atomic::Ordering::Release);
            return Ok(slot);
        }
    }
    Err(io::Error::other(format!(
        "PMU overflow slot limit {MAX_PMU_EXECUTION_THREADS} reached"
    )))
}

fn release_overflow_slot(slot: usize) {
    PMU_SLOT_FDS[slot].store(-1, std::sync::atomic::Ordering::Relaxed);
    PMU_SLOT_TIDS[slot].store(0, std::sync::atomic::Ordering::Relaxed);
    PMU_OVERFLOW_GENERATIONS[slot].store(0, std::sync::atomic::Ordering::Relaxed);
    PMU_GENERATION_WRAPPED[slot].store(false, std::sync::atomic::Ordering::Relaxed);
    // Publish availability only after all old-owner fields are cleared.
    PMU_SLOT_IN_USE[slot].store(false, std::sync::atomic::Ordering::Release);
}

fn checked_slot_generation(slot: usize) -> io::Result<u64> {
    ensure_attribution_healthy()?;
    if PMU_GENERATION_WRAPPED[slot].load(std::sync::atomic::Ordering::Acquire) {
        return Err(io::Error::other("PMU overflow generation wrapped"));
    }
    Ok(PMU_OVERFLOW_GENERATIONS[slot].load(std::sync::atomic::Ordering::Acquire))
}

fn with_pmu_signal_blocked<T>(operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    let signal_set = pmu_signal_set()?;
    // SAFETY: both sets are valid and apply only to the calling thread.
    let mut previous: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: `signal_set` and `previous` are initialized signal sets.
    let block_result =
        unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &signal_set, &mut previous) };
    if block_result != 0 {
        return Err(io::Error::from_raw_os_error(block_result));
    }
    let result = operation();
    // SAFETY: restore the exact mask captured above for this thread.
    let restore_result =
        unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut()) };
    if restore_result != 0 {
        PMU_ATTRIBUTION_POISONED.store(true, std::sync::atomic::Ordering::Release);
        return Err(io::Error::from_raw_os_error(restore_result));
    }
    result
}

fn pmu_signal_set() -> io::Result<libc::sigset_t> {
    // SAFETY: the set is initialized before return.
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: both libc calls receive a valid signal set.
    let empty_result = unsafe { libc::sigemptyset(&mut set) };
    if empty_result != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the signal number was checked during handler installation.
    let add_result = unsafe { libc::sigaddset(&mut set, pmu_overflow_signal()) };
    if add_result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(set)
}

fn drain_pending_pmu_signals() -> io::Result<usize> {
    let signal_set = pmu_signal_set()?;
    let timeout = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let mut drained = 0usize;
    loop {
        // SAFETY: `signal_set`, `info`, and zero timeout are valid.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: the PMU signal is blocked by the caller.
        let result = unsafe { libc::sigtimedwait(&signal_set, &mut info, &timeout) };
        if result == pmu_overflow_signal() {
            record_pmu_signal(result, &info);
            drained = drained
                .checked_add(1)
                .ok_or_else(|| io::Error::other("PMU pending-signal count overflow"))?;
            continue;
        }
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EAGAIN) {
                return Ok(drained);
            }
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        return Err(io::Error::other(format!(
            "unexpected signal {result} while draining PMU overflow"
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_OVERFLOW_PERIOD: u64 = 10_000;
    const FIRST_FAKE_FD: RawFd = 100;
    const SECOND_FAKE_FD: RawFd = 101;
    const FIRST_FAKE_TID: libc::pid_t = 200;
    const SECOND_FAKE_TID: libc::pid_t = 201;

    fn slot_has_registration(slot: usize, fd: RawFd, tid: libc::pid_t) -> bool {
        PMU_SLOT_IN_USE[slot].load(std::sync::atomic::Ordering::Acquire)
            && PMU_SLOT_FDS[slot].load(std::sync::atomic::Ordering::Relaxed) == fd
            && PMU_SLOT_TIDS[slot].load(std::sync::atomic::Ordering::Relaxed) == tid
            && PMU_OVERFLOW_GENERATIONS[slot].load(std::sync::atomic::Ordering::Relaxed) == 0
            && !PMU_GENERATION_WRAPPED[slot].load(std::sync::atomic::Ordering::Relaxed)
    }

    fn synthetic_siginfo(fd: RawFd) -> libc::siginfo_t {
        // SAFETY: the value is initialized before the test writes `si_fd`.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: the x86_64 Linux `_sigpoll.si_fd` offset is asserted by use.
        unsafe {
            std::ptr::write_unaligned(
                (&mut info as *mut libc::siginfo_t as *mut u8)
                    .add(SIGINFO_POLL_FD_OFFSET)
                    .cast::<libc::c_int>(),
                fd,
            );
        }
        info
    }

    #[test]
    fn guest_instruction_attr_excludes_host_but_not_guest() {
        let attr = PerfEventAttr::guest_instructions(TEST_OVERFLOW_PERIOD);
        assert_ne!(attr.flags & FLAG_EXCLUDE_HOST, 0);
        assert_eq!(attr.flags & FLAG_EXCLUDE_GUEST, 0);
        assert_ne!(attr.flags & FLAG_DISABLED, 0);
        assert_eq!(attr.wakeup_events, 1);
    }

    #[test]
    fn zero_overflow_period_is_rejected() {
        let error = InstructionCounter::with_overflow(0).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn invalid_fd_operations_return_typed_errors() {
        let counter = InstructionCounter {
            fd: -1,
            owner_tid: current_tid(),
            overflow_slot: None,
        };
        assert!(counter.reset_and_enable().is_err());
        assert!(counter.resume().is_err());
        assert!(counter.disable().is_err());
        assert!(counter.read().is_err());
        assert!(counter.overflow_generation().is_err());
        assert!(configure_async_signal(-1, current_tid()).is_err());
    }

    #[test]
    fn async_signal_owner_is_the_current_linux_thread() {
        let mut pipe_fds = [-1; 2];
        // SAFETY: `pipe_fds` is a valid two-element output buffer.
        assert_eq!(
            unsafe { libc::pipe2(pipe_fds.as_mut_ptr(), libc::O_CLOEXEC) },
            0
        );
        install_pmu_signal_handler().unwrap();
        configure_async_signal(pipe_fds[0], current_tid()).unwrap();

        let mut owner = FcntlOwner {
            owner_type: -1,
            pid: -1,
        };
        // SAFETY: `owner` matches Linux `struct f_owner_ex` and is writable.
        assert_eq!(
            unsafe { libc::fcntl(pipe_fds[0], FCNTL_GET_OWNER_EX, &mut owner) },
            0
        );
        assert_eq!(owner.owner_type, FCNTL_OWNER_THREAD);
        assert_eq!(owner.pid, current_tid());
        // SAFETY: both descriptors are uniquely owned by this test.
        unsafe {
            libc::close(pipe_fds[0]);
            libc::close(pipe_fds[1]);
        }
    }

    #[test]
    fn siginfo_fd_attributes_only_the_matching_counter_slot() {
        install_pmu_signal_handler().unwrap();
        let first = register_overflow_slot(FIRST_FAKE_FD, current_tid()).unwrap();
        let second = register_overflow_slot(SECOND_FAKE_FD, current_tid()).unwrap();
        let first_before = checked_slot_generation(first).unwrap();
        let second_before = checked_slot_generation(second).unwrap();
        let first_info = synthetic_siginfo(FIRST_FAKE_FD);

        record_pmu_signal(pmu_overflow_signal(), &first_info);

        assert_eq!(checked_slot_generation(first).unwrap(), first_before + 1);
        assert_eq!(checked_slot_generation(second).unwrap(), second_before);
        release_overflow_slot(first);
        release_overflow_slot(second);
    }

    #[test]
    fn stale_or_unrelated_signal_cannot_advance_a_live_slot() {
        install_pmu_signal_handler().unwrap();
        let slot = register_overflow_slot(FIRST_FAKE_FD, current_tid()).unwrap();
        let before = checked_slot_generation(slot).unwrap();
        let stale_info = synthetic_siginfo(SECOND_FAKE_FD);

        record_pmu_signal(pmu_overflow_signal(), &stale_info);
        record_pmu_signal(libc::SIGALRM, &synthetic_siginfo(FIRST_FAKE_FD));

        assert_eq!(checked_slot_generation(slot).unwrap(), before);
        release_overflow_slot(slot);
    }

    #[test]
    fn released_slot_reuse_preserves_new_owner_fields() {
        let slot = register_overflow_slot(FIRST_FAKE_FD, FIRST_FAKE_TID).unwrap();
        PMU_OVERFLOW_GENERATIONS[slot].store(1, std::sync::atomic::Ordering::Relaxed);
        PMU_GENERATION_WRAPPED[slot].store(true, std::sync::atomic::Ordering::Relaxed);

        release_overflow_slot(slot);
        let reused_slot = register_overflow_slot(SECOND_FAKE_FD, SECOND_FAKE_TID).unwrap();

        assert_eq!(reused_slot, slot);
        assert!(slot_has_registration(
            reused_slot,
            SECOND_FAKE_FD,
            SECOND_FAKE_TID
        ));
        release_overflow_slot(reused_slot);
    }

    #[test]
    fn legacy_release_interleaving_is_detected_as_owner_overwrite() {
        let slot = register_overflow_slot(FIRST_FAKE_FD, FIRST_FAKE_TID).unwrap();

        // Model the rejected order: publish availability, acquire for a new
        // owner, and then let the old owner clear fields late.
        PMU_SLOT_IN_USE[slot].store(false, std::sync::atomic::Ordering::Release);
        let reused_slot = register_overflow_slot(SECOND_FAKE_FD, SECOND_FAKE_TID).unwrap();
        assert_eq!(reused_slot, slot);
        PMU_SLOT_FDS[slot].store(-1, std::sync::atomic::Ordering::Relaxed);
        PMU_SLOT_TIDS[slot].store(0, std::sync::atomic::Ordering::Relaxed);
        PMU_OVERFLOW_GENERATIONS[slot].store(0, std::sync::atomic::Ordering::Relaxed);
        PMU_GENERATION_WRAPPED[slot].store(false, std::sync::atomic::Ordering::Relaxed);

        assert!(PMU_SLOT_IN_USE[slot].load(std::sync::atomic::Ordering::Acquire));
        assert!(!slot_has_registration(
            reused_slot,
            SECOND_FAKE_FD,
            SECOND_FAKE_TID
        ));
        release_overflow_slot(reused_slot);
    }

    #[test]
    fn generation_wrap_is_detected_without_wrapping() {
        install_pmu_signal_handler().unwrap();
        let slot = register_overflow_slot(FIRST_FAKE_FD, current_tid()).unwrap();
        PMU_OVERFLOW_GENERATIONS[slot].store(u64::MAX, std::sync::atomic::Ordering::Release);
        let info = synthetic_siginfo(FIRST_FAKE_FD);

        record_pmu_signal(pmu_overflow_signal(), &info);

        assert_eq!(
            PMU_OVERFLOW_GENERATIONS[slot].load(std::sync::atomic::Ordering::Acquire),
            u64::MAX
        );
        assert!(checked_slot_generation(slot).is_err());
        release_overflow_slot(slot);
    }

    #[test]
    fn multiple_overflows_fail_closed() {
        let slot = register_overflow_slot(FIRST_FAKE_FD, current_tid()).unwrap();
        let mut counter = InstructionCounter {
            fd: -1,
            owner_tid: current_tid(),
            overflow_slot: Some(slot),
        };
        PMU_OVERFLOW_GENERATIONS[slot].store(2, std::sync::atomic::Ordering::Release);
        assert!(counter.overflow_since(0).is_err());
        counter.overflow_slot.take();
        release_overflow_slot(slot);
    }

    #[test]
    fn counting_mode_reports_success_or_capability_error() {
        match InstructionCounter::new() {
            Ok(counter) => {
                counter.reset_and_enable().unwrap();
                let count = counter.read().unwrap();
                counter.disable().unwrap();
                std::hint::black_box(count);
            }
            Err(error) => eprintln!("PMU not available: {error} (expected in CI)"),
        }
    }

    #[test]
    fn overflow_mode_reports_success_or_capability_error() {
        match InstructionCounter::with_overflow(TEST_OVERFLOW_PERIOD) {
            Ok(counter) => {
                let baseline = counter.arm_overflow().unwrap();
                let attributed = counter.disarm_overflow(baseline).unwrap();
                assert!(!attributed);
            }
            Err(error) => eprintln!("PMU overflow unavailable: {error} (expected in CI)"),
        }
    }
}
