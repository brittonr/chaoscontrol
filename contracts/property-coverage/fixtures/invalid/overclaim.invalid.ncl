let Profile = import "../../profile.ncl" in
{
  schema_version = 1,
  id = "invalid-overclaim",
  target_models = ["scheduler-snapshot"],
  seed_policy = "named-explicit",
  seeds = [1],
  sequence_count = 1,
  max_steps = 1,
  max_shrink_attempts = 1,
  max_retained_counterexamples = 1,
  max_runtime_seconds = 1,
  claim = "This is a formal proof of complete correctness and absence of defects.",
  non_claims = [
    "This profile is not a formal proof.",
    "This profile does not prove whole-system correctness.",
    "This profile does not prove exhaustive state coverage.",
    "This profile does not prove KVM behavior.",
    "This profile does not prove the absence of defects.",
  ],
} | Profile
