## ADDED Requirements

### Requirement: Stable assertions accept borrowed serializable details

r[chaoscontrol.stable_assertion_details.serialize]

With `full`, stable assertions MUST accept borrowed `serde::Serialize + ?Sized` details, including typed structs, unsized strings and slices, without an intermediate JSON-value conversion. Accepted assertion identity, outcomes, details bounds and existing JSON-value callers MUST remain unchanged. Identity resolution and sparse sampling MUST occur before serialization. Without `full`, stable calls MUST retain generic borrowing and argument evaluation without requiring serialization or emitting output.

#### Scenario: Typed stable observation

- GIVEN a declared stable assertion and borrowed struct, string or slice details
- WHEN its accepted outcome emits
- THEN the local hit preserves the serialized payload, condition and accepted descriptor identity

#### Scenario: Sparse or unresolved observation

- GIVEN a repeated sampled outcome or an unknown stable identity
- WHEN the emitter is invoked with a custom serializer
- THEN it does not invoke that serializer or produce an assertion hit

#### Scenario: Disabled instrumentation

- GIVEN a no-std consumer using borrowed details without serialization traits
- WHEN the same stable callsites execute without full
- THEN condition/details expressions still evaluate once and no local output is created
