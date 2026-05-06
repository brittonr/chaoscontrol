//! Format exploration reports for human consumption.

use crate::campaign::CampaignReport;
use crate::corpus::BugReport;
use crate::explorer::{AssertionDetail, ExplorationReport};
use std::collections::BTreeMap;

#[derive(Default)]
struct AssertionExerciseGroup {
    cataloged: usize,
    exercised: usize,
    failed: usize,
}

fn assertion_exercise_groups(
    details: &[AssertionDetail],
) -> BTreeMap<(String, String), AssertionExerciseGroup> {
    let mut groups: BTreeMap<(String, String), AssertionExerciseGroup> = BTreeMap::new();
    for detail in details {
        let key = (detail.guest.clone(), detail.category.clone());
        let group = groups.entry(key).or_default();
        group.cataloged += 1;
        if detail.hit_count > 0 {
            group.exercised += 1;
        }
        if detail.verdict == "failed" {
            group.failed += 1;
        }
    }
    groups
}

pub fn min_assertion_exercise_failures(details: &[AssertionDetail], floor: usize) -> usize {
    if floor == 0 {
        return 0;
    }
    assertion_exercise_groups(details)
        .values()
        .filter(|group| group.exercised < floor)
        .count()
}

fn append_assertion_exercise_summary(output: &mut String, details: &[AssertionDetail]) {
    let groups = assertion_exercise_groups(details);
    if groups.is_empty() {
        return;
    }
    output.push_str("─── Assertion Exercise by Guest/Category ─────────────────────────────\n");
    output.push_str("  Guest          Category      Cataloged  Exercised  Failed\n");
    output.push_str("  ────────────── ───────────── ─────────  ─────────  ──────\n");
    for ((guest, category), group) in groups {
        output.push_str(&format!(
            "  {guest:<14} {category:<13} {cataloged:>9}  {exercised:>9}  {failed:>6}\n",
            guest = guest,
            category = category,
            cataloged = group.cataloged,
            exercised = group.exercised,
            failed = group.failed
        ));
    }
    output.push('\n');
}

/// Format an exploration report for human consumption.
pub fn format_report(report: &ExplorationReport) -> String {
    let mut output = String::new();

    output.push_str("═══════════════════════════════════════════════════════════════════════\n");
    output.push_str("  ChaosControl Exploration Report\n");
    output.push_str("═══════════════════════════════════════════════════════════════════════\n\n");

    // Summary
    output.push_str(&format!("Exploration rounds:     {}\n", report.rounds));
    output.push_str(&format!(
        "Total branches explored: {}\n",
        report.total_branches
    ));
    output.push_str(&format!("Corpus entries:         {}\n", report.corpus_size));
    output.push_str(&format!("Unique edges found:     {}\n", report.total_edges));
    output.push_str(&format!("Bugs discovered:        {}\n", report.bugs.len()));
    if report.wall_clock_seconds > 0.0 {
        output.push_str(&format!(
            "Wall-clock time:        {}\n",
            format_duration(report.wall_clock_seconds)
        ));
    }
    output.push('\n');

    let total_restore_ms: f64 = report.round_history.iter().map(|h| h.restore_ms).sum();
    let total_run_ms: f64 = report.round_history.iter().map(|h| h.run_ms).sum();
    let total_snapshot_ms: f64 = report.round_history.iter().map(|h| h.snapshot_ms).sum();
    let total_coverage_ms: f64 = report.round_history.iter().map(|h| h.coverage_ms).sum();
    let total_phase_ms = total_restore_ms + total_run_ms + total_snapshot_ms + total_coverage_ms;
    if report.wall_clock_seconds > 0.0 || total_phase_ms > 0.0 {
        output.push_str("─── Performance ──────────────────────────────────────────────────────\n");
        output.push_str(&format!(
            "Wall time:              {}\n",
            format_duration(report.wall_clock_seconds)
        ));
        output.push_str(&format!(
            "Throughput:             {:.2} branches/sec, {:.2} edges/sec\n",
            report.branches_per_second, report.edges_per_second
        ));
        if total_phase_ms > 0.0 {
            let pct = |ms: f64| ms / total_phase_ms * 100.0;
            output.push_str(&format!(
                "Phase breakdown:        Run: {} ({:.0}%) | Snapshot: {} ({:.0}%) | Restore: {} ({:.0}%) | Coverage: {} ({:.0}%)\n",
                format_duration(total_run_ms / 1000.0),
                pct(total_run_ms),
                format_duration(total_snapshot_ms / 1000.0),
                pct(total_snapshot_ms),
                format_duration(total_restore_ms / 1000.0),
                pct(total_restore_ms),
                format_duration(total_coverage_ms / 1000.0),
                pct(total_coverage_ms)
            ));
        }
        output.push('\n');
    }

    // Scenario metadata (if helical scenario was used)
    if let Some(ref summary) = report.scenario_summary {
        output
            .push_str("─── Helical Scenario ──────────────────────────────────────────────────\n");
        output.push_str(&format!(
            "Scenario family:        {}\n",
            summary.config.family
        ));
        output.push_str(&format!(
            "Phase ticks:            {}\n",
            summary.config.phase_ticks
        ));
        output.push_str(&format!(
            "Turns:                  {}\n",
            summary.config.turns
        ));
        output.push_str(&format!(
            "Total duration:         {} ns\n",
            summary.total_duration_ns
        ));
        output.push_str(&format!(
            "Phases:                 {}\n",
            summary.phases.len()
        ));
        output.push('\n');
        // Phase table (truncate after 20 rows)
        let max_phases = 20;
        for (i, phase) in summary.phases.iter().enumerate() {
            if i >= max_phases {
                output.push_str(&format!(
                    "  ... and {} more phases\n",
                    summary.phases.len() - max_phases
                ));
                break;
            }
            output.push_str(&format!(
                "  turn {} | vm{} | {} | {}ns–{}ns | {}\n",
                phase.turn,
                phase.target_vm,
                phase.kind,
                phase.start_ns,
                phase.end_ns,
                phase.description
            ));
        }
        output.push('\n');
    }

    // Coverage stats
    output.push_str("─── Coverage Statistics ───────────────────────────────────────────────\n");
    output.push_str(&format!(
        "Total runs:             {}\n",
        report.coverage_stats.total_runs
    ));
    output.push_str(&format!(
        "Unique edges:           {}\n",
        report.coverage_stats.total_edges
    ));
    output.push_str(&format!(
        "Avg edges/run:          {:.2}\n",
        report.coverage_stats.edges_per_run_avg
    ));
    output.push('\n');

    // Network stats
    let ns = &report.network_stats;
    if ns.packets_sent > 0 {
        output
            .push_str("─── Network Fabric Statistics ─────────────────────────────────────────\n");
        output.push_str(&format!("Packets sent:           {}\n", ns.packets_sent));
        output.push_str(&format!(
            "Packets delivered:      {}\n",
            ns.packets_delivered
        ));
        if ns.packets_dropped_partition > 0 {
            output.push_str(&format!(
                "Dropped (partition):    {}\n",
                ns.packets_dropped_partition
            ));
        }
        if ns.packets_dropped_loss > 0 {
            output.push_str(&format!(
                "Dropped (loss):         {}\n",
                ns.packets_dropped_loss
            ));
        }
        if ns.packets_corrupted > 0 {
            output.push_str(&format!(
                "Corrupted:              {}\n",
                ns.packets_corrupted
            ));
        }
        if ns.packets_duplicated > 0 {
            output.push_str(&format!(
                "Duplicated:             {}\n",
                ns.packets_duplicated
            ));
        }
        if ns.packets_bandwidth_delayed > 0 {
            let avg_bw = ns.total_bandwidth_delay_ticks / ns.packets_bandwidth_delayed.max(1);
            output.push_str(&format!(
                "Bandwidth delayed:      {} (avg {} ticks)\n",
                ns.packets_bandwidth_delayed, avg_bw
            ));
        }
        if ns.packets_jittered > 0 {
            let avg_j = ns.total_jitter_ticks / ns.packets_jittered.max(1);
            output.push_str(&format!(
                "Jittered:               {} (avg {} ticks)\n",
                ns.packets_jittered, avg_j
            ));
        }
        if ns.packets_reordered > 0 {
            output.push_str(&format!(
                "Reordered:              {}\n",
                ns.packets_reordered
            ));
        }
        output.push('\n');
    }

    // Assertion coverage
    let ast = &report.assertion_stats;
    if ast.catalog_size > 0 {
        output
            .push_str("─── Assertion Coverage ────────────────────────────────────────────────\n");
        output.push_str(&format!("Registered sites:       {}\n", ast.catalog_size));
        output.push_str(&format!("Passed:                 {}\n", ast.passed));
        output.push_str(&format!("Failed:                 {}\n", ast.failed));
        output.push_str(&format!("Unexercised:            {}\n", ast.unexercised));
        if ast.catalog_size > 0 {
            let exercised = ast.catalog_size - ast.unexercised;
            let pct = exercised as f64 / ast.catalog_size as f64 * 100.0;
            output.push_str(&format!(
                "Exercised:              {}/{} ({:.1}%)\n",
                exercised, ast.catalog_size, pct
            ));
        }
        output.push('\n');
    }

    append_assertion_exercise_summary(&mut output, &report.assertion_details);

    // Per-assertion detail
    if !report.assertion_details.is_empty() {
        // Group by verdict for readability
        let failed: Vec<_> = report
            .assertion_details
            .iter()
            .filter(|a| a.verdict == "failed")
            .collect();
        let unexercised: Vec<_> = report
            .assertion_details
            .iter()
            .filter(|a| a.verdict == "unexercised")
            .collect();
        let passed: Vec<_> = report
            .assertion_details
            .iter()
            .filter(|a| a.verdict == "passed")
            .collect();

        if !failed.is_empty() {
            output.push_str(
                "─── Failed Assertions ────────────────────────────────────────────────\n",
            );
            for a in &failed {
                output.push_str(&format!(
                    "  ✗ [{}] {} ({}): {} hits, {}/{} true\n",
                    a.kind, a.message, a.id, a.hit_count, a.true_count, a.hit_count
                ));
                if let Some(details) = &a.last_failure_details {
                    // Pretty-print JSON if valid, otherwise raw
                    let pretty = serde_json::from_str::<serde_json::Value>(details)
                        .ok()
                        .and_then(|v| serde_json::to_string_pretty(&v).ok());
                    if let Some(formatted) = pretty {
                        for line in formatted.lines() {
                            output.push_str(&format!("      {}\n", line));
                        }
                    } else {
                        output.push_str(&format!("      {}\n", details));
                    }
                }
            }
            output.push('\n');
        }

        if !unexercised.is_empty() {
            output.push_str(
                "─── Unexercised Assertions ───────────────────────────────────────────\n",
            );
            for a in &unexercised {
                output.push_str(&format!("  ○ [{}] {} ({})\n", a.kind, a.message, a.id));
            }
            output.push('\n');
        }

        if !passed.is_empty() {
            output.push_str(
                "─── Passed Assertions ────────────────────────────────────────────────\n",
            );
            for a in &passed {
                output.push_str(&format!(
                    "  ✓ [{}] {} ({}) — {} hits\n",
                    a.kind, a.message, a.id, a.hit_count
                ));
            }
            output.push('\n');
        }
    }

    // Per-round history
    if !report.round_history.is_empty() {
        output.push_str(
            "─── Exploration Progress ────���─────────────────────────────────────────\n",
        );
        let history = &report.round_history;
        let show_timings = history.iter().any(|h| {
            h.restore_ms > 0.0 || h.run_ms > 0.0 || h.snapshot_ms > 0.0 || h.coverage_ms > 0.0
        });
        if show_timings {
            output.push_str("  Round │ Branches │ New Edges │ Cum. Edges │ Bugs │ Frontier │ Corpus │ Restore │ Run │ Snapshot │ Coverage\n");
            output.push_str("  ──────┼──────────┼───────────┼────────────┼──────┼──────────┼────────┼─────────┼─────┼──────────┼─────────\n");
        } else {
            output.push_str(
                "  Round │ Branches │ New Edges │ Cum. Edges │ Bugs │ Frontier │ Corpus\n",
            );
            output.push_str(
                "  ──────┼──────────┼───────────┼────────────┼──────┼──────────┼───────\n",
            );
        }

        // Show all rounds if ≤ 20, otherwise show first 5 + last 5 with a gap
        let show_all = history.len() <= 20;
        let rows: Vec<(usize, &crate::explorer::RoundHistory)> = if show_all {
            history.iter().enumerate().collect()
        } else {
            let mut rows: Vec<(usize, &crate::explorer::RoundHistory)> =
                history.iter().enumerate().take(5).collect();
            rows.push((usize::MAX, &history[0])); // sentinel for "..."
            rows.extend(history.iter().enumerate().skip(history.len() - 5));
            rows
        };

        for (i, entry) in &rows {
            if *i == usize::MAX {
                if show_timings {
                    output.push_str(
                        "     ⋮  │    ⋮     │     ⋮     │      ⋮     │  ⋮   │    ⋮     │   ⋮    │    ⋮    │  ⋮  │    ⋮     │    ⋮\n",
                    );
                } else {
                    output.push_str(
                        "     ⋮  │    ⋮     │     ⋮     │      ⋮     │  ⋮   │    ⋮     │   ⋮\n",
                    );
                }
                continue;
            }
            if show_timings {
                output.push_str(&format!(
                    "  {:>5} │ {:>8} │ {:>9} │ {:>10} │ {:>4} │ {:>8} │ {:>6} │ {:>7} │ {:>3} │ {:>8} │ {:>7}\n",
                    entry.round,
                    entry.branches_run,
                    entry.new_edges,
                    entry.cumulative_edges,
                    entry.cumulative_bugs,
                    entry.frontier_size,
                    entry.corpus_size,
                    format_ms_or_dash(entry.restore_ms),
                    format_ms_or_dash(entry.run_ms),
                    format_ms_or_dash(entry.snapshot_ms),
                    format_ms_or_dash(entry.coverage_ms),
                ));
            } else {
                output.push_str(&format!(
                    "  {:>5} │ {:>8} │ {:>9} │ {:>10} │ {:>4} │ {:>8} │ {:>6}\n",
                    entry.round,
                    entry.branches_run,
                    entry.new_edges,
                    entry.cumulative_edges,
                    entry.cumulative_bugs,
                    entry.frontier_size,
                    entry.corpus_size,
                ));
            }
        }

        // Coverage growth summary
        if history.len() >= 2 {
            let first = &history[0];
            let last = &history[history.len() - 1];
            let mid = &history[history.len() / 2];

            output.push('\n');
            output.push_str(&format!(
                "  Coverage growth: {} → {} → {} edges (round 1 → {} → {})\n",
                first.cumulative_edges,
                mid.cumulative_edges,
                last.cumulative_edges,
                mid.round,
                last.round,
            ));

            // Rounds with zero new edges (plateau indicator)
            let plateau_rounds = history.iter().filter(|h| h.new_edges == 0).count();
            if plateau_rounds > 0 {
                output.push_str(&format!(
                    "  Plateau rounds:  {}/{} ({:.0}% produced no new coverage)\n",
                    plateau_rounds,
                    history.len(),
                    plateau_rounds as f64 / history.len() as f64 * 100.0,
                ));
            }

            // Bug discovery timeline
            let bug_rounds: Vec<u64> = history
                .iter()
                .filter(|h| h.bugs_found > 0)
                .map(|h| h.round)
                .collect();
            if !bug_rounds.is_empty() {
                let round_list: Vec<String> = bug_rounds.iter().map(|r| r.to_string()).collect();
                output.push_str(&format!(
                    "  Bugs found in:   round{} {}\n",
                    if bug_rounds.len() > 1 { "s" } else { "" },
                    round_list.join(", "),
                ));
            }
        }
        output.push('\n');
    }

    // Bug details
    if !report.bugs.is_empty() {
        output
            .push_str("─── Bugs Found ─────────────────────────────────────────────────────────\n");
        for (i, bug) in report.bugs.iter().enumerate() {
            output.push_str(&format!("\n{}. Bug #{}\n", i + 1, bug.bug_id));
            output.push_str(&format_bug(bug));
            output.push('\n');
        }
    } else {
        output
            .push_str("─── No Bugs Found ──────────────────────────────────────────────────────\n");
        output.push_str("No assertion failures detected during exploration.\n\n");
    }

    output.push_str("═══════════════════════════════════════════════════════════════════════\n");

    output
}

/// Format a bug report with reproduction steps.
pub fn format_bug(bug: &BugReport) -> String {
    let mut output = String::new();

    output.push_str(&format!("   Assertion ID: {}\n", bug.assertion_id));
    output.push_str(&format!("   Location:     {}\n", bug.assertion_location));
    output.push_str(&format!("   Tick:         {}\n", bug.tick));
    output.push_str(&format!(
        "   Schedule:     {} faults\n",
        bug.schedule.total()
    ));

    if bug.snapshot.is_some() {
        output.push_str("   Snapshot:     Available for replay\n");
    } else {
        output.push_str("   Snapshot:     Not captured\n");
    }

    // Show fault schedule details
    if bug.schedule.total() > 0 {
        output.push_str("\n   Fault Schedule:\n");

        let faults = bug.schedule.faults();

        for (i, fault) in faults.iter().take(10).enumerate() {
            output.push_str(&format!(
                "     [{}] @ {}ns: {:?}\n",
                i + 1,
                fault.time_ns,
                fault.fault
            ));
        }

        if faults.len() > 10 {
            output.push_str(&format!("     ... and {} more faults\n", faults.len() - 10));
        }
    }

    // Show schedule variant if present
    if let Some(ref variant) = bug.schedule_variant {
        output.push_str("\n   Schedule Variant:\n");
        output.push_str(&format!("     Seed:     {}\n", variant.scheduler_seed));
        if let Some(ref strategy) = variant.strategy_override {
            output.push_str(&format!("     Strategy: {:?}\n", strategy));
        }
        if let Some(quantum) = variant.quantum_override {
            output.push_str(&format!("     Quantum:  {}\n", quantum));
        }
    }

    output
}

/// Format a campaign report (multi-seed) for human consumption.
pub fn format_campaign_report(report: &CampaignReport) -> String {
    let mut output = String::new();

    output.push_str("═══════════════════════════════════════════════════════════════════════\n");
    output.push_str("  ChaosControl Campaign Report\n");
    output.push_str("═══════════════════════════════════════════════════════════════════════\n\n");

    // Summary
    output.push_str(&format!(
        "Seeds run:              {}\n",
        report.seeds_run.len()
    ));
    output.push_str(&format!(
        "Total rounds:           {}\n",
        report.total_rounds
    ));
    output.push_str(&format!(
        "Total branches:         {}\n",
        report.total_branches
    ));
    output.push_str(&format!("Unique bugs found:      {}\n", report.bugs.len()));
    output.push_str(&format!(
        "Seeds with bugs:        {}/{}\n",
        report.seeds_with_bugs.len(),
        report.seeds_run.len()
    ));
    output.push_str(&format!(
        "Wall-clock time:        {:.1}s\n",
        report.wall_clock_seconds
    ));
    if !report.failed_seeds.is_empty() {
        output.push_str(&format!(
            "Failed seeds:           {}",
            report.failed_seeds.len()
        ));
        let names: Vec<String> = report
            .failed_seeds
            .iter()
            .map(|(s, _)| s.to_string())
            .collect();
        output.push_str(&format!(" ({})", names.join(", ")));
        output.push('\n');
    }
    output.push('\n');

    // Scenario metadata (if helical scenario was used)
    if let Some(ref sc) = report.scenario_config {
        output
            .push_str("─── Helical Scenario ──────────────────────────────────────────────────\n");
        output.push_str(&format!("Scenario family:        {}\n", sc.family));
        output.push_str(&format!("Phase ticks:            {}\n", sc.phase_ticks));
        output.push_str(&format!("Turns:                  {}\n", sc.turns));
        output.push('\n');
    }

    // Per-seed table
    output.push_str("─── Per-Seed Results ──────────────────────────────────────────────────\n");
    output.push_str("  Seed   │ Rounds │ Branches │ Edges │ Bugs │ Time\n");
    output.push_str("  ───────┼────────┼──────────┼───────┼──────┼──────\n");

    for s in &report.per_seed {
        output.push_str(&format!(
            "  {:>6} │ {:>6} │ {:>8} │ {:>5} │ {:>4} │ {:.1}s\n",
            s.seed, s.rounds, s.total_branches, s.total_edges, s.bugs_found, s.wall_clock_seconds,
        ));
    }

    // Failed seeds detail
    if !report.failed_seeds.is_empty() {
        output.push_str("─── Failed Seeds ─────────────────────────────────────────────────────\n");
        for (seed, error) in &report.failed_seeds {
            output.push_str(&format!("  Seed {}: {}\n", seed, error));
        }
    }
    output.push('\n');

    // Assertion coverage
    let ast = &report.assertion_stats;
    if ast.catalog_size > 0 {
        output
            .push_str("─── Assertion Coverage (merged) ───────────────────────────────────────\n");
        output.push_str(&format!("Registered sites:       {}\n", ast.catalog_size));
        output.push_str(&format!("Passed:                 {}\n", ast.passed));
        output.push_str(&format!("Failed:                 {}\n", ast.failed));
        output.push_str(&format!("Unexercised:            {}\n", ast.unexercised));
        if ast.catalog_size > 0 {
            let exercised = ast.catalog_size - ast.unexercised;
            let pct = exercised as f64 / ast.catalog_size as f64 * 100.0;
            output.push_str(&format!(
                "Exercised:              {}/{} ({:.1}%)\n",
                exercised, ast.catalog_size, pct
            ));
        }
        output.push('\n');
    }

    append_assertion_exercise_summary(&mut output, &report.assertion_details);

    // Per-assertion detail
    if !report.assertion_details.is_empty() {
        let failed: Vec<_> = report
            .assertion_details
            .iter()
            .filter(|a| a.verdict == "failed")
            .collect();
        let unexercised: Vec<_> = report
            .assertion_details
            .iter()
            .filter(|a| a.verdict == "unexercised")
            .collect();
        let passed: Vec<_> = report
            .assertion_details
            .iter()
            .filter(|a| a.verdict == "passed")
            .collect();

        if !failed.is_empty() {
            output.push_str(
                "─── Failed Assertions ────────────────────────────────────────────────\n",
            );
            for a in &failed {
                output.push_str(&format!(
                    "  ✗ [{}] {} ({}): {} hits, {}/{} true\n",
                    a.kind, a.message, a.id, a.hit_count, a.true_count, a.hit_count
                ));
            }
            output.push('\n');
        }

        if !unexercised.is_empty() {
            output.push_str(
                "─── Unexercised Assertions ───────────────────────────────────────────\n",
            );
            for a in &unexercised {
                output.push_str(&format!("  ○ [{}] {} ({})\n", a.kind, a.message, a.id));
            }
            output.push('\n');
        }

        if !passed.is_empty() {
            output.push_str(
                "─── Passed Assertions ────────────────────────────────────────────────\n",
            );
            for a in &passed {
                output.push_str(&format!(
                    "  ✓ [{}] {} ({}) — {} hits\n",
                    a.kind, a.message, a.id, a.hit_count
                ));
            }
            output.push('\n');
        }
    }

    // Bug details
    if !report.bugs.is_empty() {
        output
            .push_str("─── Bugs Found ─────────────────────────────────────────────────────────\n");
        for (i, cbug) in report.bugs.iter().enumerate() {
            let seeds_str: Vec<String> =
                cbug.found_by_seeds.iter().map(|s| s.to_string()).collect();
            output.push_str(&format!(
                "\n{}. Assertion: {} (id={})\n",
                i + 1,
                cbug.bug.assertion_location,
                cbug.bug.assertion_id
            ));
            output.push_str(&format!("   Tick:       {}\n", cbug.bug.tick));
            output.push_str(&format!(
                "   Schedule:   {} faults\n",
                cbug.bug.schedule.faults.len()
            ));
            output.push_str(&format!(
                "   Found by:   seed{} {}\n",
                if cbug.found_by_seeds.len() > 1 {
                    "s"
                } else {
                    ""
                },
                seeds_str.join(", "),
            ));
        }
        output.push('\n');
    } else {
        output
            .push_str("─── No Bugs Found ──────────────────────────────────────────────────────\n");
        output.push_str("No assertion failures detected across any seed.\n\n");
    }

    output.push_str("═══════════════════════════════════════════════════════════════════════\n");
    output
}

/// Format seconds into human-readable duration.
fn format_ms_or_dash(ms: f64) -> String {
    if ms <= 0.0 {
        "—".to_string()
    } else if ms >= 1000.0 {
        format_duration(ms / 1000.0)
    } else {
        format!("{ms:.1}ms")
    }
}

fn format_duration(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{:.1}s", seconds)
    } else if seconds < 3600.0 {
        let mins = (seconds / 60.0).floor() as u64;
        let secs = seconds - (mins as f64 * 60.0);
        format!("{}m {:.0}s", mins, secs)
    } else {
        let hours = (seconds / 3600.0).floor() as u64;
        let mins = ((seconds - hours as f64 * 3600.0) / 60.0).floor() as u64;
        format!("{}h {}m", hours, mins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::CoverageStats;
    use chaoscontrol_fault::schedule::FaultSchedule;

    fn make_bug(id: u64, assertion_id: u64, location: &str) -> BugReport {
        BugReport {
            bug_id: id,
            assertion_id,
            assertion_location: location.to_string(),
            schedule: FaultSchedule::new(),
            snapshot: None,
            tick: 1000,
            replay_parent_depth: 0,
            dedup_key: 0,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        }
    }

    #[test]
    fn test_format_report_with_assertion_stats() {
        use crate::explorer::AssertionStats;

        let report = ExplorationReport {
            rounds: 10,
            total_branches: 80,
            total_edges: 256,
            bugs: Vec::new(),
            corpus_size: 15,
            coverage_stats: CoverageStats {
                total_edges: 256,
                total_runs: 80,
                edges_per_run_avg: 3.2,
            },
            network_stats: Default::default(),
            assertion_stats: AssertionStats {
                catalog_size: 35,
                passed: 20,
                failed: 0,
                unexercised: 15,
            },
            assertion_details: Vec::new(),
            round_history: Vec::new(),
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_report(&report);
        assert!(formatted.contains("Assertion Coverage"));
        assert!(formatted.contains("Registered sites:       35"));
        assert!(formatted.contains("Passed:                 20"));
        assert!(formatted.contains("Unexercised:            15"));
        assert!(formatted.contains("Exercised:              20/35 (57.1%)"));
    }

    #[test]
    fn test_format_report_no_bugs() {
        let report = ExplorationReport {
            rounds: 10,
            total_branches: 80,
            total_edges: 256,
            bugs: Vec::new(),
            corpus_size: 15,
            coverage_stats: CoverageStats {
                total_edges: 256,
                total_runs: 80,
                edges_per_run_avg: 3.2,
            },
            network_stats: Default::default(),
            assertion_stats: Default::default(),
            assertion_details: Vec::new(),
            round_history: Vec::new(),
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_report(&report);
        assert!(formatted.contains("Exploration rounds:     10"));
        assert!(formatted.contains("Total branches explored: 80"));
        assert!(formatted.contains("Bugs discovered:        0"));
        assert!(formatted.contains("No Bugs Found"));
    }

    #[test]
    fn test_format_report_with_bugs() {
        let bugs = vec![
            make_bug(0, 100, "test.rs:42"),
            make_bug(1, 200, "main.rs:123"),
        ];

        let report = ExplorationReport {
            rounds: 5,
            total_branches: 40,
            total_edges: 128,
            bugs,
            corpus_size: 8,
            coverage_stats: CoverageStats {
                total_edges: 128,
                total_runs: 40,
                edges_per_run_avg: 3.2,
            },
            network_stats: Default::default(),
            assertion_stats: Default::default(),
            assertion_details: Vec::new(),
            round_history: Vec::new(),
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_report(&report);
        assert!(formatted.contains("Bugs discovered:        2"));
        assert!(formatted.contains("Bug #0"));
        assert!(formatted.contains("Bug #1"));
        assert!(formatted.contains("test.rs:42"));
        assert!(formatted.contains("main.rs:123"));
    }

    #[test]
    fn test_format_bug() {
        let bug = make_bug(42, 100, "critical.rs:999");
        let formatted = format_bug(&bug);

        assert!(formatted.contains("Assertion ID: 100"));
        assert!(formatted.contains("critical.rs:999"));
        assert!(formatted.contains("Tick:         1000"));
        assert!(formatted.contains("Snapshot:     Not captured"));
    }

    #[test]
    fn test_format_bug_with_schedule() {
        use chaoscontrol_fault::faults::Fault;
        use chaoscontrol_fault::schedule::ScheduledFault;

        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(1000, Fault::NetworkHeal));
        schedule.add(ScheduledFault::new(2000, Fault::ProcessKill { target: 0 }));

        let bug = BugReport {
            bug_id: 1,
            assertion_id: 50,
            assertion_location: "bug.rs:1".to_string(),
            schedule,
            snapshot: None,
            tick: 5000,
            replay_parent_depth: 0,
            dedup_key: 0,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_bug(&bug);
        assert!(formatted.contains("Schedule:     2 faults"));
        assert!(formatted.contains("Fault Schedule:"));
        assert!(formatted.contains("@ 1000ns:"));
        assert!(formatted.contains("@ 2000ns:"));
    }

    #[test]
    fn test_format_report_with_round_history() {
        use crate::explorer::RoundHistory;

        let history = vec![
            RoundHistory {
                round: 1,
                branches_run: 8,
                new_edges: 50,
                cumulative_edges: 50,
                bugs_found: 0,
                cumulative_bugs: 0,
                frontier_size: 3,
                corpus_size: 3,
                restore_ms: 0.0,
                run_ms: 0.0,
                snapshot_ms: 0.0,
                coverage_ms: 0.0,
                wall_clock_seconds: 0.0,
            },
            RoundHistory {
                round: 2,
                branches_run: 8,
                new_edges: 30,
                cumulative_edges: 80,
                bugs_found: 1,
                cumulative_bugs: 1,
                frontier_size: 5,
                corpus_size: 5,
                restore_ms: 0.0,
                run_ms: 0.0,
                snapshot_ms: 0.0,
                coverage_ms: 0.0,
                wall_clock_seconds: 0.0,
            },
            RoundHistory {
                round: 3,
                branches_run: 8,
                new_edges: 0,
                cumulative_edges: 80,
                bugs_found: 0,
                cumulative_bugs: 1,
                frontier_size: 4,
                corpus_size: 5,
                restore_ms: 0.0,
                run_ms: 0.0,
                snapshot_ms: 0.0,
                coverage_ms: 0.0,
                wall_clock_seconds: 0.0,
            },
        ];

        let report = ExplorationReport {
            rounds: 3,
            total_branches: 24,
            total_edges: 80,
            bugs: Vec::new(),
            corpus_size: 5,
            coverage_stats: CoverageStats {
                total_edges: 80,
                total_runs: 24,
                edges_per_run_avg: 3.3,
            },
            network_stats: Default::default(),
            assertion_stats: Default::default(),
            assertion_details: Vec::new(),
            round_history: history,
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_report(&report);

        // Table header present
        assert!(formatted.contains("Exploration Progress"));
        assert!(formatted.contains("Round"));
        assert!(formatted.contains("New Edges"));

        // Row data present
        assert!(formatted.contains("50"));
        assert!(formatted.contains("80"));

        // Coverage growth summary
        assert!(formatted.contains("Coverage growth:"));
        assert!(formatted.contains("50"));
        assert!(formatted.contains("80"));

        // Plateau detection
        assert!(formatted.contains("Plateau rounds:"));
        assert!(formatted.contains("1/3"));

        // Bug discovery timeline
        assert!(formatted.contains("Bugs found in:"));
        assert!(formatted.contains("round 2"));
    }

    #[test]
    fn test_format_report_round_history_truncation() {
        use crate::explorer::RoundHistory;

        // 25 rounds — should show first 5, gap, last 5
        let history: Vec<RoundHistory> = (1..=25)
            .map(|r| RoundHistory {
                round: r,
                branches_run: 8,
                new_edges: if r <= 10 { 5 } else { 0 },
                cumulative_edges: (r as usize).min(10) * 5,
                bugs_found: 0,
                cumulative_bugs: 0,
                frontier_size: 3,
                corpus_size: r as usize,
                restore_ms: 0.0,
                run_ms: 0.0,
                snapshot_ms: 0.0,
                coverage_ms: 0.0,
                wall_clock_seconds: 0.0,
            })
            .collect();

        let report = ExplorationReport {
            rounds: 25,
            total_branches: 200,
            total_edges: 50,
            bugs: Vec::new(),
            corpus_size: 25,
            coverage_stats: CoverageStats {
                total_edges: 50,
                total_runs: 200,
                edges_per_run_avg: 2.0,
            },
            network_stats: Default::default(),
            assertion_stats: Default::default(),
            assertion_details: Vec::new(),
            round_history: history,
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_report(&report);

        // Should contain the ellipsis row
        assert!(formatted.contains("⋮"));

        // First and last rounds should appear
        assert!(formatted.contains("     1"));
        assert!(formatted.contains("    25"));
    }

    #[test]
    fn test_format_report_empty_round_history() {
        let report = ExplorationReport {
            rounds: 0,
            total_branches: 0,
            total_edges: 0,
            bugs: Vec::new(),
            corpus_size: 0,
            coverage_stats: CoverageStats {
                total_edges: 0,
                total_runs: 0,
                edges_per_run_avg: 0.0,
            },
            network_stats: Default::default(),
            assertion_stats: Default::default(),
            assertion_details: Vec::new(),
            round_history: Vec::new(),
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_report(&report);

        // No progress section when history is empty
        assert!(!formatted.contains("Exploration Progress"));
    }

    #[test]
    fn test_format_report_assertion_details() {
        use crate::explorer::{AssertionDetail, AssertionStats};

        let details = vec![
            AssertionDetail {
                id: 100,
                message: "election safety".into(),
                kind: "always".into(),
                guest: "raft".into(),
                category: "invariant".into(),
                verdict: "passed".into(),
                hit_count: 500,
                true_count: 500,
                false_count: 0,
                last_failure_details: None,
            },
            AssertionDetail {
                id: 200,
                message: "log matching".into(),
                kind: "always".into(),
                guest: "raft".into(),
                category: "invariant".into(),
                verdict: "failed".into(),
                hit_count: 300,
                true_count: 290,
                false_count: 10,
                last_failure_details: Some(r#"{"node_id":2,"term":5,"commit_index":3}"#.into()),
            },
            AssertionDetail {
                id: 300,
                message: "value committed".into(),
                kind: "sometimes".into(),
                guest: "redb".into(),
                category: "operation".into(),
                verdict: "unexercised".into(),
                hit_count: 0,
                true_count: 0,
                false_count: 0,
                last_failure_details: None,
            },
            AssertionDetail {
                id: 400,
                message: "split brain".into(),
                kind: "unreachable".into(),
                guest: "raft".into(),
                category: "branch".into(),
                verdict: "passed".into(),
                hit_count: 0,
                true_count: 0,
                false_count: 0,
                last_failure_details: None,
            },
        ];

        let report = ExplorationReport {
            rounds: 5,
            total_branches: 40,
            total_edges: 100,
            bugs: Vec::new(),
            corpus_size: 10,
            coverage_stats: CoverageStats {
                total_edges: 100,
                total_runs: 40,
                edges_per_run_avg: 2.5,
            },
            network_stats: Default::default(),
            assertion_stats: AssertionStats {
                catalog_size: 4,
                passed: 2,
                failed: 1,
                unexercised: 1,
            },
            assertion_details: details,
            round_history: Vec::new(),
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_report(&report);

        // Failed section
        assert!(formatted.contains("Failed Assertions"));
        assert!(formatted.contains("✗"));
        assert!(formatted.contains("log matching"));
        // Failure details are printed as indented JSON
        assert!(formatted.contains("\"node_id\": 2"));
        assert!(formatted.contains("\"term\": 5"));
        assert!(formatted.contains("\"commit_index\": 3"));

        // Unexercised section
        assert!(formatted.contains("Unexercised Assertions"));
        assert!(formatted.contains("○"));
        assert!(formatted.contains("value committed"));

        // Passed section
        assert!(formatted.contains("Passed Assertions"));
        assert!(formatted.contains("✓"));
        assert!(formatted.contains("election safety"));
        assert!(formatted.contains("split brain"));
    }

    #[test]
    fn test_assertion_detail_serialization() {
        use crate::explorer::AssertionDetail;

        let detail = AssertionDetail {
            id: 42,
            message: "safety property".into(),
            kind: "always".into(),
            guest: "raft".into(),
            category: "invariant".into(),
            verdict: "passed".into(),
            hit_count: 1000,
            true_count: 1000,
            false_count: 0,
            last_failure_details: None,
        };

        let json = serde_json::to_string(&detail).unwrap();
        let roundtrip: AssertionDetail = serde_json::from_str(&json).unwrap();

        assert_eq!(roundtrip.id, 42);
        assert_eq!(roundtrip.message, "safety property");
        assert_eq!(roundtrip.hit_count, 1000);
        assert_eq!(roundtrip.verdict, "passed");
    }

    #[test]
    fn test_format_report_no_assertion_details() {
        let report = ExplorationReport {
            rounds: 1,
            total_branches: 8,
            total_edges: 10,
            bugs: Vec::new(),
            corpus_size: 1,
            coverage_stats: CoverageStats {
                total_edges: 10,
                total_runs: 8,
                edges_per_run_avg: 1.25,
            },
            network_stats: Default::default(),
            assertion_stats: Default::default(),
            assertion_details: Vec::new(),
            round_history: Vec::new(),
            wall_clock_seconds: 0.0,
            branches_per_second: 0.0,
            edges_per_second: 0.0,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_report(&report);

        // No assertion detail sections when details are empty
        assert!(!formatted.contains("Failed Assertions"));
        assert!(!formatted.contains("Unexercised Assertions"));
        assert!(!formatted.contains("Passed Assertions"));
    }

    #[test]
    fn test_format_bug_truncates_long_schedule() {
        use chaoscontrol_fault::faults::Fault;
        use chaoscontrol_fault::schedule::ScheduledFault;

        let mut schedule = FaultSchedule::new();
        for i in 0..20 {
            schedule.add(ScheduledFault::new(i * 1000, Fault::NetworkHeal));
        }

        let bug = BugReport {
            bug_id: 1,
            assertion_id: 50,
            assertion_location: "bug.rs:1".to_string(),
            schedule,
            snapshot: None,
            tick: 5000,
            replay_parent_depth: 0,
            dedup_key: 0,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        };

        let formatted = format_bug(&bug);
        assert!(formatted.contains("... and 10 more faults"));
    }

    // ── 6.5: format_campaign_report ─────────────────────────────────

    #[test]
    fn test_format_campaign_report_contains_sections() {
        use crate::campaign::{CampaignBug, CampaignReport, SeedSummary};
        use crate::checkpoint::{SerializableBug, SerializableSchedule};
        use crate::explorer::{AssertionDetail, AssertionStats};

        let report = CampaignReport {
            seeds_run: vec![42, 43, 44],
            seeds_with_bugs: vec![42],
            total_rounds: 30,
            total_branches: 240,
            bugs: vec![CampaignBug {
                bug: SerializableBug {
                    bug_id: 0,
                    assertion_id: 100,
                    assertion_location: "safety.rs:10".into(),
                    schedule: SerializableSchedule { faults: Vec::new() },
                    tick: 500,
                    replay_parent_depth: 0,
                    dedup_key: Some(0xAAAA),
                    schedule_variant: None,
                    scenario_config: None,
                    scenario_summary: None,
                },
                found_by_seeds: vec![42, 44],
                first_seed: 42,
                dedup_key: 0xAAAA,
            }],
            per_seed: vec![
                SeedSummary {
                    seed: 42,
                    rounds: 10,
                    total_branches: 80,
                    total_edges: 200,
                    bugs_found: 1,
                    wall_clock_seconds: 25.3,
                    scenario_summary: None,
                },
                SeedSummary {
                    seed: 43,
                    rounds: 10,
                    total_branches: 80,
                    total_edges: 180,
                    bugs_found: 0,
                    wall_clock_seconds: 22.1,
                    scenario_summary: None,
                },
                SeedSummary {
                    seed: 44,
                    rounds: 10,
                    total_branches: 80,
                    total_edges: 190,
                    bugs_found: 1,
                    wall_clock_seconds: 24.0,
                    scenario_summary: None,
                },
            ],
            assertion_details: vec![
                AssertionDetail {
                    id: 100,
                    message: "leader completeness".into(),
                    kind: "always".into(),
                    guest: "raft".into(),
                    category: "invariant".into(),
                    verdict: "failed".into(),
                    hit_count: 300,
                    true_count: 290,
                    false_count: 10,
                    last_failure_details: None,
                },
                AssertionDetail {
                    id: 200,
                    message: "election safety".into(),
                    kind: "always".into(),
                    guest: "raft".into(),
                    category: "invariant".into(),
                    verdict: "passed".into(),
                    hit_count: 500,
                    true_count: 500,
                    false_count: 0,
                    last_failure_details: None,
                },
            ],
            assertion_stats: AssertionStats {
                catalog_size: 2,
                passed: 1,
                failed: 1,
                unexercised: 0,
            },
            wall_clock_seconds: 26.0,
            failed_seeds: Vec::new(),
            scenario_config: None,
        };

        let formatted = format_campaign_report(&report);

        // Campaign header
        assert!(formatted.contains("Campaign Report"));

        // Per-seed table
        assert!(formatted.contains("Per-Seed Results"));
        assert!(formatted.contains("42"));
        assert!(formatted.contains("43"));
        assert!(formatted.contains("44"));

        // Bug section
        assert!(formatted.contains("safety.rs:10"));
        assert!(formatted.contains("42, 44")); // found_by_seeds

        // Assertion verdicts
        assert!(formatted.contains("leader completeness"));
        assert!(formatted.contains("election safety"));
        assert!(formatted.contains("Failed Assertions"));
        assert!(formatted.contains("Passed Assertions"));
    }

    #[test]
    fn test_format_campaign_report_no_bugs() {
        use crate::campaign::CampaignReport;

        let report = CampaignReport {
            seeds_run: vec![42],
            seeds_with_bugs: Vec::new(),
            total_rounds: 10,
            total_branches: 80,
            bugs: Vec::new(),
            per_seed: Vec::new(),
            assertion_details: Vec::new(),
            assertion_stats: Default::default(),
            wall_clock_seconds: 5.0,
            failed_seeds: Vec::new(),
            scenario_config: None,
        };

        let formatted = format_campaign_report(&report);
        assert!(formatted.contains("No Bugs Found"));
        assert!(formatted.contains("Unique bugs found:      0"));
    }
}
