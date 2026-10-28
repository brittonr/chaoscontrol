//! VM guest environment setup.
//!
//! Handles the filesystem mounts and initialization that every ChaosControl
//! guest binary needs when running as PID 1 inside a VM.  Call [`guest_init`]
//! once at the top of `main()` — it replaces the mount boilerplate that
//! otherwise has to be copy-pasted into every guest.
//!
//! # Example
//!
//! ```rust,ignore
//! use chaoscontrol_sdk::prelude::*;
//!
//! fn main() {
//!     guest_init();
//!     setup_complete(&serde_json::json!({"program": "my-guest"}));
//!
//!     loop {
//!         let action = random_choice_index(3);
//!         cc_assert_always!(action < 3, "bounded", &serde_json::json!({}));
//!     }
//! }
//! ```

use std::ptr;

/// Initialize the VM guest environment.
///
/// As guest PID 1, mounts the standard pseudo-filesystems. All processes
/// initialize SDK transport and coverage; kernel coverage is best-effort in
/// the guest only. Ordinary host child processes never attempt mounts.
/// Mount errors other than EBUSY are logged but are not fatal.
///
/// Performs these steps in order:
/// 1. As PID 1, mount devtmpfs on `/dev`
/// 2. As PID 1, mount proc on `/proc`
/// 3. As PID 1, mount sysfs on `/sys`
/// 4. As PID 1, mount debugfs on `/sys/kernel/debug`
/// 5. Initialize SDK transport ([`chaoscontrol_init`](crate::chaoscontrol_init))
/// 6. In a VM or as PID 1, initialize KCOV (no-op on non-KCOV kernels)
/// 7. Initialize coverage bitmap ([`coverage::init`](crate::coverage::init))
pub fn guest_init() {
    let guest_init_process = std::process::id() == 1;
    if guest_init_process {
        mount(c"devtmpfs", c"/dev", c"devtmpfs", 0o755);
        mount(c"proc", c"/proc", c"proc", 0o555);
        mount(c"sysfs", c"/sys", c"sysfs", 0o555);
        mount(c"debugfs", c"/sys/kernel/debug", c"debugfs", 0o555);
    }

    crate::chaoscontrol_init();
    if guest_init_process || crate::is_in_vm() {
        crate::kcov::init();
    }
    crate::coverage::init();
}

/// Mount a pseudo-filesystem, creating the mount point if needed.
///
/// EBUSY (already mounted) is silently ignored.  Other errors are
/// logged to stderr but do not panic.
fn mount(fstype: &std::ffi::CStr, target: &std::ffi::CStr, source: &std::ffi::CStr, mode: u32) {
    unsafe {
        libc::mkdir(target.as_ptr().cast(), mode);
        let ret = libc::mount(
            source.as_ptr().cast(),
            target.as_ptr().cast(),
            fstype.as_ptr().cast(),
            0,
            ptr::null(),
        );
        if ret != 0 {
            let err = *libc::__errno_location();
            if err != libc::EBUSY {
                eprintln!(
                    "chaoscontrol: mount {} on {} failed (errno={})",
                    fstype.to_str().unwrap_or("?"),
                    target.to_str().unwrap_or("?"),
                    err,
                );
            }
        }
    }
}
