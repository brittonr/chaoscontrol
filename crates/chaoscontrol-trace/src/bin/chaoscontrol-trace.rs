//! Standalone KVM trace collector for ChaosControl.
//!
//! Attaches to a running ChaosControl VMM process (by PID) and streams
//! KVM trace events in real time. Can also compare two saved traces
//! for determinism verification.
//!
//! # Usage
//!
//! ```bash
//! # Live trace a running VM (requires root)
//! sudo chaoscontrol-trace live --pid 12345
//!
//! # Live trace and save to file
//! sudo chaoscontrol-trace live --pid 12345 --output trace.json
//!
//! # Compare two traces for determinism
//! chaoscontrol-trace verify --trace-a run1.json --trace-b run2.json
//!
//! # Show summary of a trace file
//! chaoscontrol-trace summary --trace run1.json
//! ```

use chaoscontrol_trace::collector::{Collector, CollectorConfig, TraceLog};
use chaoscontrol_trace::events::EventType;
use chaoscontrol_trace::verifier::DeterminismVerifier;
use clap::{Parser, Subcommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "chaoscontrol-trace")]
#[command(about = "eBPF KVM tracing harness for ChaosControl deterministic VMM")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Live trace KVM events for a running VMM process.
    Live {
        /// PID of the ChaosControl VMM process to trace.
        #[arg(short, long)]
        pid: u32,

        /// Save collected trace to a JSON file on exit.
        #[arg(short, long)]
        output: Option<String>,

        /// Filter to specific event types (comma-separated).
        /// Options: exit, entry, pio, mmio, msr, virq, pic, irq, fault, cr, cpuid
        #[arg(short, long)]
        filter: Option<String>,

        /// Maximum number of events to collect (0 = unlimited).
        #[arg(short, long, default_value = "0")]
        max_events: u64,

        /// Don't print events to stdout (only save to file).
        #[arg(short, long)]
        quiet: bool,
    },

    /// Compare two traces for deterministic equivalence.
    Verify {
        /// Path to first trace file (JSON).
        #[arg(long)]
        trace_a: String,

        /// Path to second trace file (JSON).
        #[arg(long)]
        trace_b: String,

        /// Filter comparison to specific event types.
        #[arg(short, long)]
        filter: Option<String>,
    },

    /// Show summary statistics for a trace file.
    Summary {
        /// Path to trace file (JSON).
        #[arg(short, long)]
        trace: String,
    },
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Live {
            pid,
            output,
            filter,
            max_events,
            quiet,
        } => cmd_live(pid, output, filter, max_events, quiet),
        Commands::Verify {
            trace_a,
            trace_b,
            filter,
        } => cmd_verify(trace_a, trace_b, filter),
        Commands::Summary { trace } => cmd_summary(trace),
    }
}

fn cmd_live(
    pid: u32,
    output: Option<String>,
    filter: Option<String>,
    max_events: u64,
    quiet: bool,
) {
    let type_filter = filter.map(|f| parse_event_filter(&f));

    eprintln!("Attaching KVM tracer to PID {}...", pid);
    let config = CollectorConfig::for_pid(pid);
    let mut collector = match Collector::attach(config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to attach: {}", e);
            eprintln!("Hint: This requires root. Run with sudo.");
            std::process::exit(1);
        }
    };
    eprintln!("Attached. Collecting events (Ctrl+C to stop)...");

    // Set up signal handler for SIGINT + SIGTERM
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc_simple(&r);

    // Collect all events (keep them for saving)
    let mut all_events: Vec<chaoscontrol_trace::events::TraceEvent> = Vec::new();
    let mut displayed = 0u64;

    while running.load(Ordering::Relaxed) {
        match collector.poll() {
            Ok(n) => {
                if n > 0 {
                    let events = collector.drain();
                    for event in &events {
                        // Apply type filter for display
                        let passes_filter = match type_filter {
                            Some(ref types) => types.contains(&event.event_type()),
                            None => true,
                        };
                        if passes_filter && !quiet {
                            println!("{}", event);
                            displayed += 1;
                        }
                        if max_events > 0 && displayed >= max_events {
                            running.store(false, Ordering::Relaxed);
                            break;
                        }
                    }
                    // Keep ALL events (unfiltered) for saving
                    all_events.extend(events);
                }
            }
            Err(e) => {
                eprintln!("Poll error: {}", e);
                break;
            }
        }
    }

    // Final poll to drain any remaining events
    let _ = collector.poll();
    all_events.extend(collector.drain());

    eprintln!(
        "\nCollected {} events ({} displayed)",
        all_events.len(),
        displayed,
    );

    // Save trace if requested
    if let Some(path) = output {
        eprintln!("Saving trace to {}", path);
        let log = TraceLog::new(pid, all_events);
        if let Err(e) = log.save(&path) {
            eprintln!("Failed to save trace: {}", e);
            std::process::exit(1);
        }
        eprintln!("Saved {} events", log.len());
    }
}

fn cmd_verify(trace_a: String, trace_b: String, filter: Option<String>) {
    eprintln!("Loading traces...");
    let log_a = match TraceLog::load(&trace_a) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to load {}: {}", trace_a, e);
            std::process::exit(1);
        }
    };
    let log_b = match TraceLog::load(&trace_b) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to load {}: {}", trace_b, e);
            std::process::exit(1);
        }
    };

    eprintln!(
        "Trace A: {} events, Trace B: {} events",
        log_a.len(),
        log_b.len()
    );

    let result = if let Some(ref f) = filter {
        let types = parse_event_filter(f);
        DeterminismVerifier::compare_filtered(&log_a, &log_b, &|e| {
            types.contains(&e.event_type())
        })
    } else {
        DeterminismVerifier::compare(&log_a, &log_b)
    };

    println!("{}", result);

    if !result.is_deterministic {
        std::process::exit(1);
    }
}

fn cmd_summary(trace: String) {
    let log = match TraceLog::load(&trace) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to load {}: {}", trace, e);
            std::process::exit(1);
        }
    };

    println!("Trace: {}", trace);
    println!("PID: {}", log.pid);
    println!("Events: {}", log.len());
    println!("Kernel: {}", log.metadata.kernel_version);
    println!("CPU: {}", log.metadata.cpu_model);
    println!("Started: {}", log.metadata.start_time);
    println!();

    let summary = log.summary();
    let mut sorted: Vec<_> = summary.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    println!("{:>25} {:>10} {:>8}", "Event Type", "Count", "Percent");
    println!("{}", "-".repeat(45));
    let total = log.len() as f64;
    for (name, count) in &sorted {
        let pct = **count as f64 / total * 100.0;
        println!("{:>25} {:>10} {:>7.1}%", name, count, pct);
    }
    println!("{}", "-".repeat(45));
    println!("{:>25} {:>10}", "Total", log.len());

    // Timing info
    if log.len() >= 2 {
        let first_ns = log.events.first().unwrap().host_ns;
        let last_ns = log.events.last().unwrap().host_ns;
        let duration_ms = (last_ns - first_ns) as f64 / 1_000_000.0;
        let rate = log.len() as f64 / (duration_ms / 1_000.0);
        println!();
        println!("Duration: {:.1} ms", duration_ms);
        println!("Event rate: {:.0} events/sec", rate);
    }
}

fn parse_event_filter(filter: &str) -> Vec<EventType> {
    filter
        .split(',')
        .filter_map(|s| match s.trim().to_lowercase().as_str() {
            "exit" => Some(EventType::KvmExit),
            "entry" => Some(EventType::KvmEntry),
            "pio" | "io" => Some(EventType::KvmPio),
            "mmio" => Some(EventType::KvmMmio),
            "msr" => Some(EventType::KvmMsr),
            "virq" | "inj_virq" => Some(EventType::KvmInjVirq),
            "pic" | "pic_irq" => Some(EventType::KvmPicIrq),
            "irq" | "set_irq" => Some(EventType::KvmSetIrq),
            "fault" | "page_fault" => Some(EventType::KvmPageFault),
            "cr" => Some(EventType::KvmCr),
            "cpuid" => Some(EventType::KvmCpuid),
            other => {
                eprintln!("Unknown event type filter: {}", other);
                None
            }
        })
        .collect()
}

/// Signal handler for SIGINT + SIGTERM (avoids pulling in ctrlc crate).
fn ctrlc_simple(running: &Arc<AtomicBool>) {
    let r = running.clone();
    unsafe {
        static mut RUNNING: *const AtomicBool = std::ptr::null();
        RUNNING = Arc::as_ptr(&r);

        extern "C" fn handler(_: libc::c_int) {
            unsafe {
                if !RUNNING.is_null() {
                    (*RUNNING).store(false, Ordering::Relaxed);
                }
            }
        }

        let h = handler as *const () as libc::sighandler_t;
        libc::signal(libc::SIGINT, h);
        libc::signal(libc::SIGTERM, h);
    }
}
