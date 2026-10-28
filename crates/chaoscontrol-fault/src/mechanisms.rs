//! Pure admission and ordering for deterministic fault mechanisms.
//!
//! This module owns mechanism meaning only. KVM effects, guest requests, and
//! evidence remain in their existing shells.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_LINK_ID: u32 = 1_000_000;
pub const MAX_THROTTLE_WINDOW_TICKS: u64 = 1_000_000;
pub const MAX_CUSTOM_PARAMETERS: usize = 64;
pub const MAX_CUSTOM_STRING_BYTES: usize = 256;
pub const MAX_PAUSE_WINDOW_TICKS: u64 = 1_000_000;
pub const MAX_ACTIVE_NETWORK_FAULTS: usize = 1_024;
pub const MAX_BASELINE_LATENCY_TICKS: u64 = 1_000_000;
pub const MAX_BASELINE_LOSS_PPM: u32 = 1_000_000;
pub const MAX_PAUSE_REQUESTS: usize = 1_024;

/// Direction a network fault disrupts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkDirection {
    Inbound,
    Outbound,
    Both,
}

/// How a directional network fault disrupts packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkDisruption {
    Stopped,
    Slowed,
    Jammed,
}

impl NetworkDisruption {
    /// More restrictive modes govern overlapping windows.
    pub const fn severity(self) -> u8 {
        match self {
            Self::Slowed => 1,
            Self::Jammed => 2,
            Self::Stopped => 3,
        }
    }
}

/// Canonical unordered link identity between two VM slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkId {
    pub low: u32,
    pub high: u32,
}

impl LinkId {
    pub fn new(left: u32, right: u32) -> Result<Self, MechanismError> {
        if left == right || left >= MAX_LINK_ID || right >= MAX_LINK_ID {
            return Err(MechanismError::InvalidLink);
        }
        Ok(Self {
            low: left.min(right),
            high: left.max(right),
        })
    }

    /// Recheck canonical ordering and bounds for a deserialized identity.
    pub fn validate(&self) -> Result<(), MechanismError> {
        if self.low >= self.high || self.high >= MAX_LINK_ID {
            return Err(MechanismError::InvalidLink);
        }
        Ok(())
    }
}

/// One directional network fault window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectionalNetworkFault {
    pub link: LinkId,
    pub direction: NetworkDirection,
    pub disruption: NetworkDisruption,
    pub start_tick: u64,
    pub end_tick: u64,
    pub delay_ns: u64,
    pub queue_limit: u32,
}

/// Target of a CPU throttle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThrottleTarget {
    Vm(u32),
    Process { vm: u32, process: String },
}

/// A virtual-budget CPU throttle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuThrottle {
    pub target: ThrottleTarget,
    pub budget_units: u64,
    pub start_tick: u64,
    pub end_tick: u64,
}

/// A typed consumer-declared fault effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomFaultEffect {
    pub effect_id: String,
    pub version: u32,
    pub parameters: BTreeMap<String, String>,
}

/// Stable identity of one admitted Rust-owned custom-effect adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomEffectIdentity {
    pub effect_id: String,
    pub version: u32,
}

/// Bounded context passed to a custom-effect adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomEffectContext {
    pub tick: u64,
}

/// Failure class returned by a custom-effect adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomEffectError {
    UnsupportedIdentity,
    VersionMismatch,
    Rejected,
    Failed,
}

/// A Rust-owned adapter for one admitted custom-effect identity.
///
/// The adapter owns its effect semantics. ChaosControl owns only bounded
/// admission, invocation, and evidence. Implementations must not invoke an
/// ambient shell, dynamic library, network fetch, or package installation.
pub trait CustomEffectAdapter: Send {
    fn identity(&self) -> CustomEffectIdentity;
    fn apply(
        &mut self,
        effect: &CustomFaultEffect,
        context: CustomEffectContext,
    ) -> Result<(), CustomEffectError>;
}

/// Admitted baseline network profile applied to all links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkBaselineProfile {
    pub latency_ticks: u64,
    pub loss_rate_ppm: u32,
}

/// A bounded fault-pause request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseControlRequest {
    pub requester: String,
    pub start_tick: u64,
    pub end_tick: u64,
}

/// Planned mechanism variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MechanismPlan {
    DirectionalNetwork(DirectionalNetworkFault),
    CpuThrottle(CpuThrottle),
    CustomEffect(CustomFaultEffect),
    PauseControl(PauseControlRequest),
}

/// Fault-mechanism admission failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MechanismError {
    InvalidLink,
    InvalidWindow,
    WindowTooLong,
    InvalidDirection,
    InvalidDelay,
    InvalidQueue,
    InvalidRate,
    InvalidThrottleTarget,
    InvalidThrottleBudget,
    InvalidEffectId,
    TooManyParameters,
    InvalidParameter,
    InvalidRequester,
    DuplicatePauseRequest,
    TooManyPauseRequests,
    TooManyNetworkFaults,
}

impl core::fmt::Display for MechanismError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::InvalidLink => "network link is self-referential or out of bounds",
            Self::InvalidWindow => "fault window is empty or reversed",
            Self::WindowTooLong => "fault window exceeds the admitted bound",
            Self::InvalidDirection => "network direction is invalid",
            Self::InvalidDelay => "slowed network fault requires a nonzero delay",
            Self::InvalidQueue => "jammed network fault requires a nonzero queue limit",
            Self::InvalidRate => "network rate exceeds one million parts per million",
            Self::InvalidThrottleTarget => "CPU throttle target is invalid",
            Self::InvalidThrottleBudget => "CPU throttle budget is zero or over-bound",
            Self::InvalidEffectId => "custom effect identity is empty or invalid",
            Self::TooManyParameters => "custom effect parameter count exceeds the bound",
            Self::InvalidParameter => "custom effect parameter is invalid or over-bound",
            Self::InvalidRequester => "pause requester identity is invalid",
            Self::DuplicatePauseRequest => "pause request duplicates an active request",
            Self::TooManyPauseRequests => "pause request count exceeds the bound",
            Self::TooManyNetworkFaults => "active network fault count exceeds the bound",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for MechanismError {}

/// Admit a baseline network profile.
pub fn admit_baseline_profile(profile: &NetworkBaselineProfile) -> Result<(), MechanismError> {
    if profile.latency_ticks > MAX_BASELINE_LATENCY_TICKS {
        return Err(MechanismError::InvalidDelay);
    }
    if profile.loss_rate_ppm > MAX_BASELINE_LOSS_PPM {
        return Err(MechanismError::InvalidRate);
    }
    Ok(())
}

/// Admit a directional network fault.
pub fn admit_directional_network(fault: &DirectionalNetworkFault) -> Result<(), MechanismError> {
    fault.link.validate()?;
    if fault.direction == NetworkDirection::Both {
        // Both is explicit and valid; keep the match for exhaustive review.
    } else if fault.direction == NetworkDirection::Inbound
        || fault.direction == NetworkDirection::Outbound
    {
        // Valid.
    } else {
        return Err(MechanismError::InvalidDirection);
    }
    validate_window(fault.start_tick, fault.end_tick, MAX_PAUSE_WINDOW_TICKS)?;
    match fault.disruption {
        NetworkDisruption::Slowed if fault.delay_ns == 0 => {
            return Err(MechanismError::InvalidDelay);
        }
        NetworkDisruption::Jammed if fault.queue_limit == 0 => {
            return Err(MechanismError::InvalidQueue);
        }
        _ => {}
    }
    Ok(())
}

/// Admit a virtual-budget CPU throttle.
pub fn admit_cpu_throttle(throttle: &CpuThrottle) -> Result<(), MechanismError> {
    validate_window(
        throttle.start_tick,
        throttle.end_tick,
        MAX_THROTTLE_WINDOW_TICKS,
    )?;
    if throttle.budget_units == 0 || throttle.budget_units > MAX_THROTTLE_WINDOW_TICKS {
        return Err(MechanismError::InvalidThrottleBudget);
    }
    match &throttle.target {
        ThrottleTarget::Vm(_) => Ok(()),
        ThrottleTarget::Process { process, .. } => validate_text(process, MAX_CUSTOM_STRING_BYTES)
            .map_err(|_| MechanismError::InvalidThrottleTarget),
    }
}

/// Admit a typed custom effect. No ambient command, shell, or network is allowed.
pub fn admit_custom_effect(effect: &CustomFaultEffect) -> Result<(), MechanismError> {
    validate_text(&effect.effect_id, MAX_CUSTOM_STRING_BYTES)
        .map_err(|_| MechanismError::InvalidEffectId)?;
    if effect.parameters.len() > MAX_CUSTOM_PARAMETERS {
        return Err(MechanismError::TooManyParameters);
    }
    for (name, value) in &effect.parameters {
        validate_text(name, MAX_CUSTOM_STRING_BYTES)?;
        validate_text(value, MAX_CUSTOM_STRING_BYTES)?;
        let lower = value.to_ascii_lowercase();
        if lower.contains("curl ")
            || lower.contains("wget ")
            || lower.contains("sh -c")
            || lower.contains("/bin/sh")
            || lower.contains("/bin/bash")
            || lower.contains("exec ")
            || lower.contains("system(")
        {
            return Err(MechanismError::InvalidParameter);
        }
    }
    Ok(())
}

/// Admit a bounded pause request.
pub fn admit_pause_control(request: &PauseControlRequest) -> Result<(), MechanismError> {
    validate_text(&request.requester, MAX_CUSTOM_STRING_BYTES)
        .map_err(|_| MechanismError::InvalidRequester)?;
    validate_window(request.start_tick, request.end_tick, MAX_PAUSE_WINDOW_TICKS)
}

/// Admit a set of pause requests and reject duplicate requester windows.
pub fn admit_pause_requests(requests: &[PauseControlRequest]) -> Result<(), MechanismError> {
    if requests.len() > MAX_PAUSE_REQUESTS {
        return Err(MechanismError::TooManyPauseRequests);
    }
    for (index, request) in requests.iter().enumerate() {
        admit_pause_control(request)?;
        for previous in &requests[..index] {
            if previous.requester == request.requester
                && previous.start_tick < request.end_tick
                && request.start_tick < previous.end_tick
            {
                return Err(MechanismError::DuplicatePauseRequest);
            }
        }
    }
    Ok(())
}

/// Effective disruption for overlapping network windows on one link and direction.
pub fn effective_network_mode(
    faults: &[DirectionalNetworkFault],
    link: LinkId,
    direction: NetworkDirection,
) -> Option<NetworkDisruption> {
    faults
        .iter()
        .filter(|fault| {
            fault.link == link
                && (fault.direction == direction || fault.direction == NetworkDirection::Both)
        })
        .map(|fault| fault.disruption)
        .max_by_key(|mode| mode.severity())
}

/// Admit a bounded set of directional network faults.
pub fn admit_network_faults(faults: &[DirectionalNetworkFault]) -> Result<(), MechanismError> {
    if faults.len() > MAX_ACTIVE_NETWORK_FAULTS {
        return Err(MechanismError::TooManyNetworkFaults);
    }
    for fault in faults {
        admit_directional_network(fault)?;
    }
    Ok(())
}

fn validate_window(start_tick: u64, end_tick: u64, max: u64) -> Result<(), MechanismError> {
    if start_tick >= end_tick {
        return Err(MechanismError::InvalidWindow);
    }
    if end_tick - start_tick > max {
        return Err(MechanismError::WindowTooLong);
    }
    Ok(())
}

fn validate_text(value: &str, max: usize) -> Result<(), MechanismError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(MechanismError::InvalidParameter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> LinkId {
        LinkId::new(2, 1).unwrap()
    }

    fn network(disruption: NetworkDisruption) -> DirectionalNetworkFault {
        DirectionalNetworkFault {
            link: link(),
            direction: NetworkDirection::Outbound,
            disruption,
            start_tick: 10,
            end_tick: 20,
            delay_ns: u64::from(disruption == NetworkDisruption::Slowed),
            queue_limit: u32::from(disruption == NetworkDisruption::Jammed),
        }
    }

    #[test]
    fn link_identity_is_canonical_and_self_links_fail() {
        let link = LinkId::new(9, 3).unwrap();
        assert_eq!((link.low, link.high), (3, 9));
        assert_eq!(LinkId::new(3, 3).unwrap_err(), MechanismError::InvalidLink);
    }

    #[test]
    fn deserialized_link_identity_is_rechecked_at_admission() {
        let mut fault = network(NetworkDisruption::Stopped);
        fault.link = LinkId { low: 5, high: 5 };
        assert_eq!(
            admit_directional_network(&fault).unwrap_err(),
            MechanismError::InvalidLink
        );
        fault.link = LinkId { low: 9, high: 3 };
        assert_eq!(
            admit_directional_network(&fault).unwrap_err(),
            MechanismError::InvalidLink
        );
        fault.link = LinkId {
            low: 3,
            high: MAX_LINK_ID,
        };
        assert_eq!(
            admit_directional_network(&fault).unwrap_err(),
            MechanismError::InvalidLink
        );
    }

    #[test]
    fn both_direction_faults_apply_to_each_direction() {
        let mut fault = network(NetworkDisruption::Jammed);
        fault.direction = NetworkDirection::Both;
        let faults = vec![fault];
        assert_eq!(
            effective_network_mode(&faults, link(), NetworkDirection::Outbound),
            Some(NetworkDisruption::Jammed)
        );
        assert_eq!(
            effective_network_mode(&faults, link(), NetworkDirection::Inbound),
            Some(NetworkDisruption::Jammed)
        );
    }

    #[test]
    fn directional_modes_require_their_parameters() {
        admit_directional_network(&network(NetworkDisruption::Stopped)).unwrap();
        let mut slowed = network(NetworkDisruption::Slowed);
        slowed.delay_ns = 0;
        assert_eq!(
            admit_directional_network(&slowed).unwrap_err(),
            MechanismError::InvalidDelay
        );
        let mut jammed = network(NetworkDisruption::Jammed);
        jammed.queue_limit = 0;
        assert_eq!(
            admit_directional_network(&jammed).unwrap_err(),
            MechanismError::InvalidQueue
        );
    }

    #[test]
    fn overlapping_network_windows_use_the_most_restrictive_mode() {
        let faults = vec![
            network(NetworkDisruption::Slowed),
            network(NetworkDisruption::Stopped),
            network(NetworkDisruption::Jammed),
        ];
        assert_eq!(
            effective_network_mode(&faults, link(), NetworkDirection::Outbound),
            Some(NetworkDisruption::Stopped)
        );
        assert_eq!(
            effective_network_mode(&faults, link(), NetworkDirection::Inbound),
            None
        );
    }

    #[test]
    fn cpu_throttle_is_virtual_and_bounded() {
        let throttle = CpuThrottle {
            target: ThrottleTarget::Vm(1),
            budget_units: 10,
            start_tick: 5,
            end_tick: 15,
        };
        admit_cpu_throttle(&throttle).unwrap();
        let mut invalid = throttle.clone();
        invalid.budget_units = 0;
        assert_eq!(
            admit_cpu_throttle(&invalid).unwrap_err(),
            MechanismError::InvalidThrottleBudget
        );
        invalid.budget_units = 10;
        invalid.end_tick = invalid.start_tick;
        assert_eq!(
            admit_cpu_throttle(&invalid).unwrap_err(),
            MechanismError::InvalidWindow
        );
    }

    #[test]
    fn custom_effects_reject_ambient_command_strings() {
        let effect = CustomFaultEffect {
            effect_id: "reconfigure".to_string(),
            version: 1,
            parameters: BTreeMap::from([("mode".to_string(), "slow".to_string())]),
        };
        admit_custom_effect(&effect).unwrap();
        let mut unsafe_effect = effect;
        unsafe_effect
            .parameters
            .insert("command".to_string(), "sh -c reboot".to_string());
        assert_eq!(
            admit_custom_effect(&unsafe_effect).unwrap_err(),
            MechanismError::InvalidParameter
        );
    }

    #[test]
    fn pause_requests_reject_overlapping_windows_per_requester() {
        let first = PauseControlRequest {
            requester: "liveness-check".to_string(),
            start_tick: 10,
            end_tick: 20,
        };
        let overlapping = PauseControlRequest {
            requester: "liveness-check".to_string(),
            start_tick: 19,
            end_tick: 30,
        };
        assert_eq!(
            admit_pause_requests(&[first.clone(), overlapping]).unwrap_err(),
            MechanismError::DuplicatePauseRequest
        );
        let adjacent = PauseControlRequest {
            requester: "liveness-check".to_string(),
            start_tick: 20,
            end_tick: 30,
        };
        admit_pause_requests(&[first.clone(), adjacent]).unwrap();
        let other_requester = PauseControlRequest {
            requester: "watchdog".to_string(),
            start_tick: 10,
            end_tick: 20,
        };
        admit_pause_requests(&[first, other_requester]).unwrap();
    }

    #[test]
    fn pause_requests_are_bounded_and_unique() {
        let request = PauseControlRequest {
            requester: "liveness-check".to_string(),
            start_tick: 10,
            end_tick: 20,
        };
        admit_pause_requests(std::slice::from_ref(&request)).unwrap();
        assert_eq!(
            admit_pause_requests(&[request.clone(), request.clone()]).unwrap_err(),
            MechanismError::DuplicatePauseRequest
        );
        let mut reversed = request;
        reversed.start_tick = 20;
        assert_eq!(
            admit_pause_control(&reversed).unwrap_err(),
            MechanismError::InvalidWindow
        );
    }
}
