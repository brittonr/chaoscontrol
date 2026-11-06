# Review-only finite limits. Exact VM configuration is never supplied here.
let P = import "../evidence/profile-primitives.ncl" in
{
  max_manifest_bytes | P.integer_between 1 4194304,
  max_recording_bytes | P.integer_between 1 16777216,
  max_artifact_bytes | P.integer_between 1 536870912,
  max_snapshot_bytes | P.integer_between 1 268435456,
  max_total_bytes | P.integer_between 1 4294967296,
  max_checkpoints | P.integer_between 1 128,
  max_branches | P.integer_between 1 128,
  max_artifacts | P.integer_between 1 256,
  max_decoded_snapshot_bytes | P.integer_between 1 2147483648,
  max_trace_records_per_vm | P.integer_between 1 65536,
  max_output_bytes | P.integer_between 1 16777216,
  max_replay_ticks | P.integer_between 1 65536,
  host_deadline_ms | P.integer_between 1 600000,
}
