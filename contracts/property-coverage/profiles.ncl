let Profile = import "profile.ncl" in
let TargetModels = ["scheduler-snapshot", "fault-assertion", "virtio", "evidence"] in
let CommonNonClaims = [
  "This profile is not a formal proof.",
  "This profile does not prove whole-system correctness.",
  "This profile does not prove exhaustive state coverage.",
  "This profile does not prove KVM behavior.",
  "This profile does not prove the absence of defects.",
] in
let profiles = [
  {
    schema_version = 1,
    id = "fast",
    target_models = TargetModels,
    seed_policy = "named-explicit",
    seeds = [0, 1, 17, 20260809],
    sequence_count = 64,
    max_steps = 64,
    max_shrink_attempts = 128,
    max_retained_counterexamples = 16,
    max_runtime_seconds = 120,
    claim = "Bounded model agreement for the named fast-lane seeds and limits.",
    non_claims = CommonNonClaims,
  } | Profile,
  {
    schema_version = 1,
    id = "deep",
    target_models = TargetModels,
    seed_policy = "named-explicit",
    seeds = [0, 1, 17, 65537, 20260809, 3735928559],
    sequence_count = 512,
    max_steps = 256,
    max_shrink_attempts = 1024,
    max_retained_counterexamples = 32,
    max_runtime_seconds = 1800,
    claim = "Bounded model agreement for the named deep-lane seeds and limits.",
    non_claims = CommonNonClaims,
  } | Profile,
]
in
profiles
