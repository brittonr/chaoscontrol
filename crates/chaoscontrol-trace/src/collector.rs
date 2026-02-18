//! BPF program loader and event collector.
//!
//! [`Collector`] loads the compiled KVM tracing BPF program, attaches it
//! to KVM tracepoints, and collects events from the ring buffer.
//!
//! Requires `CAP_SYS_ADMIN` (root) for BPF program loading and
//! tracepoint attachment.

use crate::events::{RawEvent, TraceEvent};
use libbpf_rs::skel::{OpenSkel, Skel, SkelBuilder};
use libbpf_rs::MapCore;
use log::info;
use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

// Generated skeleton from libbpf-cargo build step
mod kvm_trace_skel {
    include!(concat!(env!("OUT_DIR"), "/kvm_trace.skel.rs"));
}
use kvm_trace_skel::*;

// ═══════════════════════════════════════════════════════════════════════
//  Error type
// ═══════════════════════════════════════════════════════════════════════

#[derive(Error, Debug)]
pub enum CollectorError {
    #[error("Failed to open BPF skeleton: {0}")]
    Open(#[source] libbpf_rs::Error),

    #[error("Failed to load BPF programs: {0}")]
    Load(#[source] libbpf_rs::Error),

    #[error("Failed to attach BPF programs: {0}")]
    Attach(#[source] libbpf_rs::Error),

    #[error("Failed to update BPF map: {0}")]
    MapUpdate(#[source] libbpf_rs::Error),

    #[error("Failed to build ring buffer: {0}")]
    RingBuffer(#[source] libbpf_rs::Error),

    #[error("Failed to poll ring buffer: {0}")]
    Poll(#[source] libbpf_rs::Error),

    #[error("Not running as root (CAP_SYS_ADMIN required for BPF)")]
    NotRoot,
}

// ═══════════════════════════════════════════════════════════════════════
//  Collector configuration
// ═══════════════════════════════════════════════════════════════════════

/// Configuration for the trace collector.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// PID to trace (filters BPF events to this process).
    pub target_pid: u32,
    /// Ring buffer poll timeout.
    pub poll_timeout: Duration,
}

impl CollectorConfig {
    /// Create config targeting the current process.
    pub fn current_process() -> Self {
        Self {
            target_pid: std::process::id(),
            poll_timeout: Duration::from_millis(100),
        }
    }

    /// Create config targeting a specific PID.
    pub fn for_pid(pid: u32) -> Self {
        Self {
            target_pid: pid,
            poll_timeout: Duration::from_millis(100),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Collector
// ═══════════════════════════════════════════════════════════════════════

/// Collects KVM trace events via eBPF.
///
/// The collector loads a BPF program that attaches to KVM tracepoints
/// in the kernel and emits events to a ring buffer. Events are parsed
/// into typed [`TraceEvent`]s on the Rust side.
///
/// # Requirements
///
/// - Must run as root (CAP_SYS_ADMIN for BPF)
/// - Kernel must have KVM module loaded
/// - Kernel must have BTF support (CONFIG_DEBUG_INFO_BTF=y)
///
/// # Example
///
/// ```no_run
/// use chaoscontrol_trace::collector::{Collector, CollectorConfig};
///
/// let config = CollectorConfig::for_pid(12345);
/// let mut collector = Collector::attach(config).unwrap();
///
/// // Later, drain collected events
/// let events = collector.drain();
/// println!("Got {} events", events.len());
/// ```
pub struct Collector {
    // Shared event buffer (ring buffer callback appends here)
    events: Arc<Mutex<Vec<TraceEvent>>>,
    // Ring buffer handle — boxed to erase lifetimes.
    // Safe because we keep the skeleton (and thus BPF maps) alive.
    ring_buf: Box<dyn RingBufPollable>,
    // Config
    config: CollectorConfig,
    // Statistics
    total_events: u64,
    poll_count: u64,
}

/// Trait object for ring buffer polling (erases lifetime).
trait RingBufPollable {
    fn poll(&self, timeout: Duration) -> Result<(), libbpf_rs::Error>;
}

impl RingBufPollable for libbpf_rs::RingBuffer<'_> {
    fn poll(&self, timeout: Duration) -> Result<(), libbpf_rs::Error> {
        libbpf_rs::RingBuffer::poll(self, timeout).map(|_| ())
    }
}

impl Collector {
    /// Load and attach the BPF tracing program.
    ///
    /// This attaches to all supported KVM tracepoints and begins
    /// collecting events for the target PID.
    pub fn attach(config: CollectorConfig) -> Result<Self, CollectorError> {
        // Check root
        if unsafe { libc::geteuid() != 0 } {
            return Err(CollectorError::NotRoot);
        }

        info!(
            "Attaching KVM trace collector for PID {}",
            config.target_pid
        );

        // The libbpf-rs 0.26 skeleton API requires an OpenObject that
        // outlives the skeleton. We leak it since the Collector typically
        // lives for the entire process duration.
        let open_object: &'static mut MaybeUninit<libbpf_rs::OpenObject> =
            Box::leak(Box::new(MaybeUninit::<libbpf_rs::OpenObject>::uninit()));

        // Open BPF skeleton
        let skel_builder = KvmTraceSkelBuilder::default();
        let open_skel = skel_builder
            .open(open_object)
            .map_err(CollectorError::Open)?;

        // Load BPF programs into kernel
        let mut skel = open_skel.load().map_err(CollectorError::Load)?;

        // Set target PID filter
        let key = 0u32.to_ne_bytes();
        let val = config.target_pid.to_ne_bytes();
        skel.maps
            .target_pid
            .update(&key, &val, libbpf_rs::MapFlags::ANY)
            .map_err(CollectorError::MapUpdate)?;

        info!("BPF programs loaded, setting up ring buffer");

        // Attach all tracepoint programs
        skel.attach().map_err(CollectorError::Attach)?;

        info!("Attached to KVM tracepoints");

        // Set up ring buffer consumer
        let events: Arc<Mutex<Vec<TraceEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let mut builder = libbpf_rs::RingBufferBuilder::new();
        builder
            .add(&skel.maps.events, move |data: &[u8]| -> i32 {
                if data.len() >= std::mem::size_of::<RawEvent>() {
                    let raw =
                        unsafe { std::ptr::read_unaligned(data.as_ptr() as *const RawEvent) };
                    let event = TraceEvent::from_raw(&raw);
                    if let Ok(mut buf) = events_clone.lock() {
                        buf.push(event);
                    }
                }
                0 // continue processing
            })
            .map_err(CollectorError::RingBuffer)?;

        let ring_buf = builder.build().map_err(CollectorError::RingBuffer)?;

        info!("Ring buffer ready, collector active");

        // We need to erase the lifetime of the ring buffer. This is safe
        // because _open_object (which owns the BPF maps) is kept alive
        // in Self and dropped after ring_buf.
        let ring_buf: Box<dyn RingBufPollable> =
            unsafe { std::mem::transmute::<Box<dyn RingBufPollable>, Box<dyn RingBufPollable>>(Box::new(ring_buf)) };

        Ok(Self {
            events,
            ring_buf,
            config,
            total_events: 0,
            poll_count: 0,
        })
    }

    /// Poll the ring buffer for new events.
    ///
    /// This should be called periodically to drain events from the
    /// kernel ring buffer into the userspace event vector.
    ///
    /// Returns the number of new events collected.
    pub fn poll(&mut self) -> Result<usize, CollectorError> {
        let before = self.events.lock().unwrap().len();
        self.ring_buf
            .poll(self.config.poll_timeout)
            .map_err(CollectorError::Poll)?;
        let after = self.events.lock().unwrap().len();
        let new = after - before;
        self.total_events += new as u64;
        self.poll_count += 1;
        Ok(new)
    }

    /// Drain all collected events, clearing the internal buffer.
    ///
    /// Call [`poll`](Self::poll) first to ensure events are transferred
    /// from the kernel ring buffer.
    pub fn drain(&mut self) -> Vec<TraceEvent> {
        let mut buf = self.events.lock().unwrap();
        std::mem::take(&mut *buf)
    }

    /// Get a snapshot of collected events without clearing.
    pub fn peek(&self) -> Vec<TraceEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Number of events collected since creation.
    pub fn total_events(&self) -> u64 {
        self.total_events
    }

    /// Number of poll() calls since creation.
    pub fn poll_count(&self) -> u64 {
        self.poll_count
    }

    /// The target PID being traced.
    pub fn target_pid(&self) -> u32 {
        self.config.target_pid
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Trace recording (collect events into a trace log)
// ═══════════════════════════════════════════════════════════════════════

/// A recorded trace: a sequence of events from a single VM run.
///
/// Used for determinism verification (compare two traces) and
/// debugging (replay/inspect event sequence).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceLog {
    /// Target PID that was traced.
    pub pid: u32,
    /// All collected events in order.
    pub events: Vec<TraceEvent>,
    /// Host info for reproducibility checking.
    pub metadata: TraceMetadata,
}

/// Metadata about the trace environment.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceMetadata {
    /// Kernel version string.
    pub kernel_version: String,
    /// CPU model string.
    pub cpu_model: String,
    /// Timestamp when trace started (wall clock).
    pub start_time: String,
}

impl TraceMetadata {
    /// Gather metadata from the current system.
    pub fn gather() -> Self {
        let kernel_version = std::fs::read_to_string("/proc/version")
            .unwrap_or_default()
            .trim()
            .to_string();
        let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("model name"))
            .unwrap_or("unknown")
            .to_string();
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts);
        }
        let start_time = format!("{}s_{}ns", ts.tv_sec, ts.tv_nsec);

        Self {
            kernel_version,
            cpu_model,
            start_time,
        }
    }
}

impl TraceLog {
    /// Create a new trace log from collected events.
    pub fn new(pid: u32, events: Vec<TraceEvent>) -> Self {
        Self {
            pid,
            events,
            metadata: TraceMetadata::gather(),
        }
    }

    /// Save trace to a JSON file.
    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Load trace from a JSON file.
    pub fn load(path: &str) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(std::io::Error::other)
    }

    /// Number of events in the trace.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the trace is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Summary statistics about event types.
    pub fn summary(&self) -> std::collections::HashMap<String, usize> {
        let mut counts = std::collections::HashMap::new();
        for event in &self.events {
            *counts
                .entry(event.event_type().name().to_string())
                .or_insert(0) += 1;
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_config_current_process() {
        let config = CollectorConfig::current_process();
        assert_eq!(config.target_pid, std::process::id());
    }

    #[test]
    fn trace_log_serialization() {
        let log = TraceLog::new(1234, vec![]);
        let json = serde_json::to_string(&log).unwrap();
        let loaded: TraceLog = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.pid, 1234);
        assert!(loaded.events.is_empty());
    }
}
