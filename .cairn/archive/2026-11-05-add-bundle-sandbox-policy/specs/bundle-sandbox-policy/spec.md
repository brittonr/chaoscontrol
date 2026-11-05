# bundle-sandbox-policy Specification

## ADDED Requirements

### Requirement: Bundle sandbox policy is admitted without a container runtime

r[chaoscontrol.bundle_sandbox.policy_admission]

Admission MUST accept a bounded explicit uid/gid map and syscall rule list only when
maps are non-empty and within entry limits, every range is non-zero and non-
overflowing, inside and outside ranges do not overlap, no entry maps guest root, an
entry maps inside id 0, and syscall names are bounded lowercase identifiers with no
duplicates and supported actions. Rejection MUST be typed. Admission MUST NOT install
namespaces or filters, affect replay, or claim isolation.

#### Scenario: Explicit map and allowlist admitted
- GIVEN non-overlapping maps from container 0 to a non-root guest id and a bounded syscall list
- WHEN policy is admitted
- THEN the rules are returned with parsed actions

#### Scenario: Unsafe map rejected
- GIVEN a map that includes guest root or omits inside id 0
- WHEN policy is admitted
- THEN the matching typed rejection is returned

#### Scenario: Invalid rules rejected
- GIVEN an empty, duplicate, or unsupported syscall rule
- WHEN policy is admitted
- THEN the matching typed rejection is returned

### Requirement: Admitted sandbox policy carries a deterministic identity

r[chaoscontrol.bundle_sandbox.policy_identity]

An admitted sandbox policy MUST produce a BLAKE3 identity over a canonical,
length-delimited encoding of the map kinds, every range, and every rule name and
action. Identical policies MUST produce the same identity; changing any admitted
field MUST change it. Unadmitted policies MUST be rejected. The identity can be bound
into run or replay profiles; it does not install policy, prove enforcement, or make a
run replayable.

#### Scenario: Stable identity
- GIVEN the same admitted maps and rules
- WHEN the identity is computed twice
- THEN both identities match

#### Scenario: Field change changes identity
- GIVEN a changed range or action
- WHEN the identity is computed
- THEN the identity differs
