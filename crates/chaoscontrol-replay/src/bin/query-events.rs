//! Bounded local event query and static timeline tool.

use chaoscontrol_replay::event_query::{
    evaluate, receipt_for, render_timeline_dot, render_timeline_markdown, EventEnvelope,
    EventQuery, EventQueryLimits, TimelineFact,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Parser)]
#[command(about = "Query retained events or render a static timeline")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Evaluate one typed query over event envelopes.
    Query {
        /// JSON array of event envelopes.
        events: PathBuf,
        /// Query JSON file.
        query: PathBuf,
        /// Output result JSON file.
        output: PathBuf,
        /// Optional receipt JSON file.
        #[arg(long)]
        receipt: Option<PathBuf>,
        /// Optional limits JSON file.
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    /// Render a static timeline.
    Timeline {
        /// JSON array of timeline facts.
        facts: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = TimelineFormat::Markdown)]
        format: TimelineFormat,
        /// Output file.
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TimelineFormat {
    Markdown,
    Dot,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("query-events: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Command::Query {
            events,
            query,
            output,
            receipt,
            limits,
        } => {
            let limits = match limits {
                Some(path) => read_json(&path)?,
                None => EventQueryLimits::default(),
            };
            let events: Vec<EventEnvelope> = read_json(&events)?;
            let query: EventQuery = read_json(&query)?;
            let result = evaluate(&query, &events, &limits).map_err(|error| error.to_string())?;
            write_json(&output, &result)?;
            if let Some(receipt_path) = receipt {
                let source_ids: Vec<[u8; 32]> = events
                    .iter()
                    .map(|event| event.source_id)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                let receipt = receipt_for(&query, &source_ids, &result, &limits)
                    .map_err(|error| error.to_string())?;
                write_json(&receipt_path, &receipt)?;
            }
        }
        Command::Timeline {
            facts,
            format,
            output,
        } => {
            let facts: Vec<TimelineFact> = read_json(&facts)?;
            let rendered = match format {
                TimelineFormat::Markdown => render_timeline_markdown(&facts),
                TimelineFormat::Dot => render_timeline_dot(&facts),
            }
            .map_err(|error| error.to_string())?;
            std::fs::write(&output, rendered)
                .map_err(|error| format!("{}: {error}", output.display()))?;
        }
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}
