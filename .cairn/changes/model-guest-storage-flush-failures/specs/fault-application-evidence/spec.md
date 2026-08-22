# Fault Application Evidence Delta

## ADDED Requirements

### Requirement: Guest-visible virtio flush
r[chaoscontrol.storage_faults.virtio_flush] ChaosControl MUST model valid `VIRTIO_BLK_T_FLUSH` requests as first-class deterministic block operations when the device advertises flush support.

#### Scenario: Valid guest flush completes
- GIVEN a negotiated flush feature and a valid no-data descriptor chain
- WHEN the guest submits `VIRTIO_BLK_T_FLUSH`
- THEN the device executes one identified flush attempt and returns one status result

#### Scenario: Flush feature is absent
- GIVEN a device without flush support
- WHEN the guest submits a flush request
- THEN request admission fails without changing disk state

### Requirement: Flush fault outcomes
r[chaoscontrol.storage_faults.flush_faults] ChaosControl MUST support deterministic flush success, immediate error, delayed error, and acknowledged-without-durability outcomes.

#### Scenario: Flush returns an immediate error
- GIVEN an immediate-error fault bound to the next flush attempt
- WHEN the guest submits that flush
- THEN the request returns an error and the receipt identifies the exact attempt

#### Scenario: Device lies about flush
- GIVEN an acknowledged-without-durability fault
- WHEN the guest flushes and then crashes
- THEN volatile writes can be discarded despite the successful guest status

### Requirement: Cache-aware recovery modes
r[chaoscontrol.storage_faults.cache_recovery_modes] ChaosControl MUST distinguish application restart with retained cache, cache eviction, and full guest reboot.

#### Scenario: Application restarts with cache retained
- GIVEN a failed writeback left cache bytes that differ from durable media
- WHEN only the application restarts
- THEN the workload can observe the retained cache bytes separately from durable bytes

#### Scenario: Failed page is rewritten
- GIVEN a prior failed clean page
- WHEN the guest issues a new write and a successful flush
- THEN the new write can become durable under the selected device model

### Requirement: Storage-fault model validation
r[chaoscontrol.storage_faults.validation] The block and virtio suites MUST include positive and negative flush, descriptor, feature, retry, cache, corruption, partial-write, and capacity cases.

#### Scenario: Malformed flush chain runs
- GIVEN a flush request with a data descriptor or invalid status buffer
- WHEN validation runs
- THEN the request fails without disk mutation
