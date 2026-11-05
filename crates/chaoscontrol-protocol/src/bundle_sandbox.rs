// r[impl chaoscontrol.bundle_sandbox.policy_admission]
//! Pure admission for optional lowered-bundle sandbox policy.
//!
//! Evaluates two bounded policy surfaces without a container runtime: an explicit
//! uid/gid map that never maps guest root, and a syscall allowlist derived from a
//! workload contract. Admission is lexical; it does not install namespaces or
//! filters, prove isolation, or interact with replay.

/// Maximum uid or gid map entries.
pub const MAX_ID_MAP_ENTRIES: usize = 16;

/// Maximum syscall rules in one policy.
pub const MAX_SECCOMP_RULES: usize = 256;

/// Maximum syscall name length.
pub const MAX_SYSCALL_NAME_BYTES: usize = 64;

/// One uid or gid range mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdMapEntry {
    /// First inside (bundle) id.
    pub inside: u32,
    /// First outside (guest) id.
    pub outside: u32,
    /// Number of mapped ids.
    pub length: u32,
}

/// Action for one syscall rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompAction {
    /// Run the syscall.
    Allow,
    /// Fail the syscall with `EPERM`.
    Errno,
}

impl SeccompAction {
    /// Parse a wire action name.
    pub fn parse(value: &str) -> Result<Self, SandboxPolicyError> {
        match value {
            "allow" => Ok(Self::Allow),
            "errno" => Ok(Self::Errno),
            _ => Err(SandboxPolicyError::UnsupportedAction),
        }
    }
}

/// One bounded syscall rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeccompRule {
    /// Syscall name.
    pub syscall: String,
    /// Rule action.
    pub action: SeccompAction,
}

/// Rejection class for bundle sandbox policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxPolicyError {
    /// The id map has no entries.
    EmptyMap,
    /// The id map exceeds the entry bound.
    TooManyEntries,
    /// An entry has zero length.
    ZeroLength,
    /// An entry range overflows a 32-bit id.
    RangeOverflow,
    /// Inside ranges overlap.
    InsideOverlap,
    /// Outside ranges overlap.
    OutsideOverlap,
    /// An entry maps the guest root id.
    RootMapped,
    /// No entry maps inside id 0.
    ContainerZeroMissing,
    /// A syscall name is empty or contains unsupported characters.
    InvalidSyscallName,
    /// The syscall policy exceeds the rule bound.
    TooManyRules,
    /// Two rules name the same syscall.
    DuplicateSyscall,
    /// The action name is not `allow` or `errno`.
    UnsupportedAction,
}

fn admit_id_map(entries: &[IdMapEntry]) -> Result<(), SandboxPolicyError> {
    if entries.is_empty() {
        return Err(SandboxPolicyError::EmptyMap);
    }
    if entries.len() > MAX_ID_MAP_ENTRIES {
        return Err(SandboxPolicyError::TooManyEntries);
    }
    for entry in entries {
        if entry.length == 0 {
            return Err(SandboxPolicyError::ZeroLength);
        }
        if entry.inside.checked_add(entry.length).is_none()
            || entry.outside.checked_add(entry.length).is_none()
        {
            return Err(SandboxPolicyError::RangeOverflow);
        }
        if entry.outside == 0 {
            return Err(SandboxPolicyError::RootMapped);
        }
    }
    let maps_zero = entries.iter().any(|entry| entry.inside == 0);
    if !maps_zero {
        return Err(SandboxPolicyError::ContainerZeroMissing);
    }
    for (index, left) in entries.iter().enumerate() {
        for right in &entries[index + 1..] {
            if left.inside < right.inside.saturating_add(right.length)
                && right.inside < left.inside.saturating_add(left.length)
            {
                return Err(SandboxPolicyError::InsideOverlap);
            }
            if left.outside < right.outside.saturating_add(right.length)
                && right.outside < left.outside.saturating_add(left.length)
            {
                return Err(SandboxPolicyError::OutsideOverlap);
            }
        }
    }
    Ok(())
}

fn admit_syscall_name(name: &str) -> Result<(), SandboxPolicyError> {
    if name.is_empty()
        || name.len() > MAX_SYSCALL_NAME_BYTES
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(SandboxPolicyError::InvalidSyscallName);
    }
    Ok(())
}

/// Admit an explicit uid and gid map.
pub fn admit_id_maps(uid: &[IdMapEntry], gid: &[IdMapEntry]) -> Result<(), SandboxPolicyError> {
    admit_id_map(uid)?;
    admit_id_map(gid)
}

/// Admit a syscall allowlist with wire action names.
pub fn admit_seccomp(rules: &[(&str, &str)]) -> Result<Vec<SeccompRule>, SandboxPolicyError> {
    if rules.len() > MAX_SECCOMP_RULES {
        return Err(SandboxPolicyError::TooManyRules);
    }
    let mut admitted = Vec::with_capacity(rules.len());
    for (syscall, action) in rules {
        admit_syscall_name(syscall)?;
        if admitted
            .iter()
            .any(|rule: &SeccompRule| rule.syscall == *syscall)
        {
            return Err(SandboxPolicyError::DuplicateSyscall);
        }
        admitted.push(SeccompRule {
            syscall: (*syscall).to_string(),
            action: SeccompAction::parse(action)?,
        });
    }
    Ok(admitted)
}

/// Admit the combined optional policy.
pub fn admit_bundle_sandbox(
    uid: &[IdMapEntry],
    gid: &[IdMapEntry],
    rules: &[(&str, &str)],
) -> Result<Vec<SeccompRule>, SandboxPolicyError> {
    admit_id_maps(uid, gid)?;
    admit_seccomp(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ([IdMapEntry; 2], [IdMapEntry; 2]) {
        (
            [
                IdMapEntry {
                    inside: 0,
                    outside: 100_000,
                    length: 65_536,
                },
                IdMapEntry {
                    inside: 65_536,
                    outside: 200_000,
                    length: 1,
                },
            ],
            [
                IdMapEntry {
                    inside: 0,
                    outside: 100_000,
                    length: 65_536,
                },
                IdMapEntry {
                    inside: 65_536,
                    outside: 200_000,
                    length: 1,
                },
            ],
        )
    }

    #[test]
    fn explicit_maps_and_syscalls_are_admitted() {
        let (uid, gid) = map();
        let rules =
            admit_bundle_sandbox(&uid, &gid, &[("read", "allow"), ("ioctl", "errno")]).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].action, SeccompAction::Allow);
        assert_eq!(rules[1].action, SeccompAction::Errno);
    }

    #[test]
    fn root_and_missing_container_zero_are_rejected() {
        let (mut uid, gid) = map();
        uid[0].outside = 0;
        assert_eq!(
            admit_id_maps(&uid, &gid),
            Err(SandboxPolicyError::RootMapped)
        );
        let (mut uid, gid) = map();
        uid[0].inside = 1;
        assert_eq!(
            admit_id_maps(&uid, &gid),
            Err(SandboxPolicyError::ContainerZeroMissing)
        );
    }

    #[test]
    fn id_map_bounds_and_overlaps_are_rejected() {
        let (mut uid, gid) = map();
        uid[0].length = 0;
        assert_eq!(
            admit_id_maps(&uid, &gid),
            Err(SandboxPolicyError::ZeroLength)
        );
        let (mut uid, gid) = map();
        uid[0].outside = u32::MAX;
        assert_eq!(
            admit_id_maps(&uid, &gid),
            Err(SandboxPolicyError::RangeOverflow)
        );
        let (mut uid, gid) = map();
        uid[1].inside = 10;
        assert_eq!(
            admit_id_maps(&uid, &gid),
            Err(SandboxPolicyError::InsideOverlap)
        );
        let (mut uid, gid) = map();
        uid[1].outside = 100_010;
        assert_eq!(
            admit_id_maps(&uid, &gid),
            Err(SandboxPolicyError::OutsideOverlap)
        );
        let too_many = [IdMapEntry {
            inside: 0,
            outside: 100_000,
            length: 1,
        }; MAX_ID_MAP_ENTRIES + 1];
        assert_eq!(
            admit_id_maps(&too_many, &too_many),
            Err(SandboxPolicyError::TooManyEntries)
        );
    }

    #[test]
    fn syscall_policy_rejections_are_typed() {
        assert_eq!(
            admit_seccomp(&[("", "allow")]),
            Err(SandboxPolicyError::InvalidSyscallName)
        );
        assert_eq!(
            admit_seccomp(&[("openat", "allow"), ("openat", "errno")]),
            Err(SandboxPolicyError::DuplicateSyscall)
        );
        assert_eq!(
            admit_seccomp(&[("openat", "kill")]),
            Err(SandboxPolicyError::UnsupportedAction)
        );
        assert_eq!(
            admit_seccomp(&[("OpenAt", "allow")]),
            Err(SandboxPolicyError::InvalidSyscallName)
        );
    }
}
