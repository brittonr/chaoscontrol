//! Internal runtime mode detection and output dispatch.
//!
//! Determines whether the SDK is running inside a ChaosControl VM
//! (vmcall transport) or outside (local JSON output or no-op).
//!
//! This module is only compiled with the `full` feature.

use chaoscontrol_protocol::{HypercallPage, HYPERCALL_PAGE_ADDR};

use std::io::{BufWriter, Write};
use std::os::unix::io::AsRawFd;

// ═══════════════════════════════════════════════════════════════════════
//  Transport mode
// ═══════════════════════════════════════════════════════════════════════

/// How the SDK communicates assertion/lifecycle/random data.
enum TransportMode {
    /// Running inside a ChaosControl VM — use vmcall via shared page.
    VmVmcall { page_ptr: *mut HypercallPage },
    /// Running inside a ChaosControl VM — use port I/O via shared page.
    VmPortIo { page_ptr: *mut HypercallPage },
    /// Running locally — log assertions to a JSON file.
    LocalOutput {
        writer: std::sync::Mutex<BufWriter<std::fs::File>>,
    },
    /// No output — silently discard everything.
    Noop,
}

// Safety: page_ptr is only accessed through our synchronized API.
unsafe impl Send for TransportMode {}
unsafe impl Sync for TransportMode {}

/// Name of the environment variable for local JSON output.
///
/// Set `CHAOSCONTROL_SDK_LOCAL_OUTPUT=/path/to/file.json` to enable
/// local assertion logging when running outside a ChaosControl VM.
///
/// This mirrors Antithesis's `ANTITHESIS_SDK_LOCAL_OUTPUT`.
pub const LOCAL_OUTPUT: &str = "CHAOSCONTROL_SDK_LOCAL_OUTPUT";
pub const PROCESS_TRANSPORT_LOCK: &str = "CHAOSCONTROL_SDK_TRANSPORT_LOCK";

enum ProcessTransportLock {
    Disabled,
    File(std::fs::File),
    Failed,
}

pub(crate) struct ProcessTransportGuard {
    file: Option<&'static std::fs::File>,
}

impl Drop for ProcessTransportGuard {
    fn drop(&mut self) {
        if let Some(file) = self.file {
            let _ = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

static MODE: std::sync::OnceLock<TransportMode> = std::sync::OnceLock::new();
static PROCESS_LOCK: std::sync::OnceLock<ProcessTransportLock> = std::sync::OnceLock::new();

pub(crate) fn acquire_process_transport() -> Result<ProcessTransportGuard, ()> {
    let lock = PROCESS_LOCK.get_or_init(|| {
        let Ok(path) = std::env::var(PROCESS_TRANSPORT_LOCK) else {
            return ProcessTransportLock::Disabled;
        };
        match std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
        {
            Ok(file) => ProcessTransportLock::File(file),
            Err(_) => ProcessTransportLock::Failed,
        }
    });
    match lock {
        ProcessTransportLock::Disabled => Ok(ProcessTransportGuard { file: None }),
        ProcessTransportLock::File(file) => {
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result == 0 {
                Ok(ProcessTransportGuard { file: Some(file) })
            } else {
                Err(())
            }
        }
        ProcessTransportLock::Failed => Err(()),
    }
}

/// Initialize the transport and return the detected mode.
fn detect_mode() -> TransportMode {
    // Try to mmap the hypercall page via /dev/mem.
    // If this succeeds, we're inside a ChaosControl VM.
    if let Some(ptr) = try_mmap_hypercall_page() {
        // Enable I/O port access for the port I/O fallback transport.
        // When running as PID 1 (init) in a VM, we have CAP_SYS_RAWIO.
        // This is needed because the 6.19+ kernels enforce IOPL checks
        // even though KVM intercepts the port I/O at the hypervisor level.
        #[cfg(target_os = "linux")]
        unsafe {
            libc::iopl(3);
        }
        // Read the transport mode byte written by the VMM at a fixed
        // offset within the hypercall page (_reserved2 area).
        let mode_byte = unsafe {
            let base = ptr as *const u8;
            *base.add(chaoscontrol_protocol::TRANSPORT_MODE_OFFSET as usize)
        };
        if mode_byte == chaoscontrol_protocol::TRANSPORT_VMCALL {
            return TransportMode::VmVmcall { page_ptr: ptr };
        } else {
            // Default to port I/O (safe fallback) — covers both
            // TRANSPORT_PORT_IO and unrecognized/zero values.
            return TransportMode::VmPortIo { page_ptr: ptr };
        }
    }

    // Not in a VM — check for local output env var.
    if let Ok(path) = std::env::var(LOCAL_OUTPUT) {
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            return TransportMode::LocalOutput {
                writer: std::sync::Mutex::new(BufWriter::new(file)),
            };
        }
        eprintln!(
            "chaoscontrol-sdk: warning: could not open {LOCAL_OUTPUT}={path}, falling back to no-op"
        );
    }

    TransportMode::Noop
}

/// Try to mmap the hypercall page at the expected guest physical address.
/// Returns `Some(ptr)` if successful (we're in a ChaosControl VM),
/// `None` if `/dev/mem` is unavailable or mmap fails.
fn try_mmap_hypercall_page() -> Option<*mut HypercallPage> {
    let fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/mem")
        .ok()?;

    let ptr = unsafe {
        libc::mmap(
            core::ptr::null_mut(),
            4096,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            HYPERCALL_PAGE_ADDR as libc::off_t,
        )
    };

    if ptr == libc::MAP_FAILED {
        return None;
    }

    Some(ptr as *mut HypercallPage)
}

fn get_mode() -> &'static TransportMode {
    MODE.get_or_init(detect_mode)
}

// ═══════════════════════════════════════════════════════════════════════
//  Catalog init
// ═══════════════════════════════════════════════════════════════════════

static INITIALIZED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Initialize the SDK.
///
/// Detects the transport mode (VM, local output, or no-op) and performs
/// any one-time setup.  Safe to call multiple times (idempotent).
///
/// This should be called as early as possible in your program.  If not
/// called explicitly, it will be called lazily on first SDK use.
pub fn init() {
    INITIALIZED.get_or_init(|| {
        // Force mode detection
        let _ = get_mode();
        // Emit the assertion catalog so the VMM/oracle knows about
        // every assertion site, including ones never reached at runtime.
        crate::assert::emit_catalog();
    });
}

/// Returns `true` if running inside a ChaosControl VM.
pub fn is_in_vm() -> bool {
    matches!(
        get_mode(),
        TransportMode::VmVmcall { .. } | TransportMode::VmPortIo { .. }
    )
}

/// Returns `true` if local output is configured.
pub fn is_local_output() -> bool {
    matches!(get_mode(), TransportMode::LocalOutput { .. })
}

// ═══════════════════════════════════════════════════════════════════════
//  VM transport
// ═══════════════════════════════════════════════════════════════════════

/// Get the hypercall page pointer, or `None` if not in a VM.
pub(crate) fn vm_page_ptr() -> Option<*mut HypercallPage> {
    match get_mode() {
        TransportMode::VmVmcall { page_ptr } | TransportMode::VmPortIo { page_ptr } => {
            Some(*page_ptr)
        }
        _ => None,
    }
}

/// Trigger the VMM after filling the hypercall page.
///
/// Uses `vmcall` if the VMM enabled KVM_CAP_EXIT_HYPERCALL,
/// otherwise falls back to port I/O (`outb(SDK_PORT, 0)`).
///
/// # Safety
///
/// Caller must ensure the hypercall page is fully written.
pub(crate) unsafe fn vm_trigger() {
    match get_mode() {
        TransportMode::VmVmcall { .. } => {
            core::arch::asm!(
                "vmcall",
                in("rax") chaoscontrol_protocol::VMCALL_NR,
                lateout("rax") _,
                options(nostack),
            );
        }
        TransportMode::VmPortIo { .. } => {
            core::arch::asm!(
                "out dx, al",
                in("dx") chaoscontrol_protocol::SDK_PORT,
                in("al") 0u8,
                options(nostack, nomem),
            );
        }
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Local JSON output (Antithesis fallback schema)
// ═══════════════════════════════════════════════════════════════════════

/// Parameters for a local assertion record.
pub(crate) struct LocalAssert<'a> {
    pub assert_type: &'a str,
    pub hit: bool,
    pub condition: bool,
    pub message: &'a str,
    pub id: u32,
    /// Pre-serialized JSON bytes for details (e.g. `b"{}"`).
    pub json_details: &'a [u8],
}

/// Write a local assertion record to the output file.
///
/// Format follows the [Antithesis Assertion Schema](https://antithesis.com/docs/using_antithesis/sdk/fallback/schema/).
pub(crate) fn local_emit_assert(params: &LocalAssert<'_>) {
    let mode = get_mode();
    let TransportMode::LocalOutput { writer } = mode else {
        return;
    };

    // json_details is already valid JSON — embed it directly
    let details_str = std::str::from_utf8(params.json_details).unwrap_or("{}");

    let must_hit = matches!(params.assert_type, "sometimes" | "reachability");
    let record = format!(
        concat!(
            "{{\"antithesis_assert\": {{",
            "\"assert_type\": \"{assert_type}\", ",
            "\"condition\": {condition}, ",
            "\"hit\": {hit}, ",
            "\"must_hit\": {must_hit}, ",
            "\"id\": \"{id:08x}\", ",
            "\"message\": \"{message}\", ",
            "\"display_type\": \"{display_type}\", ",
            "\"details\": {details}",
            "}}}}\n"
        ),
        assert_type = params.assert_type,
        condition = params.condition,
        hit = params.hit,
        must_hit = must_hit,
        id = params.id,
        message = escape_json(params.message),
        display_type = params.assert_type,
        details = details_str,
    );

    if let Ok(mut guard) = writer.lock() {
        let _ = guard.write_all(record.as_bytes());
        let _ = guard.flush();
    }
}

pub(crate) fn local_emit_value(value: &serde_json::Value) {
    let mode = get_mode();
    let TransportMode::LocalOutput { writer } = mode else {
        return;
    };
    let Ok(mut record) = serde_json::to_vec(value) else {
        return;
    };
    record.push(b'\n');
    if let Ok(mut guard) = writer.lock() {
        let _ = guard.write_all(&record);
        let _ = guard.flush();
    }
}

/// Write a local lifecycle event to the output file.
pub(crate) fn local_emit_lifecycle(event_name: &str, json_details: &[u8]) {
    let mode = get_mode();
    let TransportMode::LocalOutput { writer } = mode else {
        return;
    };

    let details_str = std::str::from_utf8(json_details).unwrap_or("{}");

    let record = if event_name == "setup_complete" {
        format!(
            "{{\"antithesis_setup\": {{\"status\": \"complete\", \"details\": {}}}}}\n",
            details_str
        )
    } else {
        format!("{{\"{}\": {}}}\n", escape_json(event_name), details_str)
    };

    if let Ok(mut guard) = writer.lock() {
        let _ = guard.write_all(record.as_bytes());
        let _ = guard.flush();
    }
}

/// Write a local random-choice observation to the output file.
pub(crate) fn local_emit_random_choice(n: u32, choice: u64) {
    let mode = get_mode();
    let TransportMode::LocalOutput { writer } = mode else {
        return;
    };

    let record = format!(
        "{{\"chaoscontrol_random_choice\": {{\"n\": {}, \"choice\": {}}}}}\n",
        n, choice
    );

    if let Ok(mut guard) = writer.lock() {
        let _ = guard.write_all(record.as_bytes());
        let _ = guard.flush();
    }
}

/// Minimal JSON string escaping (quotes and backslashes).
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

// ═══════════════════════════════════════════════════════════════════════
//  Random dispatch (outside VM)
// ═══════════════════════════════════════════════════════════════════════

/// Provide fallback random when outside a VM.
///
/// Uses the host entropy-backed `rand` runtime.  NOT deterministic — that's
/// the point: outside Antithesis/ChaosControl, random should be truly random.
pub(crate) fn local_random_u64() -> u64 {
    let entropy = rand::random::<u64>();
    let sequence = RANDOM_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    entropy ^ sequence
}

static RANDOM_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
