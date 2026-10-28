//! Graceful shutdown via SIGINT/SIGTERM signal handling.
//!
//! Installs signal handlers that set an atomic flag on first signal and
//! force-exit on second signal. The explorer and campaign runner poll
//! [`shutdown_requested()`] after each round/seed to stop cleanly.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;

/// Global shutdown flag — set by signal handler, polled by explorer.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// How many signals have been received. Second signal force-exits.
static SIGNAL_COUNT: AtomicU32 = AtomicU32::new(0);

/// Result of the one-time handler installation.
static HANDLERS_INSTALLED: OnceLock<Result<(), i32>> = OnceLock::new();

/// Returns `true` if a shutdown signal has been received.
pub fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::Relaxed)
}

/// Manually request shutdown (for testing).
pub fn request_shutdown() {
    SHUTDOWN.store(true, Ordering::Relaxed);
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
    let _ = try_install_signal_handlers();
}

/// Install signal handlers and report the first installation failure.
///
/// Repeated calls return the result of the original installation and never
/// reinstall, preserving the first-signal/second-signal state machine.
pub fn try_install_signal_handlers() -> io::Result<()> {
    match HANDLERS_INSTALLED.get_or_init(install_raw_handlers) {
        Ok(()) => Ok(()),
        Err(errno) => Err(io::Error::from_raw_os_error(*errno)),
    }
}

fn install_raw_handlers() -> Result<(), i32> {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = signal_handler as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO;
        // Block SIGALRM during this handler to avoid re-entrancy with
        // the single-vCPU operational watchdog.
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaddset(&mut sa.sa_mask, libc::SIGALRM);

        if libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut()) != 0 {
            return Err(last_errno());
        }
        if libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut()) != 0 {
            return Err(last_errno());
        }
    }
    Ok(())
}

fn last_errno() -> i32 {
    io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EINVAL)
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

    fn isolated(test: &str, expected_exit: i32, scenario: impl FnOnce()) {
        const CHILD_TEST: &str = "CHAOSCONTROL_SIGNAL_TEST_CHILD";
        if std::env::var(CHILD_TEST).as_deref() == Ok(test) {
            scenario();
            return;
        }

        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", &format!("signal::tests::{test}")])
            .env(CHILD_TEST, test)
            .output()
            .expect("isolated signal test process");
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "isolated signal test {test}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[test]
    fn shutdown_flag_initially_false() {
        isolated("shutdown_flag_initially_false", 0, || {
            assert!(!shutdown_requested());
        });
    }

    #[test]
    fn manual_request_sets_flag() {
        isolated("manual_request_sets_flag", 0, || {
            assert!(!shutdown_requested());
            request_shutdown();
            assert!(shutdown_requested());
        });
    }

    #[test]
    fn signals_request_shutdown_then_force_exit() {
        isolated("signals_request_shutdown_then_force_exit", 1, || {
            try_install_signal_handlers().expect("install signal handlers");
            install_signal_handlers();
            assert!(!shutdown_requested());
            assert_eq!(unsafe { libc::raise(libc::SIGTERM) }, 0);
            assert!(shutdown_requested());
            unsafe { libc::raise(libc::SIGTERM) };
            panic!("second signal did not force process exit");
        });
    }
}
