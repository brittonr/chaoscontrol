# Typed review boundary for ChaosControl's VM Cohort consumer pilot.
let P = import "../evidence/profile-primitives.ncl" in
let Revision = "ab123e3673b6dd616b3df5d044026b5e85755149" in
let Status = P.closed_enum ["passed"] in
let Shape = {
  schema_version | P.exact_value 1,
  source | {
    rid | P.exact_string "rad:z2QJLUqyAZnnHPiZQ1BFjLsX9ush3",
    revision | P.exact_string Revision,
    cargo_url | P.exact_string "rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3",
    nix_url | P.exact_string "git+rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3?rev=ab123e3673b6dd616b3df5d044026b5e85755149",
    cargo_lock_matches | P.exact_value true,
    nix_input_matches | P.exact_value true,
    moving_ref | P.exact_value false,
    sibling_path_fallback | P.exact_value false,
  },
  mapping | {
    exact_snapshot | P.exact_value true,
    initialized_memory | P.exact_value true,
    initialized_disk | P.exact_value true,
    topology | P.exact_value true,
    runtime | P.exact_value true,
    adapter | P.exact_value true,
  },
  verification | {
    source_agreement | Status,
    exact_mapping | Status,
    exact_restore | Status,
    parity | Status,
    partial_creation_cleanup | Status,
    cleanup_uncertainty | Status,
    kvm_smoke | Status,
    focused_clippy | Status,
    focused_octet | Status,
  },
  selection | {
    mechanism | P.exact_string "vm-cohort",
    legacy_status | P.exact_string "diagnostic-rollback-only",
    automatic_fallback | P.exact_value false,
  },
  authority | {
    shared_mechanics_owner | P.exact_string "vm-cohort",
    product_policy_owner | P.exact_string "chaoscontrol",
    consumer_policy_leak | P.exact_value false,
    fault_authority_granted | P.exact_value false,
    replay_authority_granted | P.exact_value false,
    evidence_authority_granted | P.exact_value false,
    release_authority_granted | P.exact_value false,
  },
  non_claims | Array P.non_empty_string,
} in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked =>
      let claims = std.string.join " " checked.non_claims in
      let complete = std.array.length checked.non_claims >= 5
        && std.string.is_match "correct" claims
        && std.string.is_match "determinism" claims
        && std.string.is_match "authority" claims
        && std.string.is_match "erasure" claims
        && std.string.is_match "release fallback" claims in
      if complete then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
