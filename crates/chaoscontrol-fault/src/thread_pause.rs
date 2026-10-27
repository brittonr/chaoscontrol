//! Pure cooperative thread-pause admission and release semantics.
//!
//! The guest runtime owns thread identity and safe points. The VMM owns the
//! actual pause gate. This module owns the bounded plan, release tick, stage
//! vocabulary, and admission rules.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const THREAD_PAUSE_ABI_SCHEMA: &str = "chaoscontrol.thread-pause-abi.v1";
pub const MAX_SAFE_POINTS: u32 = 1_000_000;
pub const MAX_ACTIVE_PAUSED_THREADS: usize = 256;
pub const MAX_PAUSE_EVENTS: u64 = 4_096;
pub const MAX_PAUSE_TICKS: u64 = 1_000_000;
pub const MAX_TOTAL_PAUSED_TICKS: u64 = 10_000_000;

/// Versioned cooperative runtime safe-point ABI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafePointAbi {
    pub schema: String,
    pub version: u32,
    pub max_safe_points: u32,
}

/// Stable logical thread identity owned by the guest runtime.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogicalThreadId {
    pub process: String,
    pub generation: u32,
    pub ordinal: u32,
}

/// Finite virtual-time pause window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadPauseWindow {
    pub start_tick: u64,
    pub end_tick: u64,
}

impl ThreadPauseWindow {
    pub fn duration(self) -> u64 {
        self.end_tick.saturating_sub(self.start_tick)
    }
}

/// One admitted thread-pause plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadPausePlan {
    pub target: LogicalThreadId,
    pub window: ThreadPauseWindow,
}

/// Plan and evidence bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadPauseLimits {
    pub max_active_threads: usize,
    pub max_pause_events: u64,
    pub max_pause_ticks: u64,
    pub max_total_paused_ticks: u64,
}

impl Default for ThreadPauseLimits {
    fn default() -> Self {
        Self {
            max_active_threads: MAX_ACTIVE_PAUSED_THREADS,
            max_pause_events: MAX_PAUSE_EVENTS,
            max_pause_ticks: MAX_PAUSE_TICKS,
            max_total_paused_ticks: MAX_TOTAL_PAUSED_TICKS,
        }
    }
}

/// Current run facts used for admission.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadPauseFacts {
    /// Active thread to its exact release tick.
    pub active: BTreeMap<LogicalThreadId, u64>,
    pub pause_events: u64,
    pub total_paused_ticks: u64,
}

/// Stage vocabulary for thread-pause evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadPauseStage {
    Selected,
    Applicable,
    Applied,
    Observed,
    Released,
    Rejected,
    ApplicationFailed,
}

/// Thread-pause admission failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadPauseError {
    UnsupportedAbi,
    InvalidThread,
    InvalidWindow,
    PauseTooLong,
    TooManyActiveThreads,
    DuplicateActiveThread,
    TooManyEvents,
    TotalPauseBound,
    CounterOverflow,
}

impl core::fmt::Display for ThreadPauseError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::UnsupportedAbi => "thread-pause ABI is not the admitted runtime cohort",
            Self::InvalidThread => "logical thread identity is invalid",
            Self::InvalidWindow => "thread-pause window is empty or reversed",
            Self::PauseTooLong => "thread-pause window exceeds the per-pause bound",
            Self::TooManyActiveThreads => "active paused-thread count exceeds the bound",
            Self::DuplicateActiveThread => "thread already has an active pause window",
            Self::TooManyEvents => "thread-pause event count exceeds the bound",
            Self::TotalPauseBound => "total paused-thread tick bound would be exceeded",
            Self::CounterOverflow => "thread-pause counter overflow",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ThreadPauseError {}

/// Admit one logical thread identity.
pub fn admit_thread(thread: &LogicalThreadId) -> Result<(), ThreadPauseError> {
    if !chaoscontrol_protocol::process::validate_process_token(&thread.process) {
        return Err(ThreadPauseError::InvalidThread);
    }
    Ok(())
}

/// Admit the versioned safe-point ABI.
pub fn admit_abi(abi: &SafePointAbi) -> Result<(), ThreadPauseError> {
    if abi.schema != THREAD_PAUSE_ABI_SCHEMA
        || abi.version != 1
        || abi.max_safe_points == 0
        || abi.max_safe_points > MAX_SAFE_POINTS
    {
        return Err(ThreadPauseError::UnsupportedAbi);
    }
    Ok(())
}

/// Admit one bounded thread-pause plan.
pub fn admit_thread_pause(
    target: LogicalThreadId,
    window: ThreadPauseWindow,
    abi: &SafePointAbi,
    limits: &ThreadPauseLimits,
    facts: &ThreadPauseFacts,
) -> Result<ThreadPausePlan, ThreadPauseError> {
    admit_abi(abi)?;
    admit_thread(&target)?;
    if window.start_tick >= window.end_tick {
        return Err(ThreadPauseError::InvalidWindow);
    }
    let duration = window.duration();
    if duration > limits.max_pause_ticks {
        return Err(ThreadPauseError::PauseTooLong);
    }
    if facts.active.len() >= limits.max_active_threads {
        return Err(ThreadPauseError::TooManyActiveThreads);
    }
    if facts.active.contains_key(&target) {
        return Err(ThreadPauseError::DuplicateActiveThread);
    }
    if facts.pause_events >= limits.max_pause_events {
        return Err(ThreadPauseError::TooManyEvents);
    }
    let total = facts
        .total_paused_ticks
        .checked_add(duration)
        .ok_or(ThreadPauseError::CounterOverflow)?;
    if total > limits.max_total_paused_ticks {
        return Err(ThreadPauseError::TotalPauseBound);
    }
    Ok(ThreadPausePlan { target, window })
}

/// Exact release tick of an admitted plan.
pub fn release_tick(plan: &ThreadPausePlan) -> u64 {
    plan.window.end_tick
}

/// Whether a plan is still paused at a tick.
pub fn is_paused_at(plan: &ThreadPausePlan, tick: u64) -> bool {
    (plan.window.start_tick..plan.window.end_tick).contains(&tick)
}

/// Classify a pause attempt that reached the plan but no safe point.
pub const fn unobserved_stage() -> ThreadPauseStage {
    ThreadPauseStage::Applied
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abi() -> SafePointAbi {
        SafePointAbi {
            schema: THREAD_PAUSE_ABI_SCHEMA.to_string(),
            version: 1,
            max_safe_points: 1_000,
        }
    }

    fn thread(ordinal: u32) -> LogicalThreadId {
        LogicalThreadId {
            process: "process:v1:writer".to_string(),
            generation: 1,
            ordinal,
        }
    }

    fn window() -> ThreadPauseWindow {
        ThreadPauseWindow {
            start_tick: 10,
            end_tick: 20,
        }
    }

    #[test]
    fn admits_exact_window_and_release_tick() {
        let plan = admit_thread_pause(
            thread(1),
            window(),
            &abi(),
            &ThreadPauseLimits::default(),
            &ThreadPauseFacts::default(),
        )
        .unwrap();
        assert_eq!(release_tick(&plan), 20);
        assert!(is_paused_at(&plan, 10));
        assert!(is_paused_at(&plan, 19));
        assert!(!is_paused_at(&plan, 20));
    }

    #[test]
    fn rejects_unsupported_abi_and_thread_identity() {
        let mut unsupported = abi();
        unsupported.version = 2;
        assert_eq!(
            admit_thread_pause(
                thread(1),
                window(),
                &unsupported,
                &ThreadPauseLimits::default(),
                &ThreadPauseFacts::default()
            )
            .unwrap_err(),
            ThreadPauseError::UnsupportedAbi
        );
        let invalid = LogicalThreadId {
            process: String::new(),
            generation: 1,
            ordinal: 1,
        };
        assert_eq!(
            admit_thread(&invalid).unwrap_err(),
            ThreadPauseError::InvalidThread
        );
    }

    #[test]
    fn rejects_duplicate_active_and_bounded_windows() {
        let mut facts = ThreadPauseFacts::default();
        facts.active.insert(thread(1), 20);
        assert_eq!(
            admit_thread_pause(
                thread(1),
                window(),
                &abi(),
                &ThreadPauseLimits::default(),
                &facts
            )
            .unwrap_err(),
            ThreadPauseError::DuplicateActiveThread
        );
        let limits = ThreadPauseLimits {
            max_pause_ticks: 5,
            ..ThreadPauseLimits::default()
        };
        assert_eq!(
            admit_thread_pause(
                thread(2),
                window(),
                &abi(),
                &limits,
                &ThreadPauseFacts::default()
            )
            .unwrap_err(),
            ThreadPauseError::PauseTooLong
        );
        let limits = ThreadPauseLimits {
            max_total_paused_ticks: 5,
            ..ThreadPauseLimits::default()
        };
        assert_eq!(
            admit_thread_pause(
                thread(2),
                window(),
                &abi(),
                &limits,
                &ThreadPauseFacts::default()
            )
            .unwrap_err(),
            ThreadPauseError::TotalPauseBound
        );
    }

    #[test]
    fn unobserved_attempt_stays_applied() {
        assert_eq!(unobserved_stage(), ThreadPauseStage::Applied);
    }
}
