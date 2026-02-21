//! Input tree exploration — branch at guest random choice points.
//!
//! Instead of mutating fault schedules at fixed tick intervals, this module
//! explores the tree of possible random choices made by the guest program.
//! Each `random_choice(n)` call is a decision point with `n` possible
//! branches.
//!
//! # Strategy
//!
//! 1. Run the simulation from a snapshot, recording all random choices
//! 2. Select "interesting" choice points to vary
//! 3. Restore snapshot, override one choice to an alternate value, re-run
//! 4. Coverage-guided selection of which alternatives are most interesting
//!
//! # Choice Selection Heuristics
//!
//! - **Small-n priority**: `random_choice(3)` explored before
//!   `random_choice(100)` — fewer options means higher probability each
//!   alternative leads to meaningfully different behavior
//! - **Depth weighting**: Earlier choices affect more of the execution
//! - **Budget capping**: Never generate more alternatives than the budget

use chaoscontrol_fault::engine::ChoiceRecord;
use rand::Rng;
use std::collections::BTreeMap;

/// A proposed alternative value for a specific choice point.
#[derive(Debug, Clone)]
pub struct ChoiceAlternative {
    /// Which VM made the original choice.
    pub vm_id: usize,
    /// The sequence ID within that VM's fault engine.
    pub sequence_id: u64,
    /// How many options the guest requested (0 for `get_random`).
    pub n_options: u32,
    /// The value originally returned.
    pub original_value: u64,
    /// The alternative value to try.
    pub alternative_value: u64,
}

/// Scored choice point for ranking.
struct ScoredChoice {
    vm_id: usize,
    record: ChoiceRecord,
    score: f64,
}

/// Select which choices to vary and what alternatives to try.
///
/// The `budget` parameter caps how many alternatives to generate.
///
/// Selection strategy:
/// 1. Filter to choices with `n_options >= 2` (real decisions only)
/// 2. Score by `1.0 / n_options` × depth_weight (small-n, early = best)
/// 3. For top-scoring choices, generate alternatives that differ from
///    the original value
/// 4. Small-n (≤ 10): enumerate all alternatives
/// 5. Large-n (> 10): sample up to 3 random alternatives
pub fn select_alternatives(
    histories: &[(usize, Vec<ChoiceRecord>)],
    budget: usize,
    rng: &mut impl Rng,
) -> Vec<ChoiceAlternative> {
    if budget == 0 {
        return Vec::new();
    }

    // Flatten and score all choice points
    let mut scored: Vec<ScoredChoice> = Vec::new();
    for &(vm_id, ref choices) in histories {
        for choice in choices {
            if choice.n_options < 2 {
                continue; // Skip get_random and degenerate random_choice(0/1)
            }
            // Score: prefer small n (more impactful) and early choices (deeper effect)
            let n_score = 1.0 / choice.n_options as f64;
            let depth_weight = 1.0 / (1.0 + choice.sequence_id as f64 * 0.01);
            scored.push(ScoredChoice {
                vm_id,
                record: choice.clone(),
                score: n_score * depth_weight,
            });
        }
    }

    // Sort by score descending (highest priority first)
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Generate alternatives from top-scoring choices
    let mut alternatives = Vec::new();
    for sc in &scored {
        if alternatives.len() >= budget {
            break;
        }
        let n = sc.record.n_options;
        let original = sc.record.value;

        if n <= 10 {
            // Enumerate ALL alternatives for small n
            for v in 0..n as u64 {
                if v != original && alternatives.len() < budget {
                    alternatives.push(ChoiceAlternative {
                        vm_id: sc.vm_id,
                        sequence_id: sc.record.sequence_id,
                        n_options: n,
                        original_value: original,
                        alternative_value: v,
                    });
                }
            }
        } else {
            // Sample up to 3 random alternatives for large n
            let sample_count = 3.min(budget - alternatives.len());
            for _ in 0..sample_count {
                let mut v = rng.gen_range(0..n as u64);
                // Retry once if we hit the original value
                if v == original {
                    v = (v + 1) % n as u64;
                }
                alternatives.push(ChoiceAlternative {
                    vm_id: sc.vm_id,
                    sequence_id: sc.record.sequence_id,
                    n_options: n,
                    original_value: original,
                    alternative_value: v,
                });
            }
        }
    }

    alternatives
}

/// Convert a list of alternatives into per-VM override maps.
///
/// Each alternative becomes one independent branch to explore.
/// Returns a `Vec` where each element is a per-VM `Vec` of override
/// maps (indexed by VM ID).
pub fn alternatives_to_overrides(
    alternatives: &[ChoiceAlternative],
    num_vms: usize,
) -> Vec<Vec<BTreeMap<u64, u64>>> {
    alternatives
        .iter()
        .map(|alt| {
            let mut per_vm: Vec<BTreeMap<u64, u64>> = vec![BTreeMap::new(); num_vms];
            if alt.vm_id < num_vms {
                per_vm[alt.vm_id].insert(alt.sequence_id, alt.alternative_value);
            }
            per_vm
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(seq: u64, n: u32, val: u64) -> ChoiceRecord {
        ChoiceRecord {
            sequence_id: seq,
            n_options: n,
            value: val,
        }
    }

    #[test]
    fn empty_history_returns_empty() {
        let mut rng = rand::thread_rng();
        let alts = select_alternatives(&[], 10, &mut rng);
        assert!(alts.is_empty());
    }

    #[test]
    fn zero_budget_returns_empty() {
        let mut rng = rand::thread_rng();
        let histories = vec![(0, vec![choice(0, 3, 1)])];
        let alts = select_alternatives(&histories, 0, &mut rng);
        assert!(alts.is_empty());
    }

    #[test]
    fn skips_n_one_choices() {
        let mut rng = rand::thread_rng();
        let histories = vec![(0, vec![choice(0, 1, 0), choice(1, 0, 42)])];
        let alts = select_alternatives(&histories, 10, &mut rng);
        assert!(alts.is_empty());
    }

    #[test]
    fn small_n_enumerates_all_alternatives() {
        let mut rng = rand::thread_rng();
        let histories = vec![(0, vec![choice(0, 3, 1)])];
        let alts = select_alternatives(&histories, 10, &mut rng);

        // n=3, original=1 → alternatives 0, 2
        assert_eq!(alts.len(), 2);
        let values: Vec<u64> = alts.iter().map(|a| a.alternative_value).collect();
        assert!(values.contains(&0));
        assert!(values.contains(&2));
        assert!(!values.contains(&1)); // Not the original
    }

    #[test]
    fn budget_respected() {
        let mut rng = rand::thread_rng();
        // 3 choices with n=5 → up to 4 alternatives each = 12 total, but budget=5
        let histories = vec![(0, vec![choice(0, 5, 0), choice(1, 5, 0), choice(2, 5, 0)])];
        let alts = select_alternatives(&histories, 5, &mut rng);
        assert!(alts.len() <= 5);
    }

    #[test]
    fn small_n_preferred_over_large_n() {
        let mut rng = rand::thread_rng();
        // Small n choice at seq 0, large n choice at seq 1
        let histories = vec![(0, vec![choice(0, 3, 1), choice(1, 100, 50)])];
        let alts = select_alternatives(&histories, 3, &mut rng);

        // First alternatives should be from the n=3 choice
        assert!(alts.len() >= 2);
        assert_eq!(alts[0].n_options, 3);
        assert_eq!(alts[1].n_options, 3);
    }

    #[test]
    fn large_n_samples_alternatives() {
        let mut rng = rand::thread_rng();
        let histories = vec![(0, vec![choice(0, 100, 50)])];
        let alts = select_alternatives(&histories, 10, &mut rng);

        // Should get up to 3 alternatives for large n
        assert!(alts.len() <= 3);
        assert!(alts.len() >= 1);
        for alt in &alts {
            assert!(alt.alternative_value < 100);
        }
    }

    #[test]
    fn multi_vm_histories() {
        let mut rng = rand::thread_rng();
        let histories = vec![(0, vec![choice(0, 3, 1)]), (1, vec![choice(0, 4, 2)])];
        let alts = select_alternatives(&histories, 20, &mut rng);

        // Should have alternatives from both VMs
        let vm0_count = alts.iter().filter(|a| a.vm_id == 0).count();
        let vm1_count = alts.iter().filter(|a| a.vm_id == 1).count();
        assert!(vm0_count > 0);
        assert!(vm1_count > 0);
    }

    #[test]
    fn alternatives_to_overrides_correct() {
        let alts = vec![
            ChoiceAlternative {
                vm_id: 0,
                sequence_id: 5,
                n_options: 3,
                original_value: 1,
                alternative_value: 2,
            },
            ChoiceAlternative {
                vm_id: 1,
                sequence_id: 3,
                n_options: 4,
                original_value: 0,
                alternative_value: 3,
            },
        ];

        let overrides = alternatives_to_overrides(&alts, 3);
        assert_eq!(overrides.len(), 2);

        // First override: VM 0 seq 5 → 2
        assert_eq!(overrides[0].len(), 3); // 3 VMs
        assert_eq!(overrides[0][0].get(&5), Some(&2));
        assert!(overrides[0][1].is_empty());
        assert!(overrides[0][2].is_empty());

        // Second override: VM 1 seq 3 → 3
        assert!(overrides[1][0].is_empty());
        assert_eq!(overrides[1][1].get(&3), Some(&3));
        assert!(overrides[1][2].is_empty());
    }

    #[test]
    fn alternatives_to_overrides_empty() {
        let overrides = alternatives_to_overrides(&[], 2);
        assert!(overrides.is_empty());
    }
}
