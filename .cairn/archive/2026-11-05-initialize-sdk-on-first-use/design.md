## Decision

Reuse the existing `internal::init` OnceLock rather than adding another flag or reentrancy mechanism. Public lifecycle operations initialize before serializing/outputting application data. Entropy-consuming random APIs initialize with `full`; empty/singleton fast paths remain unchanged. Direct guidance hits initialize before identity resolution; initialization-time `hit = false` records bypass that call.

Extract the existing lifecycle event dispatch into a crate-private `send_event_uninitialized` writer. Public `send_event` initializes then forwards; initialization-time SDK metadata, raw must-hit metadata and bound guidance records use the private writer. Numeric/boolean/compatibility guidance calls initialize for hits, and stable JSON guidance initializes at its entry point. There is no public bypass API and no signature change.

## Verification

Exercise first operations in fresh child processes so global OnceLocks and environment configuration are isolated. Cover setup, named events, get_random, index choice and direct stable/compatibility guidance. Include an unexecuted numeric assertion so catalog emission exercises its guidance declaration without recursion. Assert first metadata, one completed catalog, declarations with no fabricated assertion hits, event/guidance data, and no duplication after repeated or explicit initialization. Include simultaneous event callers and entropy-free fast paths. Bound child execution with a deadline so recursion cannot hang the suite.

Run an actual local-output example as well as SDK default/all-feature tests, no-full shape checks, workspace check, SDK Clippy, formatting and evidence contracts. Keep this change active; record unrelated lifecycle failures without changing them.
