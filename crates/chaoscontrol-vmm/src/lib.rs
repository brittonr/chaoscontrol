#![allow(unknown_lints)]
#![allow(
    explicit_defaults,
    reason = "VMM shell code initializes Linux/KVM ABI structs with kernel-provided zero defaults"
)]

//! ChaosControl VMM — a deterministic virtual machine monitor for simulation testing.
//!
//! This crate provides a KVM-backed VMM with all sources of non-determinism
//! controlled, making it suitable for deterministic simulation testing of
//! distributed systems.
//!
//! # Architecture
//!
//! - [`vm`] — Core VM creation, kernel loading, and execution
//! - [`cpu`] — CPUID filtering, TSC pinning, virtual TSC tracking
//! - [`memory`] — Guest memory management, page tables, GDT, snapshots
//! - [`snapshot`] — Complete VM state capture and restore
//! - [`devices`] — Deterministic device backends
//! - [`controller`] — Multi-VM simulation controller
//! - [`verified`] — Pure functions extracted for formal verification with Verus

pub mod acpi;
pub mod controller;
pub mod cpu;
pub mod determinism_gate;
pub mod devices;
pub mod dlog;
pub mod memory;
pub mod perf;
pub mod registers;
pub mod scheduler;
pub mod sim_adapter;
pub mod snapshot;
pub mod verified;
pub mod vm;
