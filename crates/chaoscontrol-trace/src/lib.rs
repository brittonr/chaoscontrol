//! eBPF-based KVM tracing harness for ChaosControl.
//!
//! This crate provides non-invasive tracing of KVM virtual machines from
//! the host side using eBPF. It attaches to kernel KVM tracepoints to
//! observe admitted KVM tracepoint records without modifying the guest.
//! Complete evidence requires producer and userspace accounting.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │  ChaosControl VMM Process           │
//! │  (runs guest via KVM ioctls)        │
//! └──────────────┬──────────────────────┘
//!                │ ioctl(KVM_RUN)
//! ═══════════════╪══════════════════════════
//!                │ KVM Module
//! ┌──────────────▼──────────────────────┐
//! │  KVM Tracepoints (in kernel):       │
//! │    kvm_exit, kvm_entry, kvm_pio,    │
//! │    kvm_mmio, kvm_msr, kvm_inj_virq, │
//! │    kvm_set_irq, kvm_page_fault, ... │
//! └──────────────┬──────────────────────┘
//!                │ eBPF attachment
//! ┌──────────────▼──────────────────────┐
//! │  BPF Ring Buffer → Userspace        │
//! │  chaoscontrol-trace Collector       │
//! │    → TraceEvent stream              │
//! │    → legacy TraceLog diagnostics    │
//! │    → typed evidence manifests       │
//! └─────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ## Live tracing
//!
//! ```no_run
//! use chaoscontrol_trace::collector::{Collector, CollectorConfig};
//!
//! // Attach to a running ChaosControl VMM process
//! let config = CollectorConfig::for_pid(12345);
//! let mut collector = Collector::attach(config).unwrap();
//!
//! // Poll for events periodically
//! collector.poll().unwrap();
//! let events = collector.drain();
//! for event in &events {
//!     println!("{}", event);
//! }
//! ```
//!
//! ## Legacy debug comparison
//!
//! ```no_run
//! use chaoscontrol_trace::collector::TraceLog;
//! use chaoscontrol_trace::verifier::DeterminismVerifier;
//!
//! let trace_a = TraceLog::load("run1.json").unwrap();
//! let trace_b = TraceLog::load("run2.json").unwrap();
//!
//! let diagnostic = DeterminismVerifier::compare(&trace_a, &trace_b);
//! println!("legacy diagnostic only: {}", diagnostic);
//! // Use evidence::compare_complete_traces for a bounded evidence verdict.
//! ```

pub mod collector;
pub mod events;
pub mod evidence;
pub mod verified;
pub mod verifier;
