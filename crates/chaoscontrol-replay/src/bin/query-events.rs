//! Bounded local event query and static timeline tool.

use chaoscontrol_protocol::branch_marker::MarkerEvidenceContext;
use chaoscontrol_replay::event_query::{
    envelopes_from_dlog_bytes, envelopes_from_markers, envelopes_from_recording, evaluate,
    receipt_for, render_timeline_dot, render_timeline_markdown, validate_receipt, EventEnvelope,
    EventQuery, EventQueryLimits, EventQueryReceipt, EventQueryResult, TimelineFact,
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
        /// Optional JSON array of event envelopes.
        #[arg(long)]
        events: Option<PathBuf>,
        /// Optional retained recording JSON file.
        #[arg(long)]
        recording: Option<PathBuf>,
        /// Optional raw dlog file.
        #[arg(long)]
        dlog: Option<PathBuf>,
        /// Optional JSON array of retained branch-marker evidence.
        #[arg(long)]
        markers: Option<PathBuf>,
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
    /// Validate a query result and receipt against the admitted inputs.
    Validate {
        /// Optional JSON array of event envelopes.
        #[arg(long)]
        events: Option<PathBuf>,
        /// Optional retained recording JSON file.
        #[arg(long)]
        recording: Option<PathBuf>,
        /// Optional raw dlog file.
        #[arg(long)]
        dlog: Option<PathBuf>,
        /// Optional JSON array of retained branch-marker evidence.
        #[arg(long)]
        markers: Option<PathBuf>,
        /// Query JSON file.
        query: PathBuf,
        /// Result JSON file to validate.
        result: PathBuf,
        /// Receipt JSON file to validate.
        receipt: PathBuf,
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
            recording,
            dlog,
            markers,
            query,
            output,
            receipt,
            limits,
        } => {
            let limits = read_limits(limits)?;
            let events = load_events(events, recording, dlog, markers, &limits)?;
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
        Command::Validate {
            events,
            recording,
            dlog,
            markers,
            query,
            result,
            receipt,
            limits,
        } => {
            let limits = read_limits(limits)?;
            let events = load_events(events, recording, dlog, markers, &limits)?;
            let query: EventQuery = read_json(&query)?;
            let result: EventQueryResult = read_json(&result)?;
            let receipt: EventQueryReceipt = read_json(&receipt)?;
            validate_receipt(&receipt, &query, &events, &result, &limits)
                .map_err(|error| error.to_string())?;
            println!("receipt validated");
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

fn read_limits(path: Option<PathBuf>) -> Result<EventQueryLimits, String> {
    match path {
        Some(path) => read_json(&path),
        None => Ok(EventQueryLimits::default()),
    }
}

fn load_events(
    events: Option<PathBuf>,
    recording: Option<PathBuf>,
    dlog: Option<PathBuf>,
    markers: Option<PathBuf>,
    limits: &EventQueryLimits,
) -> Result<Vec<EventEnvelope>, String> {
    match (events, recording, dlog, markers) {
        (Some(path), None, None, None) => read_json(&path),
        (None, Some(path), None, None) => {
            let recording = chaoscontrol_replay::serialize::load_recording(&path)
                .map_err(|error| error.to_string())?;
            envelopes_from_recording(&recording).map_err(|error| error.to_string())
        }
        (None, None, Some(path), None) => {
            let bytes = read_bytes(&path)?;
            envelopes_from_dlog_bytes(&bytes, limits.max_scanned_events)
                .map_err(|error| error.to_string())
        }
        (None, None, None, Some(path)) => {
            let markers: Vec<MarkerEvidenceContext> = read_json(&path)?;
            let bytes = serde_json::to_vec(&markers).map_err(|error| error.to_string())?;
            envelopes_from_markers(*blake3::hash(&bytes).as_bytes(), &markers)
                .map_err(|error| error.to_string())
        }
        (None, None, None, None) => {
            Err("pass --recording, --dlog, --events, or --markers".to_string())
        }
        _ => Err("pass exactly one input source".to_string()),
    }
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

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_accepts_optional_events_input() {
        let cli = Cli::try_parse_from([
            "query-events",
            "query",
            "--events",
            "events.json",
            "query.json",
            "out.json",
        ])
        .expect("query arguments parse");
        match cli.command {
            Command::Query {
                events,
                query,
                output,
                recording,
                dlog,
                markers,
                ..
            } => {
                assert_eq!(events, Some(PathBuf::from("events.json")));
                assert_eq!(query, PathBuf::from("query.json"));
                assert_eq!(output, PathBuf::from("out.json"));
                assert_eq!(recording, None);
                assert_eq!(dlog, None);
                assert_eq!(markers, None);
            }
            Command::Timeline { .. } | Command::Validate { .. } => {
                panic!("expected query command")
            }
        }
    }

    #[test]
    fn query_requires_query_and_output() {
        assert!(Cli::try_parse_from(["query-events", "query"]).is_err());
        assert!(Cli::try_parse_from(["query-events", "query", "query.json"]).is_err());
    }

    #[test]
    fn validate_requires_result_and_receipt() {
        let cli = Cli::try_parse_from([
            "query-events",
            "validate",
            "--events",
            "events.json",
            "query.json",
            "result.json",
            "receipt.json",
        ])
        .expect("validate arguments parse");
        match cli.command {
            Command::Validate {
                events,
                query,
                result,
                receipt,
                ..
            } => {
                assert_eq!(events, Some(PathBuf::from("events.json")));
                assert_eq!(query, PathBuf::from("query.json"));
                assert_eq!(result, PathBuf::from("result.json"));
                assert_eq!(receipt, PathBuf::from("receipt.json"));
            }
            Command::Query { .. } | Command::Timeline { .. } => {
                panic!("expected validate command")
            }
        }
        assert!(
            Cli::try_parse_from(["query-events", "validate", "query.json", "result.json"]).is_err()
        );
    }
}
