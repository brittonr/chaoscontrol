//! Format exploration reports for human consumption.

use crate::corpus::BugReport;
use crate::explorer::ExplorationReport;

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
    output.push_str("\n");

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
    output.push_str("\n");

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

    // Bug details
    if !report.bugs.is_empty() {
        output
            .push_str("─── Bugs Found ─────────────────────────────────────────────────────────\n");
        for (i, bug) in report.bugs.iter().enumerate() {
            output.push_str(&format!("\n{}. Bug #{}\n", i + 1, bug.bug_id));
            output.push_str(&format_bug(bug));
            output.push_str("\n");
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

        // Clone and drain to list faults
        let mut sched_clone = bug.schedule.clone();
        sched_clone.reset();
        let mut faults = Vec::new();
        while let Some(time) = sched_clone.next_time() {
            faults.extend(sched_clone.drain_due(time));
        }

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

    output
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
        }
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
        };

        let formatted = format_bug(&bug);
        assert!(formatted.contains("Schedule:     2 faults"));
        assert!(formatted.contains("Fault Schedule:"));
        assert!(formatted.contains("@ 1000ns:"));
        assert!(formatted.contains("@ 2000ns:"));
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
        };

        let formatted = format_bug(&bug);
        assert!(formatted.contains("... and 10 more faults"));
    }
}
