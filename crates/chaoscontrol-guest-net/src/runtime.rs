#[allow(
    ambient_clock,
    reason = "guest network shell boundary feeds smoltcp time"
)]
pub(crate) fn now() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_millis(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
    )
}

pub(crate) fn mount_pseudo_filesystems() {
    unsafe {
        libc::mkdir(c"/proc".as_ptr().cast(), 0o555);
        libc::mount(
            c"proc".as_ptr().cast(),
            c"/proc".as_ptr().cast(),
            c"proc".as_ptr().cast(),
            0,
            std::ptr::null(),
        );
        libc::mkdir(c"/sys".as_ptr().cast(), 0o555);
        libc::mount(
            c"sysfs".as_ptr().cast(),
            c"/sys".as_ptr().cast(),
            c"sysfs".as_ptr().cast(),
            0,
            std::ptr::null(),
        );
    }
}
