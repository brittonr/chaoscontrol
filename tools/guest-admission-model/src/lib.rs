//! Pure evaluation model for bounded guest-process admission.
//!
//! Mirrors the Capsem mechanisms reviewed in `docs/guest-seccomp-evaluation.md`:
//! a workload user-namespace map that never maps container root to guest root,
//! and a syscall policy resolved from a small vendored allowlist. This crate is a
//! detached model with tests; it is not wired into intake and does not apply a
//! filter. The product-scope decision and replay configuration binding remain
//! prerequisites.

/// One OCI-style id map, using the same spelling as the evaluated contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdMap {
    pub container_id: u32,
    pub host_id: u32,
    pub size: u32,
}

pub const MIN_HOST_ID: u32 = 65_536;
pub const MAX_MAP_SIZE: u32 = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    /// The map must start at container id 0 so the documented rule applies.
    NonRootContainerStart {
        container_id: u32,
    },
    /// Container root must not translate to guest root.
    RootMapsToGuestRoot,
    /// A map at or below the reserved guest range is rejected.
    HostIdBelowFloor {
        host_id: u32,
    },
    ZeroSize,
    SizeAboveMaximum {
        size: u32,
    },
    HostRangeOverflow {
        host_id: u32,
        size: u32,
    },
    UnsupportedPolicy {
        policy: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyId {
    MobyDefaultV1,
}

impl PolicyId {
    pub fn parse(value: &str) -> Result<Self, AdmissionError> {
        match value {
            "moby-default-v1" => Ok(Self::MobyDefaultV1),
            other => Err(AdmissionError::UnsupportedPolicy {
                policy: other.to_string(),
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MobyDefaultV1 => "moby-default-v1",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuestAdmission {
    pub uid_map: Option<IdMap>,
    pub gid_map: Option<IdMap>,
    pub syscall_policy: Option<PolicyId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionDecision {
    Admitted {
        uid_map: Option<IdMap>,
        gid_map: Option<IdMap>,
        syscall_policy: Option<PolicyId>,
    },
    Rejected(Vec<AdmissionError>),
}

pub fn validate_id_map(map: IdMap) -> Result<IdMap, AdmissionError> {
    if map.container_id != 0 {
        return Err(AdmissionError::NonRootContainerStart {
            container_id: map.container_id,
        });
    }
    if map.host_id == 0 {
        return Err(AdmissionError::RootMapsToGuestRoot);
    }
    if map.host_id < MIN_HOST_ID {
        return Err(AdmissionError::HostIdBelowFloor {
            host_id: map.host_id,
        });
    }
    if map.size == 0 {
        return Err(AdmissionError::ZeroSize);
    }
    if map.size > MAX_MAP_SIZE {
        return Err(AdmissionError::SizeAboveMaximum { size: map.size });
    }
    if map.host_id.checked_add(map.size).is_none() {
        return Err(AdmissionError::HostRangeOverflow {
            host_id: map.host_id,
            size: map.size,
        });
    }
    Ok(map)
}

/// Validate a declaration without applying it. All errors are collected so an
/// operator sees every problem at once; any error rejects the declaration.
pub fn admit(declaration: &GuestAdmission) -> AdmissionDecision {
    let mut errors = Vec::new();
    let uid_map = declaration
        .uid_map
        .and_then(|map| match validate_id_map(map) {
            Ok(map) => Some(map),
            Err(error) => {
                errors.push(error);
                None
            }
        });
    let gid_map = declaration
        .gid_map
        .and_then(|map| match validate_id_map(map) {
            Ok(map) => Some(map),
            Err(error) => {
                errors.push(error);
                None
            }
        });
    if !errors.is_empty() {
        return AdmissionDecision::Rejected(errors);
    }
    AdmissionDecision::Admitted {
        uid_map,
        gid_map,
        syscall_policy: declaration.syscall_policy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(host_id: u32, size: u32) -> IdMap {
        IdMap {
            container_id: 0,
            host_id,
            size,
        }
    }

    #[test]
    fn root_safe_map_is_admitted() {
        let declaration = GuestAdmission {
            uid_map: Some(map(100_000, 65_536)),
            gid_map: Some(map(100_000, 65_536)),
            syscall_policy: Some(PolicyId::MobyDefaultV1),
        };
        assert!(matches!(
            admit(&declaration),
            AdmissionDecision::Admitted { .. }
        ));
    }

    #[test]
    fn empty_declaration_is_admitted_but_enforces_nothing() {
        assert_eq!(
            admit(&GuestAdmission::default()),
            AdmissionDecision::Admitted {
                uid_map: None,
                gid_map: None,
                syscall_policy: None
            }
        );
    }

    #[test]
    fn guest_root_mapping_is_rejected() {
        assert_eq!(
            validate_id_map(map(0, 65_536)),
            Err(AdmissionError::RootMapsToGuestRoot)
        );
        assert_eq!(
            validate_id_map(map(1_000, 65_536)),
            Err(AdmissionError::HostIdBelowFloor { host_id: 1_000 })
        );
    }

    #[test]
    fn container_start_and_bounds_are_enforced() {
        let mut non_root = map(100_000, 1);
        non_root.container_id = 1;
        assert!(matches!(
            validate_id_map(non_root),
            Err(AdmissionError::NonRootContainerStart { .. })
        ));
        assert_eq!(
            validate_id_map(map(100_000, 0)),
            Err(AdmissionError::ZeroSize)
        );
        assert!(matches!(
            validate_id_map(map(100_000, MAX_MAP_SIZE + 1)),
            Err(AdmissionError::SizeAboveMaximum { .. })
        ));
        assert!(matches!(
            validate_id_map(map(u32::MAX - 10, 65_536)),
            Err(AdmissionError::HostRangeOverflow { .. })
        ));
        assert!(validate_id_map(map(100_000, MAX_MAP_SIZE)).is_ok());
    }

    #[test]
    fn policy_ids_resolve_from_the_vendored_allowlist() {
        assert_eq!(
            PolicyId::parse("moby-default-v1"),
            Ok(PolicyId::MobyDefaultV1)
        );
        assert_eq!(PolicyId::MobyDefaultV1.as_str(), "moby-default-v1");
        assert_eq!(
            PolicyId::parse("hand-written-allowlist"),
            Err(AdmissionError::UnsupportedPolicy {
                policy: "hand-written-allowlist".to_string()
            })
        );
    }

    #[test]
    fn all_map_errors_are_collected() {
        let declaration = GuestAdmission {
            uid_map: Some(map(0, 0)),
            gid_map: Some(map(1_000, 65_536)),
            syscall_policy: None,
        };
        match admit(&declaration) {
            AdmissionDecision::Rejected(errors) => {
                assert_eq!(errors.len(), 2, "{errors:?}");
                assert_eq!(errors[0], AdmissionError::RootMapsToGuestRoot);
                assert_eq!(
                    errors[1],
                    AdmissionError::HostIdBelowFloor { host_id: 1_000 }
                );
            }
            admitted => panic!("unexpected decision: {admitted:?}"),
        }
    }
}
