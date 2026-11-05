## ADDED Requirements

### Requirement: Instrumentation first use initializes the SDK

r[chaoscontrol.sdk_first_use.initialization] With `full` enabled, public setup/event output, entropy-consuming random operations and direct guidance hits MUST complete SDK initialization before their observations. Metadata and the accepted catalog MUST be emitted even if no assertion body executes. Initialization-time events MUST NOT recursively initialize the SDK.

#### Scenario: Lifecycle, randomness or guidance is the first operation

- GIVEN a linked catalog containing an unexecuted assertion and no explicit initialization
- WHEN a workload sends setup/event data, requests randomness or emits direct guidance
- THEN metadata and the completed catalog precede the observation and the unexecuted assertion has a declaration, not a fabricated hit.

### Requirement: First-use initialization is idempotent

r[chaoscontrol.sdk_first_use.idempotence] Repeated, concurrent and later explicit initialization MUST NOT duplicate metadata or catalog records. Entropy-free choice/buffer fast paths and no-full no-op behavior MUST remain unchanged.

#### Scenario: Several callers begin together

- GIVEN an uninitialized SDK
- WHEN concurrent event callers race and explicit initialization follows
- THEN one metadata/catalog sequence precedes the application events and all application payloads are retained.

#### Scenario: No entropy or output is requested

- GIVEN an uninitialized SDK
- WHEN empty/singleton choice or empty byte filling is used, or the SDK is built without `full`
- THEN these operations do not create initialization output.
