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
//!         let action = random_choice(3);
//!         cc_assert_always!(action < 3, "bounded", &serde_json::json!({}));
//!     }
//! }
//! ```

use std::ptr;

/// Initialize the VM guest environment.
///
/// Mounts the standard pseudo-filesystems, initializes kernel coverage
/// collection (best-effort), and sets up the SDK transport.  Safe to call
/// as PID 1 — mount errors other than EBUSY are logged to stderr but are
/// not fatal.
///
/// Performs these steps in order:
/// 1. Mount devtmpfs on `/dev`
/// 2. Mount proc on `/proc`
/// 3. Mount sysfs on `/sys`
/// 4. Mount debugfs on `/sys/kernel/debug`
/// 5. Initialize KCOV (no-op on non-KCOV kernels)
/// 6. Initialize SDK transport ([`chaoscontrol_init`](crate::chaoscontrol_init))
/// 7. Initialize coverage bitmap ([`coverage::init`](crate::coverage::init))
pub fn guest_init() {
    mount(c"devtmpfs", c"/dev", c"devtmpfs", 0o755);
    mount(c"proc", c"/proc", c"proc", 0o555);
    mount(c"sysfs", c"/sys", c"sysfs", 0o555);
    mount(c"debugfs", c"/sys/kernel/debug", c"debugfs", 0o555);

    crate::kcov::init();
    crate::chaoscontrol_init();
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
