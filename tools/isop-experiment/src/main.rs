//! Controlled ISOP experiment over small Boolean truth tables.
//!
//! Models a finite fault schedule as ordered inclusion variables and a fixed
//! parent/horizon: `f(x)` is the admitted target outcome for subset `x`. Only
//! `true`/`false` rows are admitted here; the production classifier must also
//! reject indeterminate rows, which this experiment deliberately does not model.
//!
//! The cover is computed with a Quine–McCluskey prime extraction followed by an
//! irredundant selection. A separate checker re-derives sufficiency, coverage,
//! and irredundancy from the truth table without reusing the ISOP recurrence.

use serde::Serialize;

/// One cube: `care[i]` is true when variable `i` is constrained; `value[i]` is
/// the required bit for constrained variables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Cube {
    care: u32,
    value: u32,
}

impl Cube {
    fn literal_count(&self) -> u32 {
        self.care.count_ones()
    }

    fn matches(&self, assignment: u32) -> bool {
        assignment & self.care == self.value & self.care
    }

    fn free_bits(&self, variables: u32) -> u32 {
        !self.care & mask(variables)
    }
}

fn mask(variables: u32) -> u32 {
    if variables >= 32 {
        u32::MAX
    } else {
        (1u32 << variables) - 1
    }
}

fn minterm_cube(assignment: u32, variables: u32) -> Cube {
    Cube {
        care: mask(variables),
        value: assignment & mask(variables),
    }
}

fn merges(left: Cube, right: Cube) -> Option<Cube> {
    let differing = left.care ^ right.care;
    if differing != 0 {
        return None;
    }
    let value_difference = (left.value ^ right.value) & left.care;
    if value_difference.count_ones() != 1 {
        return None;
    }
    let bit = value_difference.trailing_zeros();
    Some(Cube {
        care: left.care & !(1 << bit),
        value: left.value & !(1 << bit),
    })
}

fn prime_implicants(table: &[bool], variables: u32) -> Vec<Cube> {
    let mut current: Vec<Cube> = table
        .iter()
        .enumerate()
        .filter(|(_, fail)| **fail)
        .map(|(index, _)| minterm_cube(index as u32, variables))
        .collect();
    let mut primes: Vec<Cube> = Vec::new();
    while !current.is_empty() {
        current.sort_by_key(|cube| (cube.care, cube.value));
        current.dedup();
        let mut used = vec![false; current.len()];
        let mut next = Vec::new();
        for left in 0..current.len() {
            for right in (left + 1)..current.len() {
                if let Some(merged) = merges(current[left], current[right]) {
                    used[left] = true;
                    used[right] = true;
                    if !next.contains(&merged) {
                        next.push(merged);
                    }
                }
            }
        }
        for (index, cube) in current.iter().enumerate() {
            if !used[index] && !primes.contains(cube) {
                primes.push(*cube);
            }
        }
        current = next;
    }
    primes.sort_by_key(|cube| (cube.literal_count(), cube.care, cube.value));
    primes
}

fn covers(cube: Cube, minterm: u32) -> bool {
    cube.matches(minterm)
}

fn irredundant_cover(table: &[bool], variables: u32) -> Vec<Cube> {
    let primes = prime_implicants(table, variables);
    let failures: Vec<u32> = table
        .iter()
        .enumerate()
        .filter(|(_, fail)| **fail)
        .map(|(index, _)| index as u32)
        .collect();
    let mut selected: Vec<Cube> = Vec::new();
    let mut covered = vec![false; failures.len()];

    loop {
        let mut best: Option<(usize, usize, Cube)> = None;
        for prime in &primes {
            let newly: Vec<usize> = failures
                .iter()
                .enumerate()
                .filter(|(index, minterm)| !covered[*index] && covers(*prime, **minterm))
                .map(|(index, _)| index)
                .collect();
            if newly.is_empty() {
                continue;
            }
            let candidate = (newly.len(), prime.literal_count() as usize, *prime);
            let better = match best {
                None => true,
                Some((count, literals, _)) => {
                    candidate.0 > count || (candidate.0 == count && candidate.1 < literals)
                }
            };
            if better {
                best = Some(candidate);
            }
        }
        let Some((_, _, prime)) = best else { break };
        for (index, minterm) in failures.iter().enumerate() {
            if covers(prime, *minterm) {
                covered[index] = true;
            }
        }
        selected.push(prime);
        if covered.iter().all(|value| *value) {
            break;
        }
    }

    // Drop any cube whose removal keeps every failing minterm covered.
    let mut changed = true;
    while changed {
        changed = false;
        for index in 0..selected.len() {
            let candidate = selected[index];
            let rest: Vec<Cube> = selected
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, cube)| *cube)
                .collect();
            if failures
                .iter()
                .all(|minterm| rest.iter().any(|cube| covers(*cube, *minterm)))
            {
                selected.remove(index);
                changed = true;
                break;
            }
        }
    }
    selected.sort_by_key(|cube| (cube.care, cube.value));
    selected
}

/// Fault-injected or externally supplied cover, checked independently.
fn check_cover(table: &[bool], variables: u32, cover: &[Cube]) -> CoverCheck {
    let mut sufficiency = true;
    let mut cube_rows = 0_u64;
    for cube in cover {
        let free = cube.free_bits(variables);
        let mut bits = 0_u32;
        loop {
            let assignment = cube.value | spread(bits, free);
            cube_rows += 1;
            if !cube.matches(assignment) || !table[assignment as usize] {
                sufficiency = false;
            }
            if bits == mask(free.count_ones()) {
                break;
            }
            bits += 1;
        }
    }

    let mut coverage = true;
    let mut uncovered = Vec::new();
    for (assignment, fail) in table.iter().enumerate() {
        if *fail && !cover.iter().any(|cube| covers(*cube, assignment as u32)) {
            coverage = false;
            uncovered.push(assignment as u32);
        }
    }

    let mut irredundant = true;
    for index in 0..cover.len() {
        let cube = cover[index];
        let rest: Vec<Cube> = cover
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, cube)| *cube)
            .collect();
        let covers_unique = (0..1u32 << variables).any(|assignment| {
            table[assignment as usize] && covers(cube, assignment)
                && !rest.iter().any(|other| covers(*other, assignment))
        });
        if !covers_unique {
            irredundant = false;
        }
    }

    CoverCheck {
        sufficiency,
        coverage,
        irredundant,
        cube_rows,
        cubes: cover.len(),
        uncovered,
    }
}

fn spread(bits: u32, positions: u32) -> u32 {
    let mut result = 0;
    let mut source = 0;
    for bit in 0..32 {
        if positions & (1 << bit) != 0 {
            if bits & (1 << source) != 0 {
                result |= 1 << bit;
            }
            source += 1;
        }
    }
    result
}

#[derive(Debug, Serialize)]
struct CoverCheck {
    sufficiency: bool,
    coverage: bool,
    irredundant: bool,
    cube_rows: u64,
    cubes: usize,
    uncovered: Vec<u32>,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    name: String,
    variables: u32,
    duplicate_same_time_variables: Vec<u32>,
    failing_rows: u64,
    matrix_rows: u64,
    cover: Vec<String>,
    literal_count: u32,
    check: CoverCheck,
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    cases: Vec<CaseReport>,
    limitations: Vec<&'static str>,
}

fn render(cube: Cube, variables: u32) -> String {
    let mut parts = Vec::new();
    for bit in 0..variables {
        if cube.care & (1 << bit) == 0 {
            continue;
        }
        let literal = if cube.value & (1 << bit) != 0 {
            format!("x{bit}")
        } else {
            format!("!x{bit}")
        };
        parts.push(literal);
    }
    if parts.is_empty() {
        "TRUE".to_string()
    } else {
        parts.join(" & ")
    }
}

fn case(name: &str, variables: u32, duplicate: &[u32], function: fn(u32) -> bool) -> CaseReport {
    let rows = 1u32 << variables;
    let table: Vec<bool> = (0..rows).map(|assignment| function(assignment)).collect();
    let cover = irredundant_cover(&table, variables);
    let check = check_cover(&table, variables, &cover);
    let literal_count: u32 = cover.iter().map(|cube| cube.literal_count()).sum();
    CaseReport {
        name: name.to_string(),
        variables,
        duplicate_same_time_variables: duplicate.to_vec(),
        failing_rows: table.iter().filter(|fail| **fail).count() as u64,
        matrix_rows: rows as u64,
        cover: cover.iter().map(|cube| render(*cube, variables)).collect(),
        literal_count,
        check,
    }
}

fn main() {
    let cases = vec![
        case("single_fault", 3, &[], |bits| bits & 1 != 0),
        case("and_two_faults", 3, &[], |bits| bits & 0b011 == 0b011),
        case("or_two_faults", 3, &[], |bits| bits & 0b011 != 0),
        case("negative_literal_non_monotone", 2, &[], |bits| {
            bits & 0b01 == 0b01 && bits & 0b10 == 0
        }),
        case("redundant_alternatives", 2, &[], |bits| {
            bits & 0b01 != 0 || (bits & 0b11 == 0b11)
        }),
        case("three_way_consensus", 3, &[], |bits| {
            let x0 = bits & 1 != 0;
            let x1 = bits & 2 != 0;
            let x2 = bits & 4 != 0;
            (x0 && x1) || (x1 && x2) || (x0 && x2)
        }),
        case("constant_true", 2, &[], |_| true),
        case("constant_false", 2, &[], |_| false),
        case("duplicate_same_time_entries", 2, &[0, 1], |bits| {
            bits & 0b11 == 0b11
        }),
        case("no_single_fault_suffices", 4, &[], |bits| {
            (bits & 0b0011 == 0b0011) || (bits & 0b1100 == 0b1100)
        }),
    ];

    // Fault-injected checks: the independent checker must reject broken covers.
    let table: Vec<bool> = (0..4).map(|bits| bits & 0b011 == 0b011).collect();
    let truth = Cube {
        care: 0b011,
        value: 0b011,
    };
    let dropped = check_cover(&table, 2, &[]);
    let insufficient = Cube {
        care: 0b001,
        value: 0b001,
    };
    let unrelated = check_cover(&table, 2, &[insufficient]);
    let redundant = Cube {
        care: 0b000,
        value: 0b000,
    };
    let with_redundant = check_cover(&table, 2, &[truth, redundant]);
    assert!(!dropped.coverage, "empty cover must be reported as incomplete");
    assert!(
        !unrelated.sufficiency,
        "a non-implicant cube must fail sufficiency"
    );
    assert!(
        !with_redundant.irredundant,
        "a TRUE cube beside a full cover must fail irredundancy"
    );

    let report = Report {
        schema: "chaoscontrol.isop_experiment.v1",
        cases,
        limitations: vec![
            "Synthetic truth tables only; no branch evidence, KVM row, or admitted fault outcome is produced.",
            "Indeterminate outcomes, timeouts, replay drift and resource exhaustion are not modeled.",
            "The cover is irredundant, not globally minimum, and grows exponentially in the worst case.",
            "A cube is sufficient within this finite inclusion model only; it does not claim causality or other schedules.",
        ],
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("serialize report")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_of(variables: u32, function: fn(u32) -> bool) -> Vec<bool> {
        (0..1u32 << variables).map(|bits| function(bits)).collect()
    }

    #[test]
    fn every_case_has_a_verified_cover() {
        let and_two = table_of(3, |bits| bits & 0b011 == 0b011);
        let cover = irredundant_cover(&and_two, 3);
        let check = check_cover(&and_two, 3, &cover);
        assert!(check.sufficiency && check.coverage && check.irredundant);

        let non_monotone = table_of(2, |bits| bits & 0b01 == 0b01 && bits & 0b10 == 0);
        let cover = irredundant_cover(&non_monotone, 2);
        let check = check_cover(&non_monotone, 2, &cover);
        assert!(check.sufficiency && check.coverage && check.irredundant);
        assert!(cover.iter().any(|cube| cube.literal_count() == 2));

        let constant_true = table_of(2, |_| true);
        let cover = irredundant_cover(&constant_true, 2);
        let check = check_cover(&constant_true, 2, &cover);
        assert_eq!(cover.len(), 1);
        assert!(check.sufficiency && check.coverage && check.irredundant);

        let constant_false = table_of(2, |_| false);
        let cover = irredundant_cover(&constant_false, 2);
        let check = check_cover(&constant_false, 2, &cover);
        assert!(cover.is_empty());
        assert!(check.sufficiency && check.coverage && check.irredundant);
    }

    #[test]
    fn dropped_cube_fails_coverage() {
        let table = table_of(2, |bits| bits & 0b01 != 0);
        let cover = irredundant_cover(&table, 2);
        let mut broken = cover.clone();
        broken.remove(0);
        let check = check_cover(&table, 2, &broken);
        assert!(!check.coverage);
    }

    #[test]
    fn oversized_cube_fails_sufficiency() {
        let table = table_of(2, |bits| bits & 0b01 != 0);
        let invalid = Cube {
            care: 0b000,
            value: 0b000,
        };
        let check = check_cover(&table, 2, &[invalid]);
        assert!(!check.sufficiency);
    }

    #[test]
    fn redundant_cube_fails_irredundancy() {
        let table = table_of(2, |bits| bits & 0b01 != 0);
        let cover = irredundant_cover(&table, 2);
        let mut redundant = cover.clone();
        let mut extra = cover[0];
        extra.care &= 0b100;
        extra.value &= 0b100;
        redundant.push(extra);
        if extra != cover[0] {
            let check = check_cover(&table, 2, &redundant);
            assert!(!check.irredundant || !check.sufficiency);
        }
    }

    #[test]
    fn different_variable_mapping_changes_the_result() {
        let direct = table_of(2, |bits| (bits & 1 != 0) && (bits & 2 != 0));
        let swapped = table_of(2, |bits| {
            let x0 = bits & 1 != 0;
            let x1 = bits & 2 != 0;
            x1 && x0
        });
        assert_eq!(direct, swapped);
        let asymmetric = table_of(2, |bits| bits & 1 != 0);
        let wrong = table_of(2, |bits| bits & 2 != 0);
        assert_ne!(asymmetric, wrong);
    }

    #[test]
    fn covers_are_irredundant_for_all_functions_of_two_variables() {
        for function in 0..16u32 {
            let table: Vec<bool> = (0..4)
                .map(|bits| function & (1 << bits) != 0)
                .collect();
            let cover = irredundant_cover(&table, 2);
            let check = check_cover(&table, 2, &cover);
            assert!(
                check.sufficiency && check.coverage && check.irredundant,
                "function {function:04b} failed: {check:?}"
            );
        }
    }

    #[test]
    fn duplicate_same_time_entries_are_distinct_variables() {
        let table = table_of(2, |bits| bits & 0b11 == 0b11);
        let cover = irredundant_cover(&table, 2);
        assert_eq!(cover.len(), 1);
        assert_eq!(cover[0].literal_count(), 2);
        let collapsed = table_of(1, |bits| bits & 0b1 == 0b1);
        let collapsed_cover = irredundant_cover(&collapsed, 1);
        assert_eq!(collapsed_cover[0].literal_count(), 1);
    }

}
