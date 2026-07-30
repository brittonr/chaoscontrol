//! Graceful shutdown via SIGINT/SIGTERM signal handling.
//!
//! Installs signal handlers that set an atomic flag on first signal and
//! force-exit on second signal. The explorer and campaign runner poll
//! [`shutdown_requested()`] after each round/seed to stop cleanly.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Global shutdown flag — set by signal handler, polled by explorer.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// How many signals have been received. Second signal force-exits.
static SIGNAL_COUNT: AtomicU32 = AtomicU32::new(0);

/// Returns `true` if a shutdown signal has been received.
pub fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::Relaxed)
}

/// Manually request shutdown (for testing).
pub fn request_shutdown() {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

/// Reset shutdown state (for testing only — not safe in production).
#[cfg(test)]
pub fn reset_shutdown() {
    SHUTDOWN.store(false, Ordering::Relaxed);
    SIGNAL_COUNT.store(0, Ordering::Relaxed);
}

/// Install signal handlers for SIGINT and SIGTERM.
///
/// First signal: sets the `SHUTDOWN` flag so the explorer can finish
/// the current round and save a checkpoint.
///
/// Second signal: calls `std::process::exit(1)` for an immediate exit
/// if the graceful path is stuck.
///
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn install_signal_handlers() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = signal_handler as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO;
        // Block SIGALRM during this handler to avoid re-entrancy with
        // the single-vCPU operational watchdog.
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaddset(&mut sa.sa_mask, libc::SIGALRM);

        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
    }
}

/// Signal handler — async-signal-safe (only atomics + process::exit).
extern "C" fn signal_handler(
    _sig: libc::c_int,
    _info: *mut libc::siginfo_t,
    _ctx: *mut libc::c_void,
) {
    let prev = SIGNAL_COUNT.fetch_add(1, Ordering::Relaxed);
    if prev == 0 {
        // First signal: set flag, let explorer finish gracefully.
        SHUTDOWN.store(true, Ordering::Relaxed);
    } else {
        // Second signal: force exit.
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_flag_initially_false() {
        // Note: other tests may have set it, so we reset first.
        reset_shutdown();
        assert!(!shutdown_requested());
    }

    #[test]
    fn double_install_is_safe() {
        install_signal_handlers();
        install_signal_handlers();
        // No crash = pass.
    }

    #[test]
    fn manual_request_sets_flag() {
        reset_shutdown();
        assert!(!shutdown_requested());
        request_shutdown();
        assert!(shutdown_requested());
        reset_shutdown();
    }
}
