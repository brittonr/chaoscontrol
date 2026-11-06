//! Pure retained-moment admission. Historical windows require no VM; restored
//! or future-of-moment windows require an admitted exact persisted session.

use crate::recording::Recording;
use crate::session::{valid_identity, ExactSession, SessionBranch};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_MOMENT_SOURCES: usize = 64;
pub const MAX_MOMENT_TICKS: u64 = 1_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MomentSelector {
    Tick(u64),
    Checkpoint(u64),
    Branch(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MomentRequest {
    pub selector: MomentSelector,
    pub past_ticks: u64,
    pub future_ticks: u64,
    pub requires_restored_state: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MomentCheckpoint {
    pub id: u64,
    pub tick: u64,
    pub identity: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedHistory {
    pub source_ids: Vec<String>,
    pub start_tick: u64,
    pub end_tick: u64,
    pub checkpoints: Vec<MomentCheckpoint>,
    pub branches: Vec<SessionBranch>,
}
impl RetainedHistory {
    pub fn from_recording(
        recording: &Recording,
        mut source_ids: Vec<String>,
    ) -> Result<Self, MomentError> {
        source_ids.sort();
        let history = Self {
            source_ids,
            start_tick: 0,
            end_tick: recording.total_ticks,
            checkpoints: recording
                .checkpoints
                .all()
                .iter()
                .map(|checkpoint| MomentCheckpoint {
                    id: checkpoint.id,
                    tick: checkpoint.tick,
                    identity: None,
                })
                .collect(),
            branches: Vec::new(),
        };
        history.validate()?;
        Ok(history)
    }
    pub fn from_session(
        session: &ExactSession,
        mut source_ids: Vec<String>,
    ) -> Result<Self, MomentError> {
        source_ids.sort();
        let manifest = session.manifest();
        let history = Self {
            source_ids,
            start_tick: manifest.retained_start_tick,
            end_tick: manifest.retained_end_tick,
            checkpoints: manifest
                .checkpoints
                .iter()
                .map(|checkpoint| MomentCheckpoint {
                    id: checkpoint.id,
                    tick: checkpoint.tick,
                    identity: Some(checkpoint.artifact.digest.clone()),
                })
                .collect(),
            branches: manifest.branches.clone(),
        };
        history.validate()?;
        Ok(history)
    }
    pub fn validate(&self) -> Result<(), MomentError> {
        validate_sources(&self.source_ids)?;
        if self.start_tick > self.end_tick {
            return Err(MomentError::InvalidHistory);
        }
        if self.checkpoints.len() > crate::session::MAX_SESSION_CHECKPOINTS
            || self.branches.len() > crate::session::MAX_SESSION_BRANCHES
        {
            return Err(MomentError::Bound);
        }
        let mut ids = BTreeSet::new();
        let mut prior = None;
        for checkpoint in &self.checkpoints {
            if !ids.insert(checkpoint.id)
                || checkpoint.tick < self.start_tick
                || checkpoint.tick > self.end_tick
                || prior.is_some_and(|tick| checkpoint.tick < tick)
                || checkpoint
                    .identity
                    .as_deref()
                    .is_some_and(|id| !valid_identity(id))
            {
                return Err(MomentError::InvalidHistory);
            }
            prior = Some(checkpoint.tick);
        }
        let mut branches = BTreeSet::new();
        for branch in &self.branches {
            if !valid_identity(&branch.branch_id)
                || !branches.insert(branch.branch_id.as_str())
                || !self.checkpoints.iter().any(|checkpoint| {
                    checkpoint.id == branch.checkpoint_id && checkpoint.tick == branch.tick
                })
            {
                return Err(MomentError::InvalidHistory);
            }
        }
        if !self.branches.is_empty()
            && self
                .branches
                .iter()
                .filter(|branch| branch.parent.is_none())
                .count()
                != 1
        {
            return Err(MomentError::InvalidHistory);
        }
        for branch in &self.branches {
            let mut cursor = branch;
            let mut seen = BTreeSet::new();
            while let Some(parent_id) = &cursor.parent {
                if !seen.insert(cursor.branch_id.as_str()) {
                    return Err(MomentError::InvalidHistory);
                }
                let parent = self
                    .branches
                    .iter()
                    .find(|parent| &parent.branch_id == parent_id)
                    .ok_or(MomentError::InvalidHistory)?;
                if parent.tick > cursor.tick {
                    return Err(MomentError::InvalidHistory);
                }
                cursor = parent;
            }
        }
        Ok(())
    }
}

/// Receipt binding for the exact admitted selection and declared evidence
/// horizon. Identity strings are canonical `b3:<64 lowercase hex>` values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MomentContext {
    pub tick: u64,
    pub start_tick: u64,
    pub end_tick: u64,
    pub retained_start_tick: u64,
    pub retained_end_tick: u64,
    pub source_ids: Vec<String>,
    pub session_id: Option<String>,
    pub checkpoint_id: Option<String>,
    pub branch_id: Option<String>,
}
impl MomentContext {
    pub fn validate(&self) -> Result<(), MomentError> {
        validate_sources(&self.source_ids)?;
        if self.retained_start_tick > self.start_tick
            || self.start_tick > self.tick
            || self.tick > self.end_tick
            || self.end_tick > self.retained_end_tick
            || self.tick - self.start_tick > MAX_MOMENT_TICKS
            || self.end_tick - self.tick > MAX_MOMENT_TICKS
        {
            return Err(MomentError::Horizon);
        }
        if self
            .session_id
            .as_deref()
            .is_some_and(|id| !valid_identity(id))
            || self
                .checkpoint_id
                .as_deref()
                .is_some_and(|id| !valid_identity(id))
            || self
                .branch_id
                .as_deref()
                .is_some_and(|id| !valid_identity(id))
        {
            return Err(MomentError::InvalidIdentity);
        }
        if self.session_id.is_some() != self.checkpoint_id.is_some()
            || (self.end_tick > self.tick && self.session_id.is_none())
        {
            return Err(MomentError::MissingSession);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MomentError {
    MissingSession,
    MissingCheckpoint,
    UnknownBranch,
    Horizon,
    Bound,
    InvalidIdentity,
    InvalidHistory,
    SessionMismatch,
}
impl std::fmt::Display for MomentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingSession => "future/restored moment requires an exact persisted session",
            Self::MissingCheckpoint => {
                "moment requires a validated checkpoint at or before its tick"
            }
            Self::UnknownBranch => "moment branch identity is not retained",
            Self::Horizon => "moment or declared window is outside its retained horizon",
            Self::Bound => "moment bounds exceeded",
            Self::InvalidIdentity => "moment artifact identity is invalid",
            Self::InvalidHistory => "retained moment/checkpoint/branch history is inconsistent",
            Self::SessionMismatch => "retained history does not match the exact session",
        })
    }
}
impl std::error::Error for MomentError {}
fn validate_sources(sources: &[String]) -> Result<(), MomentError> {
    if sources.is_empty() || sources.len() > MAX_MOMENT_SOURCES {
        return Err(MomentError::Bound);
    }
    if sources.iter().any(|id| !valid_identity(id))
        || sources.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(MomentError::InvalidIdentity);
    }
    Ok(())
}

// r[impl chaoscontrol.debug_query.moment]
// r[impl chaoscontrol.debug_query.evidence]
pub fn admit_moment(
    request: &MomentRequest,
    history: &RetainedHistory,
    session: Option<&ExactSession>,
) -> Result<MomentContext, MomentError> {
    history.validate()?;
    if request.past_ticks > MAX_MOMENT_TICKS || request.future_ticks > MAX_MOMENT_TICKS {
        return Err(MomentError::Bound);
    }
    let (tick, branch_id, explicit_checkpoint) = match &request.selector {
        MomentSelector::Tick(tick) => (*tick, None, None),
        MomentSelector::Checkpoint(id) => {
            let checkpoint = history
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.id == *id)
                .ok_or(MomentError::MissingCheckpoint)?;
            (checkpoint.tick, None, Some(*id))
        }
        MomentSelector::Branch(id) => {
            let branch = history
                .branches
                .iter()
                .find(|branch| &branch.branch_id == id)
                .ok_or(MomentError::UnknownBranch)?;
            (
                branch.tick,
                Some(branch.branch_id.clone()),
                Some(branch.checkpoint_id),
            )
        }
    };
    let start_tick = tick
        .checked_sub(request.past_ticks)
        .ok_or(MomentError::Horizon)?;
    let end_tick = tick
        .checked_add(request.future_ticks)
        .ok_or(MomentError::Horizon)?;
    if start_tick < history.start_tick || end_tick > history.end_tick || tick < history.start_tick {
        return Err(MomentError::Horizon);
    }
    let needs_exact = request.future_ticks != 0 || request.requires_restored_state;
    let (session_id, checkpoint_id) = if needs_exact {
        let session = session.ok_or(MomentError::MissingSession)?;
        let manifest = session.manifest();
        if manifest.retained_start_tick != history.start_tick
            || manifest.retained_end_tick != history.end_tick
        {
            return Err(MomentError::SessionMismatch);
        }
        // Identity-bearing retained inputs must include this exact recording.
        if !history.source_ids.contains(&manifest.recording.digest) {
            return Err(MomentError::SessionMismatch);
        }
        for retained in &history.checkpoints {
            let checkpoint = manifest
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.id == retained.id)
                .ok_or(MomentError::SessionMismatch)?;
            if retained.tick != checkpoint.tick
                || retained
                    .identity
                    .as_deref()
                    .is_some_and(|identity| identity != checkpoint.artifact.digest)
            {
                return Err(MomentError::SessionMismatch);
            }
        }
        let checkpoint = if let Some(id) = explicit_checkpoint {
            manifest
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.id == id)
        } else {
            manifest
                .checkpoints
                .iter()
                .rev()
                .find(|checkpoint| checkpoint.tick <= tick)
        }
        .ok_or(MomentError::MissingCheckpoint)?;
        if let Some(id) = &branch_id {
            let branch = manifest
                .branches
                .iter()
                .find(|branch| &branch.branch_id == id)
                .ok_or(MomentError::SessionMismatch)?;
            if branch.tick != tick || branch.checkpoint_id != checkpoint.id {
                return Err(MomentError::SessionMismatch);
            }
        }
        (
            Some(manifest.session_id.clone()),
            Some(checkpoint.artifact.digest.clone()),
        )
    } else {
        (None, None)
    };
    let context = MomentContext {
        tick,
        start_tick,
        end_tick,
        retained_start_tick: history.start_tick,
        retained_end_tick: history.end_tick,
        source_ids: history.source_ids.clone(),
        session_id,
        checkpoint_id,
        branch_id,
    };
    context.validate()?;
    Ok(context)
}

/// Re-admit rather than trusting a serialized context, catching source/horizon,
/// selection and exact-session drift before receipt reuse or restored execution.
pub fn validate_moment(
    context: &MomentContext,
    request: &MomentRequest,
    history: &RetainedHistory,
    session: Option<&ExactSession>,
) -> Result<(), MomentError> {
    if &admit_moment(request, history, session)? != context {
        return Err(MomentError::SessionMismatch);
    }
    Ok(())
}

#[cfg(test)]
#[path = "moment_tests.rs"]
mod tests;
