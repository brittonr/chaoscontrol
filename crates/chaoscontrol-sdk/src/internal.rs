//! Internal runtime mode detection and output dispatch.
//!
//! Determines whether the SDK is running inside a ChaosControl VM
//! (vmcall transport) or outside (local JSON output or no-op).
//!
//! This module is only compiled with the `full` feature.

use chaoscontrol_protocol::{HypercallPage, HYPERCALL_PAGE_ADDR};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::io::AsRawFd;
use std::sync::{Mutex, OnceLock};

// ═══════════════════════════════════════════════════════════════════════
//  Transport mode
// ═══════════════════════════════════════════════════════════════════════

/// How the SDK communicates assertion/lifecycle/random data.
enum TransportMode {
    /// Running inside a ChaosControl VM — use vmcall via shared page.
    Vm { page_ptr: *mut HypercallPage },
    /// Running locally — log assertions to a JSON file.
    LocalOutput { writer: Mutex<BufWriter<File>> },
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

static MODE: OnceLock<TransportMode> = OnceLock::new();

/// Initialize the transport and return the detected mode.
fn detect_mode() -> TransportMode {
    // Try to mmap the hypercall page via /dev/mem.
    // If this succeeds, we're inside a ChaosControl VM.
    if let Some(ptr) = try_mmap_hypercall_page() {
        return TransportMode::Vm { page_ptr: ptr };
    }

    // Not in a VM — check for local output env var.
    if let Ok(path) = std::env::var(LOCAL_OUTPUT) {
        if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
            return TransportMode::LocalOutput {
                writer: Mutex::new(BufWriter::new(file)),
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
    let fd = OpenOptions::new()
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

static INITIALIZED: OnceLock<()> = OnceLock::new();

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
    });
}

/// Returns `true` if running inside a ChaosControl VM.
pub fn is_in_vm() -> bool {
    matches!(get_mode(), TransportMode::Vm { .. })
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
        TransportMode::Vm { page_ptr } => Some(*page_ptr),
        _ => None,
    }
}

/// Trigger the VMM via `vmcall` instruction.
///
/// # Safety
///
/// Caller must ensure the hypercall page is fully written.
pub(crate) unsafe fn vm_trigger() {
    core::arch::asm!(
        "vmcall",
        in("rax") chaoscontrol_protocol::VMCALL_NR,
        lateout("rax") _,
        options(nostack),
    );
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
    pub details: &'a [(&'a str, &'a str)],
}

/// Write a local assertion record to the output file.
///
/// Format follows the [Antithesis Assertion Schema](https://antithesis.com/docs/using_antithesis/sdk/fallback/schema/).
pub(crate) fn local_emit_assert(params: &LocalAssert<'_>) {
    let mode = get_mode();
    let TransportMode::LocalOutput { writer } = mode else {
        return;
    };

    let details_json = if params.details.is_empty() {
        "{}".to_owned()
    } else {
        let pairs: Vec<String> = params
            .details
            .iter()
            .map(|(k, v)| format!("\"{}\": \"{}\"", escape_json(k), escape_json(v)))
            .collect();
        format!("{{{}}}", pairs.join(", "))
    };

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
        details = details_json,
    );

    if let Ok(mut guard) = writer.lock() {
        let _ = guard.write_all(record.as_bytes());
        let _ = guard.flush();
    }
}

/// Write a local lifecycle event to the output file.
pub(crate) fn local_emit_lifecycle(event_name: &str, details: &[(&str, &str)]) {
    let mode = get_mode();
    let TransportMode::LocalOutput { writer } = mode else {
        return;
    };

    let details_json = if details.is_empty() {
        "{}".to_owned()
    } else {
        let pairs: Vec<String> = details
            .iter()
            .map(|(k, v)| format!("\"{}\": \"{}\"", escape_json(k), escape_json(v)))
            .collect();
        format!("{{{}}}", pairs.join(", "))
    };

    let record = if event_name == "setup_complete" {
        format!(
            "{{\"antithesis_setup\": {{\"status\": \"complete\", \"details\": {}}}}}\n",
            details_json
        )
    } else {
        format!("{{\"{}\": {}}}\n", escape_json(event_name), details_json)
    };

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
/// Uses `std::collections::hash_map::RandomState` for entropy,
/// seeded by thread RNG.  NOT deterministic — that's the point:
/// outside Antithesis/ChaosControl, random should be truly random.
pub(crate) fn local_random_u64() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    // Simple entropy: hash of timestamp + address of stack variable
    let mut hasher = DefaultHasher::new();
    SystemTime::now().hash(&mut hasher);
    let stack_var = 0u8;
    (core::ptr::addr_of!(stack_var) as u64).hash(&mut hasher);
    RANDOM_COUNTER
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .hash(&mut hasher);
    hasher.finish()
}

static RANDOM_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
