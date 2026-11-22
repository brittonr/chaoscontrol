//! Exact already-admitted inputs shared by the full-ABI engine shells.

use super::host::{self, DeclaredCall, Exchange, RequestTrap, Script, ScriptIssue};
use super::{admit_full_abi_module, FullAbiAdmission, FullAbiProfile};
use crate::admission::AdmissionDenial;

// The exact Molten actor input and output limits, independent of module size.
const MAXIMUM_INPUT_PACKET_BYTES: usize = 8 * 1024;
const MAXIMUM_OUTPUT_PACKET_BYTES: usize = 8 * 1024;

#[derive(Debug)]
pub enum CaseDenial {
    Module(AdmissionDenial),
    Input(ScriptIssue),
    Output(ScriptIssue),
    Hostcalls(ScriptIssue),
}

/// Keep the admitted bytes and profile borrowed until the engine has finished.
pub struct AdmittedCase<'a> {
    pub(super) profile: &'a FullAbiProfile,
    pub(super) module: &'a [u8],
    pub(super) admission: FullAbiAdmission,
    pub(super) script: Script,
    pub(super) input: Vec<u8>,
    expected_output: Vec<u8>,
}

impl<'a> AdmittedCase<'a> {
    // r[impl chaoscontrol.spacewasm_full_abi.functional_core]
    pub fn admit(
        profile: &'a FullAbiProfile,
        module: &'a [u8],
        input: &preserves::IOValue,
        expected_output: &preserves::IOValue,
        hostcalls: &[DeclaredCall<'_>],
    ) -> Result<Self, CaseDenial> {
        let admission = admit_full_abi_module(profile, module).map_err(CaseDenial::Module)?;
        let mut exchange_bound = profile.bounds.maximum_input_bytes as usize;
        let input = host::packet(input, MAXIMUM_INPUT_PACKET_BYTES, &mut exchange_bound)
            .map_err(CaseDenial::Input)?;
        let expected_output = host::packet(
            expected_output,
            MAXIMUM_OUTPUT_PACKET_BYTES,
            &mut exchange_bound,
        )
        .map_err(CaseDenial::Output)?;
        let script = Script::for_checked_profile(profile, hostcalls, &mut exchange_bound)
            .map_err(CaseDenial::Hostcalls)?;
        Ok(Self {
            profile,
            module,
            admission,
            script,
            input,
            expected_output,
        })
    }

    /// Borrow the bounded output; comparison never parses or copies guest bytes.
    pub(super) fn match_output(&self, memory: &[u8], descriptor: i64) -> Result<Output, Terminal> {
        let bits = descriptor as u64;
        let pointer = i32::try_from(bits >> 32).map_err(|_| Terminal::OutputRange)?;
        let length = (bits as u32) as usize;
        if length > MAXIMUM_OUTPUT_PACKET_BYTES
            || length as u64 > self.profile.bounds.maximum_input_bytes
        {
            return Err(Terminal::OutputRange);
        }
        let bytes = span(pointer, length)
            .and_then(|range| memory.get(range))
            .ok_or(Terminal::OutputRange)?;
        if bytes != self.expected_output {
            return Err(Terminal::OutputMismatch);
        }
        Ok(Output {
            pointer,
            length: length as i32,
            blake3: blake3::hash(bytes),
        })
    }
}

pub(super) fn span(pointer: i32, length: usize) -> Option<std::ops::Range<usize>> {
    let offset = usize::try_from(pointer).ok()?;
    Some(offset..offset.checked_add(length)?)
}

pub(super) struct Output {
    pub pointer: i32,
    pub length: i32,
    pub blake3: blake3::Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    Completed {
        output_blake3: blake3::Hash,
        output_bytes: u32,
    },
    InputRange,
    OutputRange,
    OutputMismatch,
    Hostcall(RequestTrap),
    IncompleteHostcalls,
    MemoryTrap,
    StackLimit,
    FuelExhausted,
    GuestTrap,
}

/// A single engine's local execution, never a parity or release receipt.
#[derive(Debug, PartialEq, Eq)]
pub struct DiagnosticObservation {
    pub terminal: Terminal,
    pub transcript: Vec<Exchange>,
    pub fuel_consumed: u64,
}
