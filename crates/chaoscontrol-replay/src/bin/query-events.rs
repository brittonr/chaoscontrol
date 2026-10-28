//! Bounded local event query and static timeline tool.

use chaoscontrol_protocol::branch_marker::MarkerEvidenceContext;
use chaoscontrol_replay::event_query::{
    envelopes_from_dlog_bytes, envelopes_from_markers, envelopes_from_recording, evaluate,
    receipt_for, render_timeline_dot, render_timeline_markdown, timeline_receipt, validate_receipt,
    validate_timeline_receipt, EventEnvelope, EventQuery, EventQueryLimits, EventQueryReceipt,
    EventQueryResult, TimelineFact, TimelineReceipt, MAX_TIMELINE_BYTES,
};
use clap::{Parser, Subcommand, ValueEnum};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{Read, Write};
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
    /// Validate a timeline receipt against its facts and rendered output.
    ValidateTimeline {
        /// JSON array of timeline facts.
        facts: PathBuf,
        /// Rendered output format.
        #[arg(long, value_enum, default_value_t = TimelineFormat::Markdown)]
        format: TimelineFormat,
        /// Rendered Markdown or DOT file.
        rendered: PathBuf,
        /// Receipt JSON file to validate.
        receipt: PathBuf,
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
        /// Optional receipt JSON file.
        #[arg(long)]
        receipt: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TimelineFormat {
    Markdown,
    Dot,
}

impl TimelineFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Dot => "dot",
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("query-events: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    execute(Cli::parse())
}

fn execute(cli: Cli) -> Result<(), String> {
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
                let receipt = receipt_for(&query, &events, &result, &limits)
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
        Command::ValidateTimeline {
            facts,
            format,
            rendered,
            receipt,
        } => {
            let facts: Vec<TimelineFact> = read_json(&facts)?;
            let rendered = String::from_utf8(read_bytes_with_limit(
                &rendered,
                u64::try_from(MAX_TIMELINE_BYTES).map_err(|_| "timeline bound overflow")?,
            )?)
            .map_err(|_| "rendered timeline is not UTF-8".to_string())?;
            let receipt: TimelineReceipt = read_json(&receipt)?;
            validate_timeline_receipt(&receipt, &facts, format.as_str(), &rendered)
                .map_err(|error| error.to_string())?;
            println!("timeline receipt validated");
        }
        Command::Timeline {
            facts,
            format,
            output,
            receipt,
        } => {
            let facts: Vec<TimelineFact> = read_json(&facts)?;
            let rendered = match format {
                TimelineFormat::Markdown => render_timeline_markdown(&facts),
                TimelineFormat::Dot => render_timeline_dot(&facts),
            }
            .map_err(|error| error.to_string())?;
            let receipt = match receipt {
                Some(path) => Some((
                    path,
                    timeline_receipt(&facts, format.as_str(), &rendered)
                        .map_err(|error| error.to_string())?,
                )),
                None => None,
            };
            write_bytes(&output, rendered.as_bytes())?;
            if let Some((path, receipt)) = receipt {
                write_json(&path, &receipt)?;
            }
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
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    read_bytes_with_limit(path, MAX_INPUT_BYTES)
}

fn read_bytes_with_limit(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.len() > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    Ok(bytes)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    write_json_with_limit(path, value, MAX_INPUT_BYTES)
}

fn write_json_with_limit<T: Serialize>(path: &Path, value: &T, limit: u64) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    write_bytes(path, &bytes)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("{}: {error}", path.display()))
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
            Command::Timeline { .. }
            | Command::Validate { .. }
            | Command::ValidateTimeline { .. } => panic!("expected query command"),
        }
    }

    #[test]
    fn query_and_validate_receipt_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let events = temp.path().join("events.json");
        let query = temp.path().join("query.json");
        let result = temp.path().join("result.json");
        let receipt = temp.path().join("receipt.json");
        std::fs::write(
            &events,
            r#"[{"schema":"chaoscontrol.event-envelope.v1","source_id":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"source_kind":"recording","source_sequence":0,"tick":10,"kind":"assert","text":"one","payload_digest":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]}]"#,
        )
        .unwrap();
        std::fs::write(&query, r#""All""#).unwrap();
        execute(Cli {
            command: Command::Query {
                events: Some(events.clone()),
                recording: None,
                dlog: None,
                markers: None,
                query: query.clone(),
                output: result.clone(),
                receipt: Some(receipt.clone()),
                limits: None,
            },
        })
        .unwrap();
        execute(Cli {
            command: Command::Validate {
                events: Some(events.clone()),
                recording: None,
                dlog: None,
                markers: None,
                query: query.clone(),
                result: result.clone(),
                receipt: receipt.clone(),
                limits: None,
            },
        })
        .unwrap();

        let mut tampered: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
        tampered["rows"] = serde_json::json!(tampered["rows"].as_u64().unwrap() + 1);
        std::fs::write(&receipt, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(execute(Cli {
            command: Command::Validate {
                events: Some(events),
                recording: None,
                dlog: None,
                markers: None,
                query,
                result,
                receipt,
                limits: None,
            },
        })
        .is_err());
    }

    #[test]
    fn timeline_and_validate_timeline_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let facts = temp.path().join("facts.json");
        let rendered = temp.path().join("out.md");
        let receipt = temp.path().join("receipt.json");
        std::fs::write(
            &facts,
            r#"[{"id":1,"tick":0,"kind":"root","label":"root"}]"#,
        )
        .unwrap();
        execute(Cli {
            command: Command::Timeline {
                facts: facts.clone(),
                format: TimelineFormat::Markdown,
                output: rendered.clone(),
                receipt: Some(receipt.clone()),
            },
        })
        .unwrap();
        execute(Cli {
            command: Command::ValidateTimeline {
                facts: facts.clone(),
                format: TimelineFormat::Markdown,
                rendered: rendered.clone(),
                receipt: receipt.clone(),
            },
        })
        .unwrap();

        std::fs::write(&rendered, b"tampered").unwrap();
        assert!(execute(Cli {
            command: Command::ValidateTimeline {
                facts,
                format: TimelineFormat::Markdown,
                rendered,
                receipt,
            },
        })
        .is_err());
    }

    #[test]
    fn timeline_does_not_write_a_receipt_when_the_output_fails() {
        let temp = tempfile::tempdir().expect("tempdir");
        let facts = temp.path().join("facts.json");
        let receipt = temp.path().join("receipt.json");
        std::fs::write(
            &facts,
            r#"[{"id":1,"tick":0,"kind":"root","label":"root"}]"#,
        )
        .unwrap();
        assert!(execute(Cli {
            command: Command::Timeline {
                facts,
                format: TimelineFormat::Markdown,
                output: temp.path().to_path_buf(),
                receipt: Some(receipt.clone()),
            },
        })
        .is_err());
        assert!(!receipt.exists());
    }

    #[test]
    fn json_output_is_rejected_before_creation_over_its_limit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("out.json");
        assert!(write_json_with_limit(&path, &"abcd", 5).is_err());
        assert!(!path.exists());
        write_json_with_limit(&path, &"abcd", 6).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), r#""abcd""#);
    }

    #[test]
    fn query_refuses_a_symlink_input_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("target.json");
        let link = temp.path().join("events.json");
        std::fs::write(&target, b"[]").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(execute(Cli {
            command: Command::Query {
                events: Some(link),
                recording: None,
                dlog: None,
                markers: None,
                query: temp.path().join("query.json"),
                output: temp.path().join("out.json"),
                receipt: None,
                limits: None,
            },
        })
        .is_err());
    }

    #[test]
    fn timeline_refuses_a_symlink_output_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let facts = temp.path().join("facts.json");
        let target = temp.path().join("target.md");
        let link = temp.path().join("out.md");
        std::fs::write(
            &facts,
            r#"[{"id":1,"tick":0,"kind":"root","label":"root"}]"#,
        )
        .unwrap();
        std::fs::write(&target, b"original").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(execute(Cli {
            command: Command::Timeline {
                facts,
                format: TimelineFormat::Markdown,
                output: link,
                receipt: None,
            },
        })
        .is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
    }

    #[test]
    fn bounded_reads_accept_the_limit_and_reject_one_more_byte() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("input");
        std::fs::write(&path, b"abc").expect("write");
        assert_eq!(read_bytes_with_limit(&path, 3).unwrap(), b"abc");
        assert!(read_bytes_with_limit(&path, 2).is_err());
    }

    #[test]
    fn query_requires_query_and_output() {
        assert!(Cli::try_parse_from(["query-events", "query"]).is_err());
        assert!(Cli::try_parse_from(["query-events", "query", "query.json"]).is_err());
    }

    #[test]
    fn validate_timeline_accepts_rendered_receipt() {
        let cli = Cli::try_parse_from([
            "query-events",
            "validate-timeline",
            "--format",
            "markdown",
            "facts.json",
            "out.md",
            "receipt.json",
        ])
        .expect("validate-timeline arguments parse");
        match cli.command {
            Command::ValidateTimeline {
                facts,
                format,
                rendered,
                receipt,
            } => {
                assert_eq!(facts, PathBuf::from("facts.json"));
                assert!(matches!(format, TimelineFormat::Markdown));
                assert_eq!(rendered, PathBuf::from("out.md"));
                assert_eq!(receipt, PathBuf::from("receipt.json"));
            }
            Command::Query { .. } | Command::Validate { .. } | Command::Timeline { .. } => {
                panic!("expected validate-timeline command")
            }
        }
    }

    #[test]
    fn timeline_accepts_a_receipt() {
        let cli = Cli::try_parse_from([
            "query-events",
            "timeline",
            "--format",
            "dot",
            "--receipt",
            "receipt.json",
            "facts.json",
            "out.dot",
        ])
        .expect("timeline arguments parse");
        match cli.command {
            Command::Timeline {
                facts,
                format,
                output,
                receipt,
            } => {
                assert_eq!(facts, PathBuf::from("facts.json"));
                assert!(matches!(format, TimelineFormat::Dot));
                assert_eq!(output, PathBuf::from("out.dot"));
                assert_eq!(receipt, Some(PathBuf::from("receipt.json")));
            }
            Command::Query { .. } | Command::Validate { .. } | Command::ValidateTimeline { .. } => {
                panic!("expected timeline command")
            }
        }
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
            Command::Query { .. } | Command::Timeline { .. } | Command::ValidateTimeline { .. } => {
                panic!("expected validate command")
            }
        }
        assert!(
            Cli::try_parse_from(["query-events", "validate", "query.json", "result.json"]).is_err()
        );
    }
}
