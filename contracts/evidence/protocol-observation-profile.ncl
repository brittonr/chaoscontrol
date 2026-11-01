# Review boundary only; observations and drain traces remain Rust-owned.
let P = import "profile-primitives.ncl" in
let Participant = {
  participant_ref | P.blake3,
  producer_ref | P.blake3,
  guest_id | P.u32,
  generation | P.u64,
} in
let Bounds = {
  records | P.bounded_positive 1000000,
  record_bytes | P.bounded_positive 16777216,
  total_bytes | P.bounded_positive 16777216,
  boundaries | P.bounded_positive 1000000,
  cohort_backlog | P.bounded_positive 1000000,
  oracle_work | P.bounded_positive 1000000,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.protocol-observation-profile.v1",
  protocol_ref | P.blake3,
  projection_schema_ref | P.blake3,
  boundary_schema_ref | P.blake3,
  oracle_adapter_ref | P.blake3,
  independent_oracle_ref | P.blake3,
  novelty_selection_ref | P.blake3,
  participants | Array Participant,
  bounds | Bounds,
  marker_policy | P.exact_string "disabled",
  non_claims | Array String,
}

in
let Coherent = fun value =>
  std.array.length value.participants > 0
  && std.array.length value.participants <= 4096
  && P.unique_by (fun p => p.participant_ref) value.participants
  && P.unique_by (fun p => p.producer_ref) value.participants
  && value.oracle_adapter_ref != value.independent_oracle_ref
  && value.bounds.record_bytes <= value.bounds.total_bytes
  && value.bounds.boundaries <= value.bounds.records
  && value.bounds.cohort_backlog <= value.bounds.boundaries
  && std.array.all (fun claim => std.array.any (fun found => found == claim) value.non_claims)
    ["universal-protocol-correctness", "production-readiness", "release-eligibility",
      "snapshot-backed-replay", "cross-participant-total-order"]
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)