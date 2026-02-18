// Verus specification for entropy pure functions
verus! {

/// Seed expansion places bytes correctly
proof fn expand_seed_correctness(seed: u64)
    ensures
        expand_seed(seed)[0..8] == seed.to_le_bytes(),
        forall |i: usize| 8 <= i < 32 ==> expand_seed(seed)[i] == 0,
{ }

/// Same seed always produces same expansion
proof fn expand_seed_deterministic(seed: u64)
    ensures expand_seed(seed) == expand_seed(seed),
{ }

/// Different seeds produce different expansions
proof fn expand_seed_injective(a: u64, b: u64)
    requires a != b,
    ensures expand_seed(a) != expand_seed(b),
{ }

}
