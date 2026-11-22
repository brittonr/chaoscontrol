//! Deterministic hostcalls for an exact differential scenario. Guest bytes are
//! compared with the canonical encoding of the declared request, not decoded.

use super::{
    validate_full_abi_profile, FullAbiProfile, ABI_REPLY_OFFSET, HOSTCALL_NAMES, WASM_PAGE_BYTES,
};
use std::sync::Arc;

/// Already typed scenario values; no guest memory or effect ports enter admission.
pub struct DeclaredCall<'a> {
    pub name: &'a str,
    pub request: &'a preserves::IOValue,
    pub reply: &'a preserves::IOValue,
    pub is_pause_requested: bool,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ScriptIssue {
    #[error("invalid full-ABI profile: {0}")]
    Profile(String),
    #[error("scenario exceeds configured call or byte bounds")]
    Bounds,
    #[error("scenario names an undeclared hostcall")]
    Name,
    #[error("cannot encode a bounded canonical scenario value")]
    Encoding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RequestTrap {
    #[error("unexpected hostcall")]
    UnexpectedCall,
    #[error("guest memory is outside the admitted bound")]
    Memory,
    #[error("hostcall request pointer or length is outside its bound")]
    RequestRange,
    #[error("hostcall request is not the declared canonical packet")]
    RequestBytes,
    #[error("hostcall reply does not fit guest memory")]
    ReplyRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub name: &'static str,
    pub request_blake3: blake3::Hash,
    pub reply_blake3: blake3::Hash,
    pub is_pause_requested: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reply {
    /// ABI `i64`: offset in the high 32 bits, length in the low 32 bits.
    pub descriptor: i64,
    pub is_pause_requested: bool,
}

struct Call {
    request: Vec<u8>,
    reply: Vec<u8>,
    exchange: Exchange,
}

pub struct Script {
    data: Arc<ScriptData>,
}

struct ScriptData {
    calls: Vec<Call>,
    maximum_call_bytes: usize,
    maximum_memory_bytes: usize,
}

impl Script {
    // r[impl chaoscontrol.spacewasm_full_abi.functional_core]
    pub fn admit(
        profile: &FullAbiProfile,
        calls: &[DeclaredCall<'_>],
    ) -> Result<Self, ScriptIssue> {
        validate_full_abi_profile(profile).map_err(ScriptIssue::Profile)?;
        let mut remaining =
            usize::try_from(profile.bounds.maximum_input_bytes).map_err(|_| ScriptIssue::Bounds)?;
        Self::for_checked_profile(profile, calls, &mut remaining)
    }

    pub(super) fn for_checked_profile(
        profile: &FullAbiProfile,
        calls: &[DeclaredCall<'_>],
        remaining: &mut usize,
    ) -> Result<Self, ScriptIssue> {
        if calls.len() > profile.bounds.maximum_hostcalls_per_run {
            return Err(ScriptIssue::Bounds);
        }
        let maximum_call_bytes = usize::try_from(profile.bounds.maximum_hostcall_bytes)
            .map_err(|_| ScriptIssue::Bounds)?;
        let maximum_memory_bytes =
            usize::try_from(profile.abi.maximum_memory_pages * WASM_PAGE_BYTES)
                .map_err(|_| ScriptIssue::Bounds)?;
        let calls = calls
            .iter()
            .map(|call| compile(call, maximum_call_bytes, remaining))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            data: Arc::new(ScriptData {
                calls,
                maximum_call_bytes,
                maximum_memory_bytes,
            }),
        })
    }

    /// Each engine gets independent progress over the same admitted packets.
    pub fn start(&self) -> Run {
        Run {
            script: Arc::clone(&self.data),
            completed: 0,
            trap: None,
        }
    }
}

fn compile(
    declared: &DeclaredCall<'_>,
    per_value: usize,
    remaining: &mut usize,
) -> Result<Call, ScriptIssue> {
    let name = HOSTCALL_NAMES
        .into_iter()
        .find(|name| *name == declared.name)
        .ok_or(ScriptIssue::Name)?;
    let request = packet(declared.request, per_value, remaining)?;
    let reply = packet(declared.reply, per_value, remaining)?;
    let exchange = Exchange {
        name,
        request_blake3: blake3::hash(&request),
        reply_blake3: blake3::hash(&reply),
        is_pause_requested: declared.is_pause_requested,
    };
    Ok(Call {
        request,
        reply,
        exchange,
    })
}

pub(super) fn packet(
    value: &preserves::IOValue,
    per_value: usize,
    remaining: &mut usize,
) -> Result<Vec<u8>, ScriptIssue> {
    let mut sink = PacketSink {
        capacity: per_value.min(*remaining),
        bytes: Vec::new(),
    };
    let mut writer = preserves::PackedWriter::new(&mut sink).set_write_annotations(false);
    value
        .write(&mut writer, &mut preserves::IOValueDomainEncode)
        .map_err(|_| ScriptIssue::Encoding)?;
    *remaining -= sink.bytes.len();
    Ok(sink.bytes)
}

struct PacketSink {
    capacity: usize,
    bytes: Vec<u8>,
}

impl std::io::Write for PacketSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.capacity - self.bytes.len() {
            return Err(std::io::ErrorKind::FileTooLarge.into());
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct Run {
    script: Arc<ScriptData>,
    completed: usize,
    trap: Option<RequestTrap>,
}

impl Run {
    // r[impl chaoscontrol.spacewasm_full_abi.host]
    pub fn invoke(
        &mut self,
        name: &str,
        pointer: i32,
        length: i32,
        memory: &mut [u8],
    ) -> Result<Reply, RequestTrap> {
        if let Some(trap) = self.trap {
            return Err(trap);
        }
        match self.place_reply(name, pointer, length, memory) {
            Ok(reply) => {
                self.completed += 1;
                Ok(reply)
            }
            Err(trap) => {
                self.trap = Some(trap);
                Err(trap)
            }
        }
    }

    pub fn transcript(&self) -> impl ExactSizeIterator<Item = &Exchange> {
        self.script.calls[..self.completed]
            .iter()
            .map(|call| &call.exchange)
    }

    pub const fn trap(&self) -> Option<RequestTrap> {
        self.trap
    }

    pub fn is_complete(&self) -> bool {
        self.trap.is_none() && self.completed == self.script.calls.len()
    }

    fn place_reply(
        &self,
        name: &str,
        pointer: i32,
        length: i32,
        memory: &mut [u8],
    ) -> Result<Reply, RequestTrap> {
        let call = self
            .script
            .calls
            .get(self.completed)
            .filter(|call| call.exchange.name == name)
            .ok_or(RequestTrap::UnexpectedCall)?;
        if memory.len() > self.script.maximum_memory_bytes {
            return Err(RequestTrap::Memory);
        }
        let length = usize::try_from(length).map_err(|_| RequestTrap::RequestRange)?;
        if length > self.script.maximum_call_bytes {
            return Err(RequestTrap::RequestRange);
        }
        let request = view(
            memory,
            usize::try_from(pointer).map_err(|_| RequestTrap::RequestRange)?,
            length,
        )
        .ok_or(RequestTrap::RequestRange)?;
        if request != call.request {
            return Err(RequestTrap::RequestBytes);
        }
        let reply_offset = ABI_REPLY_OFFSET as usize;
        let reply_end = reply_offset
            .checked_add(call.reply.len())
            .ok_or(RequestTrap::ReplyRange)?;
        let reply = memory
            .get_mut(reply_offset..reply_end)
            .ok_or(RequestTrap::ReplyRange)?;
        reply.copy_from_slice(&call.reply);
        // Both fields fit in u32 by profile admission; offset 4096 keeps the signed result positive.
        let descriptor = ((ABI_REPLY_OFFSET as i64) << 32) | call.reply.len() as i64;
        Ok(Reply {
            descriptor,
            is_pause_requested: call.exchange.is_pause_requested,
        })
    }
}

fn view(memory: &[u8], offset: usize, length: usize) -> Option<&[u8]> {
    memory.get(offset..offset.checked_add(length)?)
}

#[cfg(test)]
mod tests;
