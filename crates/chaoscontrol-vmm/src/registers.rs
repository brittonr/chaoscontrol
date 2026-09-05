//! Guest register state — portable representation for debugger and dlog.

use std::fmt;
use std::str::FromStr;

/// VM register state — all general-purpose, segment, and control registers.
///
/// This is a VMM-independent representation that can be serialized and
/// passed to the replay debugger without depending on kvm_bindings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegisterState {
    pub rip: u64,
    pub rsp: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rflags: u64,
    pub cs: u64,
    pub ss: u64,
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
    pub cr0: u64,
    pub cr3: u64,
    pub cr4: u64,
}

impl RegisterState {
    /// Build from KVM register structs.
    pub fn from_kvm(regs: &kvm_bindings::kvm_regs, sregs: &kvm_bindings::kvm_sregs) -> Self {
        Self {
            rip: regs.rip,
            rsp: regs.rsp,
            rax: regs.rax,
            rbx: regs.rbx,
            rcx: regs.rcx,
            rdx: regs.rdx,
            rsi: regs.rsi,
            rdi: regs.rdi,
            rbp: regs.rbp,
            r8: regs.r8,
            r9: regs.r9,
            r10: regs.r10,
            r11: regs.r11,
            r12: regs.r12,
            r13: regs.r13,
            r14: regs.r14,
            r15: regs.r15,
            rflags: regs.rflags,
            cs: sregs.cs.base,
            ss: sregs.ss.base,
            ds: sregs.ds.base,
            es: sregs.es.base,
            fs: sregs.fs.base,
            gs: sregs.gs.base,
            cr0: sregs.cr0,
            cr3: sregs.cr3,
            cr4: sregs.cr4,
        }
    }

    /// Apply general-purpose register values to a KVM regs struct.
    ///
    /// Only writes the GP registers + RFLAGS. Segment and control
    /// registers require separate set_sregs() handling.
    pub fn apply_to_kvm_regs(&self, regs: &mut kvm_bindings::kvm_regs) {
        regs.rip = self.rip;
        regs.rsp = self.rsp;
        regs.rax = self.rax;
        regs.rbx = self.rbx;
        regs.rcx = self.rcx;
        regs.rdx = self.rdx;
        regs.rsi = self.rsi;
        regs.rdi = self.rdi;
        regs.rbp = self.rbp;
        regs.r8 = self.r8;
        regs.r9 = self.r9;
        regs.r10 = self.r10;
        regs.r11 = self.r11;
        regs.r12 = self.r12;
        regs.r13 = self.r13;
        regs.r14 = self.r14;
        regs.r15 = self.r15;
        regs.rflags = self.rflags;
    }
}

impl fmt::Display for RegisterState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "rip={:#018x}  rsp={:#018x}  rflags={:#018x}",
            self.rip, self.rsp, self.rflags
        )?;
        writeln!(
            f,
            "rax={:#018x}  rbx={:#018x}  rcx={:#018x}  rdx={:#018x}",
            self.rax, self.rbx, self.rcx, self.rdx
        )?;
        writeln!(
            f,
            "rsi={:#018x}  rdi={:#018x}  rbp={:#018x}",
            self.rsi, self.rdi, self.rbp
        )?;
        writeln!(
            f,
            "r8 ={:#018x}  r9 ={:#018x}  r10={:#018x}  r11={:#018x}",
            self.r8, self.r9, self.r10, self.r11
        )?;
        writeln!(
            f,
            "r12={:#018x}  r13={:#018x}  r14={:#018x}  r15={:#018x}",
            self.r12, self.r13, self.r14, self.r15
        )?;
        writeln!(
            f,
            "cs={:#06x}  ss={:#06x}  ds={:#06x}  es={:#06x}  fs={:#06x}  gs={:#06x}",
            self.cs, self.ss, self.ds, self.es, self.fs, self.gs
        )?;
        write!(
            f,
            "cr0={:#018x}  cr3={:#018x}  cr4={:#018x}",
            self.cr0, self.cr3, self.cr4
        )
    }
}

/// Named register for individual register modifications.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Register {
    // NOTE: Display and FromStr impls are below.
    Rip,
    Rsp,
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    Rbp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    Rflags,
}

impl Register {
    /// Read this register's value from a RegisterState.
    pub fn get(&self, state: &RegisterState) -> u64 {
        match self {
            Self::Rip => state.rip,
            Self::Rsp => state.rsp,
            Self::Rax => state.rax,
            Self::Rbx => state.rbx,
            Self::Rcx => state.rcx,
            Self::Rdx => state.rdx,
            Self::Rsi => state.rsi,
            Self::Rdi => state.rdi,
            Self::Rbp => state.rbp,
            Self::R8 => state.r8,
            Self::R9 => state.r9,
            Self::R10 => state.r10,
            Self::R11 => state.r11,
            Self::R12 => state.r12,
            Self::R13 => state.r13,
            Self::R14 => state.r14,
            Self::R15 => state.r15,
            Self::Rflags => state.rflags,
        }
    }

    /// Set this register's value in a RegisterState.
    pub fn set(&self, state: &mut RegisterState, value: u64) {
        match self {
            Self::Rip => state.rip = value,
            Self::Rsp => state.rsp = value,
            Self::Rax => state.rax = value,
            Self::Rbx => state.rbx = value,
            Self::Rcx => state.rcx = value,
            Self::Rdx => state.rdx = value,
            Self::Rsi => state.rsi = value,
            Self::Rdi => state.rdi = value,
            Self::Rbp => state.rbp = value,
            Self::R8 => state.r8 = value,
            Self::R9 => state.r9 = value,
            Self::R10 => state.r10 = value,
            Self::R11 => state.r11 = value,
            Self::R12 => state.r12 = value,
            Self::R13 => state.r13 = value,
            Self::R14 => state.r14 = value,
            Self::R15 => state.r15 = value,
            Self::Rflags => state.rflags = value,
        }
    }
}

impl fmt::Display for Register {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Rip => "rip",
            Self::Rsp => "rsp",
            Self::Rax => "rax",
            Self::Rbx => "rbx",
            Self::Rcx => "rcx",
            Self::Rdx => "rdx",
            Self::Rsi => "rsi",
            Self::Rdi => "rdi",
            Self::Rbp => "rbp",
            Self::R8 => "r8",
            Self::R9 => "r9",
            Self::R10 => "r10",
            Self::R11 => "r11",
            Self::R12 => "r12",
            Self::R13 => "r13",
            Self::R14 => "r14",
            Self::R15 => "r15",
            Self::Rflags => "rflags",
        };
        f.write_str(name)
    }
}

impl FromStr for Register {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "rip" => Ok(Self::Rip),
            "rsp" => Ok(Self::Rsp),
            "rax" => Ok(Self::Rax),
            "rbx" => Ok(Self::Rbx),
            "rcx" => Ok(Self::Rcx),
            "rdx" => Ok(Self::Rdx),
            "rsi" => Ok(Self::Rsi),
            "rdi" => Ok(Self::Rdi),
            "rbp" => Ok(Self::Rbp),
            "r8" => Ok(Self::R8),
            "r9" => Ok(Self::R9),
            "r10" => Ok(Self::R10),
            "r11" => Ok(Self::R11),
            "r12" => Ok(Self::R12),
            "r13" => Ok(Self::R13),
            "r14" => Ok(Self::R14),
            "r15" => Ok(Self::R15),
            "rflags" => Ok(Self::Rflags),
            _ => Err(format!("unknown register: {s}")),
        }
    }
}

/// Register modification for counterfactual replay.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegisterModification {
    /// Which VM to modify.
    pub vm_index: usize,
    /// Which vCPU to modify.
    pub vcpu: usize,
    /// Register changes: register → new value.
    pub changes: std::collections::BTreeMap<Register, u64>,
}
