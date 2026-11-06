# exact-local-v1. Export with:
# nickel export contracts/persisted-debugger/exact-local-v1.ncl --format json --output contracts/persisted-debugger/generated/exact-local-v1.json
# Existing session/store byte and checkpoint ceilings are preserved. Trace and
# replay-tick caps reuse the 65,536-record schedule journal ceiling. Output reuses
# the recording-byte ceiling; 256 artifact slots bound the 128-checkpoint table
# plus guest inputs, recording, manifest and finding provenance. The 600-second
# host deadline is operational only, not deterministic guest configuration.
let Limits = import "session-limits.ncl" in
{
  max_manifest_bytes = 4194304,
  max_recording_bytes = 16777216,
  max_artifact_bytes = 536870912,
  max_snapshot_bytes = 268435456,
  max_total_bytes = 4294967296,
  max_checkpoints = 128,
  max_branches = 128,
  max_artifacts = 256,
  max_decoded_snapshot_bytes = 2147483648,
  max_trace_records_per_vm = 65536,
  max_output_bytes = 16777216,
  max_replay_ticks = 65536,
  host_deadline_ms = 600000,
} | Limits
