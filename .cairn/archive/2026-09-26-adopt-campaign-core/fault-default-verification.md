# Explicit absence in fault snapshots

The fault crate now supplies explicit empty values for two legacy fields:

- A missing `process_fault_queue` contains no pending supervisor commands.
- Missing `process_instances` contain no observed process owners.

The values remain empty collections. Null or malformed collections still fail decoding. The repair does not erase malformed data, invent ownership, execute commands, or change restoration authority.

The existing queue replay test now also crosses the JSON snapshot boundary. It retains its duplicate-command denial and exact replay assertions. Four new tests cover missing fields, explicit process owners, and malformed queue or owner data.

The baseline passed 101 fault library tests before these edits. The changed library passed 105 tests, with no ignores. Strict Clippy passed with all targets and all features.

The focused Octet check advanced past the fault errors. It now rejects two external `VmConfig::default()` calls in `crates/chaoscontrol-evidence/src/guest_determinism.rs`. These call sites require a reviewed explicit fixture configuration. A wrapper that hides the same implicit default is not an acceptable repair.

The check still reports broader existing warnings. Neither focused Octet nor full workspace acceptance passes. No suppression, disabled lint, warning budget, test exclusion, or new runtime claim was added.

Logs remain under `campaign/.pi/complete-20260906/`:

- `fault-default-baseline.log`
- `fault-default-tests.log`
- `fault-default-clippy.log`
- `fault-default-octet.log`
