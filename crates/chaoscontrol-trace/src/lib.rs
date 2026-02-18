//! eBPF-based KVM tracing harness for ChaosControl.
//!
//! This crate provides non-invasive tracing of KVM virtual machines from
//! the host side using eBPF. It attaches to kernel KVM tracepoints to
//! observe every VM exit, I/O operation, interrupt injection, MSR access,
//! and more — without modifying the guest.
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
//! │    → TraceLog (save/load)           │
//! │    → DeterminismVerifier            │
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
//! ## Determinism verification
//!
//! ```no_run
//! use chaoscontrol_trace::collector::TraceLog;
//! use chaoscontrol_trace::verifier::DeterminismVerifier;
//!
//! let trace_a = TraceLog::load("run1.json").unwrap();
//! let trace_b = TraceLog::load("run2.json").unwrap();
//!
//! let result = DeterminismVerifier::compare(&trace_a, &trace_b);
//! println!("{}", result);
//! assert!(result.is_deterministic);
//! ```

pub mod collector;
pub mod events;
pub mod verifier;
