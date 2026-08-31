use chaoscontrol_sim_core::{
    GuestDeterminismProbe, BOOT_ENTROPY_SEED_BYTES, GUEST_DETERMINISM_PROBE_SCHEMA,
};
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

const CLOCK_SPIN_ITERATIONS: usize = 4_096;
const EXPECTED_SIGNAL_COUNT: usize = 2;
const EMPTY_SIGNAL: i32 = 0;
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
const HEX_DIGIT_COUNT: usize = 16;
const HEX_CHARS_PER_BYTE: usize = 2;
const NIBBLE_BITS: u8 = 4;
const LOW_NIBBLE_MASK: u8 = 0x0f;
const PROBE_PREFIX: &str = "GUEST_DETERMINISM_PROBE=";
const READY_MARKER: &str = "GUEST_DETERMINISM_READY";
const ERROR_PREFIX: &str = "GUEST_DETERMINISM_ERROR=";

static SIGNAL_INDEX: AtomicUsize = AtomicUsize::new(0);
static SIGNALS: [AtomicI32; EXPECTED_SIGNAL_COUNT] =
    [AtomicI32::new(EMPTY_SIGNAL), AtomicI32::new(EMPTY_SIGNAL)];

extern "C" fn record_signal(signal: libc::c_int) {
    let index = SIGNAL_INDEX.fetch_add(1, Ordering::SeqCst);
    if index < EXPECTED_SIGNAL_COUNT {
        SIGNALS[index].store(signal, Ordering::SeqCst);
    }
}

#[inline(never)]
fn text_marker() -> usize {
    CLOCK_SPIN_ITERATIONS
}

fn read_entropy() -> Result<[u8; BOOT_ENTROPY_SEED_BYTES], String> {
    let mut bytes = [0_u8; BOOT_ENTROPY_SEED_BYTES];
    let mut offset = 0_usize;
    while offset < bytes.len() {
        // SAFETY: The pointer targets the remaining writable part of `bytes`.
        let result = unsafe {
            libc::getrandom(bytes[offset..].as_mut_ptr().cast(), bytes.len() - offset, 0)
        };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("getrandom failed: {error}"));
        }
        if result == 0 {
            return Err("getrandom returned no progress".to_string());
        }
        let read = usize::try_from(result).map_err(|_| "getrandom count overflow".to_string())?;
        offset = offset
            .checked_add(read)
            .ok_or_else(|| "getrandom offset overflow".to_string())?;
    }
    Ok(bytes)
}

fn monotonic_ns() -> Result<u64, String> {
    let mut timestamp = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `timestamp` is a valid writable timespec.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut timestamp) };
    if result != 0 {
        return Err(format!(
            "clock_gettime failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let seconds =
        u64::try_from(timestamp.tv_sec).map_err(|_| "negative monotonic seconds".to_string())?;
    let nanoseconds = u64::try_from(timestamp.tv_nsec)
        .map_err(|_| "negative monotonic nanoseconds".to_string())?;
    seconds
        .checked_mul(NANOSECONDS_PER_SECOND)
        .and_then(|value| value.checked_add(nanoseconds))
        .ok_or_else(|| "monotonic time overflow".to_string())
}

fn monotonic_delta() -> Result<u64, String> {
    let start = monotonic_ns()?;
    let mut accumulator = 0_usize;
    for value in 0..CLOCK_SPIN_ITERATIONS {
        accumulator = accumulator.wrapping_add(std::hint::black_box(value));
    }
    std::hint::black_box(accumulator);
    monotonic_ns()?
        .checked_sub(start)
        .ok_or_else(|| "monotonic time moved backward".to_string())
}

fn observed_signal_order() -> Result<Vec<u32>, String> {
    SIGNAL_INDEX.store(0, Ordering::SeqCst);
    for slot in &SIGNALS {
        slot.store(EMPTY_SIGNAL, Ordering::SeqCst);
    }

    // SAFETY: The zeroed value is immediately initialized as a sigaction.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = record_signal as *const () as usize;
    action.sa_flags = 0;
    // SAFETY: `action.sa_mask` is a valid signal set.
    if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0 {
        return Err("sigemptyset action failed".to_string());
    }
    // SAFETY: The action contains a valid one-argument handler.
    if unsafe { libc::sigaction(libc::SIGUSR1, &action, std::ptr::null_mut()) } != 0
        || unsafe { libc::sigaction(libc::SIGUSR2, &action, std::ptr::null_mut()) } != 0
    {
        return Err(format!(
            "sigaction failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // SAFETY: Both signal-set values are valid writable objects.
    let mut blocked: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: `previous` receives the process's current signal mask.
    let mut previous: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: `blocked` is a valid signal set.
    if unsafe { libc::sigemptyset(&mut blocked) } != 0
        || unsafe { libc::sigaddset(&mut blocked, libc::SIGUSR1) } != 0
        || unsafe { libc::sigaddset(&mut blocked, libc::SIGUSR2) } != 0
        || unsafe { libc::sigprocmask(libc::SIG_BLOCK, &blocked, &mut previous) } != 0
    {
        return Err("signal mask setup failed".to_string());
    }

    // SAFETY: Both signal numbers have installed handlers and are blocked.
    if unsafe { libc::raise(libc::SIGUSR2) } != 0 || unsafe { libc::raise(libc::SIGUSR1) } != 0 {
        return Err("raise failed".to_string());
    }
    // SAFETY: `previous` came from the successful signal-mask operation.
    if unsafe { libc::sigprocmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut()) } != 0 {
        return Err("signal mask restore failed".to_string());
    }

    let observed = SIGNAL_INDEX.load(Ordering::SeqCst);
    if observed != EXPECTED_SIGNAL_COUNT {
        return Err(format!(
            "expected {EXPECTED_SIGNAL_COUNT} signals, observed {observed}"
        ));
    }
    SIGNALS
        .iter()
        .map(|slot| {
            u32::try_from(slot.load(Ordering::SeqCst))
                .map_err(|_| "negative signal observation".to_string())
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; HEX_DIGIT_COUNT] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * HEX_CHARS_PER_BYTE);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> NIBBLE_BITS)]));
        output.push(char::from(HEX[usize::from(byte & LOW_NIBBLE_MASK)]));
    }
    output
}

fn pointer_address<T>(pointer: *const T) -> Result<u64, String> {
    u64::try_from(pointer as usize).map_err(|_| "pointer address exceeds u64".to_string())
}

// r[impl chaoscontrol.guest_determinism.validation_fixture]
// r[impl chaoscontrol.guest_determinism.signals]
fn observe() -> Result<GuestDeterminismProbe, String> {
    let entropy = read_entropy()?;
    let monotonic_delta_ns = monotonic_delta()?;
    let stack_value = text_marker();
    let heap_value = Box::new(text_marker());
    let signal_order = observed_signal_order()?;
    Ok(GuestDeterminismProbe {
        schema: GUEST_DETERMINISM_PROBE_SCHEMA.to_string(),
        entropy_hex: hex(&entropy),
        monotonic_delta_ns,
        text_address: pointer_address(text_marker as *const ())?,
        stack_address: pointer_address(&stack_value)?,
        heap_address: pointer_address(&*heap_value)?,
        signal_order,
    })
}

fn main() {
    println!("{READY_MARKER}");
    match observe()
        .and_then(|probe| serde_json::to_string(&probe).map_err(|error| error.to_string()))
    {
        Ok(encoded) => println!("{PROBE_PREFIX}{encoded}"),
        Err(error) => println!("{ERROR_PREFIX}{error}"),
    }
    println!("chaoscontrol-guest-determinism-probe: done, idling");
    loop {
        // SAFETY: PID 1 intentionally waits for signals after emitting the fixture.
        unsafe {
            libc::pause();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encodes_all_nibbles() {
        const INPUT: [u8; 4] = [0x00, 0x1f, 0xa0, 0xff];
        assert_eq!(hex(&INPUT), "001fa0ff");
    }

    #[test]
    fn signal_probe_observes_each_queued_signal_once() {
        let order = observed_signal_order().expect("signal observation");
        assert_eq!(order.len(), EXPECTED_SIGNAL_COUNT);
        let first_signal = u32::try_from(libc::SIGUSR1).expect("positive signal");
        let second_signal = u32::try_from(libc::SIGUSR2).expect("positive signal");
        assert!(order.contains(&first_signal));
        assert!(order.contains(&second_signal));
    }
}
