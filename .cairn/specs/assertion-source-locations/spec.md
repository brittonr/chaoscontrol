# Assertion Source Locations Specification

## Purpose

Defines the `assertion-source-locations` capability.

## Requirements

### Requirement: Source metadata follows accepted assertion identity

r[chaoscontrol.assertion_locations.identity] Local bound assertion hits and declarations MUST select source metadata using the accepted descriptor rather than an ambiguous compact alias. Distinct admitted stable sites with colliding aliases and identical kind/message MUST retain independently attributed source fields.

#### Scenario: Accepted stable sites share a compact alias

- GIVEN two admitted stable descriptors in different namespaces with the same alias, kind and message but distinct Rust functions
- WHEN declarations and bound hits are emitted
- THEN each record reports its own function/class/file and descriptor line/column under its accepted fingerprint.

### Requirement: Location correction preserves admission and raw metadata

r[chaoscontrol.assertion_locations.preservation] Location lookup MUST preserve raw source/display/must-hit metadata and the JSON schema. It MUST NOT admit an undeclared stable call or resolve an ambiguous unqualified alias merely to obtain source fields.

#### Scenario: An identity does not resolve

- GIVEN colliding accepted sites
- WHEN an undeclared stable call or ambiguous unqualified assertion call is attempted
- THEN neither creates an accepted hit attributed to an existing site.

#### Scenario: A raw site has explicit metadata

- GIVEN a raw assertion declared before initialization with explicit source, display type and must-hit metadata
- WHEN it emits a hit after initialization
- THEN its existing metadata and verdict fields are retained.
