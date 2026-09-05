//! Pre-flight memory availability check.
//!
//! Reads `/proc/meminfo` to get available memory and compares against
//! the estimated VM memory requirement. Warns or errors if the estimate
//! exceeds 80% of available memory.

/// Read `MemAvailable` from `/proc/meminfo`, in megabytes.
///
/// Returns `None` on non-Linux or if the file can't be parsed.
pub fn read_available_memory_mb() -> Option<usize> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb_str = rest.trim().strip_suffix("kB")?.trim();
            let kb: usize = kb_str.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

/// Check whether the estimated VM memory fits within available RAM.
///
/// `estimated_mb` is the total VM memory (seeds × VMs × VM size).
/// If `strict` is true, returns `Err` when over the 80% threshold;
/// otherwise logs a warning and returns `Ok`.
pub fn check_memory(estimated_mb: usize, strict: bool) -> Result<(), String> {
    let available_mb = match read_available_memory_mb() {
        Some(mb) => mb,
        None => {
            log::debug!("Could not read /proc/meminfo, skipping memory check");
            return Ok(());
        }
    };

    ::log::info!(
        "Memory: {:.1} GB estimated ({} MB), {:.1} GB available ({} MB)",
        estimated_mb as f64 / 1024.0,
        estimated_mb,
        available_mb as f64 / 1024.0,
        available_mb,
    );

    let threshold = (available_mb as f64 * 0.8) as usize;
    if estimated_mb > threshold {
        let msg = format!(
            "Estimated VM memory ({:.1} GB) exceeds 80% of available memory ({:.1} GB). \
             Consider reducing seeds or VMs.",
            estimated_mb as f64 / 1024.0,
            available_mb as f64 / 1024.0,
        );
        if strict {
            return Err(msg);
        }
        eprintln!("Warning: {}", msg);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_available_memory_returns_some_on_linux() {
        // This test only passes on Linux with /proc/meminfo.
        if std::path::Path::new("/proc/meminfo").exists() {
            let mb = read_available_memory_mb();
            assert!(mb.is_some());
            assert!(mb.unwrap() > 0);
        }
    }

    #[test]
    fn check_memory_ok_when_under_threshold() {
        // 1 MB estimated vs whatever is available — should always pass.
        assert!(check_memory(1, false).is_ok());
        assert!(check_memory(1, true).is_ok());
    }

    #[test]
    fn check_memory_strict_rejects_overcommit() {
        // Use a huge estimate that exceeds any real machine.
        let result = check_memory(usize::MAX / 2, true);
        if std::path::Path::new("/proc/meminfo").exists() {
            assert!(result.is_err());
        }
        // On non-Linux, the check is skipped → Ok.
    }
}
