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
pub mod perf;
pub mod controller;
pub mod cpu;
pub mod devices;
pub mod memory;
pub mod scheduler;
pub mod snapshot;
pub mod verified;
pub mod vm;
