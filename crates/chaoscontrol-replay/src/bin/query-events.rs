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
#[command(version)]
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
            ensure_distinct_outputs(&output, &receipt)?;
            let mut outputs = vec![("result", output.as_path())];
            if let Some(receipt) = receipt.as_deref() {
                outputs.push(("receipt", receipt));
            }
            ensure_outputs_do_not_overwrite_inputs(
                &outputs,
                &[
                    ("events", events.as_deref()),
                    ("recording", recording.as_deref()),
                    ("dlog", dlog.as_deref()),
                    ("markers", markers.as_deref()),
                    ("query", Some(query.as_path())),
                    ("limits", limits.as_deref()),
                ],
            )?;
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
            ensure_distinct_outputs(&output, &receipt)?;
            let mut outputs = vec![("rendered", output.as_path())];
            if let Some(receipt) = receipt.as_deref() {
                outputs.push(("receipt", receipt));
            }
            ensure_outputs_do_not_overwrite_inputs(&outputs, &[("facts", Some(facts.as_path()))])?;
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

fn same_output_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (std::path::absolute(left), std::path::absolute(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn ensure_distinct_outputs(output: &Path, receipt: &Option<PathBuf>) -> Result<(), String> {
    if receipt
        .as_deref()
        .is_some_and(|receipt| same_output_path(output, receipt))
    {
        return Err(String::from(
            "receipt path must differ from the rendered output path",
        ));
    }
    Ok(())
}

fn ensure_outputs_do_not_overwrite_inputs(
    outputs: &[(&str, &Path)],
    inputs: &[(&str, Option<&Path>)],
) -> Result<(), String> {
    for (output_label, output) in outputs {
        for (input_label, input) in inputs {
            if input.is_some_and(|input| same_output_path(output, input)) {
                return Err(format!(
                    "{output_label} path must differ from the {input_label} input path"
                ));
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
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(usize::MAX)
        .min(usize::try_from(limit).unwrap_or(usize::MAX));
    let mut bytes = Vec::with_capacity(capacity);
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
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(format!("{}: output is not a regular file", path.display()))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{}: {error}", path.display())),
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let directory =
        std::fs::File::open(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("{}: {error}", parent.display()))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| format!("{}: {}", path.display(), error.error))?;
    directory
        .sync_all()
        .map_err(|error| format!("{}: {error}", parent.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_exposes_a_version() {
        use clap::CommandFactory;
        assert!(Cli::command().get_version().is_some());
    }

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
    fn outputs_must_not_overwrite_inputs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("artifact.json");
        assert!(ensure_outputs_do_not_overwrite_inputs(
            &[("result", path.as_path())],
            &[("events", Some(path.as_path()))],
        )
        .is_err());
        let indirect = temp.path().join(".").join("artifact.json");
        assert!(ensure_outputs_do_not_overwrite_inputs(
            &[("result", path.as_path())],
            &[("events", Some(indirect.as_path()))],
        )
        .is_err());
        let other = temp.path().join("other.json");
        ensure_outputs_do_not_overwrite_inputs(
            &[("result", path.as_path())],
            &[("events", Some(other.as_path()))],
        )
        .unwrap();
    }

    #[test]
    fn timeline_rejects_receipt_overwriting_output() {
        let temp = tempfile::tempdir().expect("tempdir");
        let facts = temp.path().join("facts.json");
        let output = temp.path().join("out.md");
        std::fs::write(
            &facts,
            r#"[{"id":1,"tick":0,"kind":"root","label":"root"}]"#,
        )
        .unwrap();
        assert!(execute(Cli {
            command: Command::Timeline {
                facts: facts.clone(),
                format: TimelineFormat::Markdown,
                output: output.clone(),
                receipt: Some(output.clone()),
            },
        })
        .is_err());
        assert!(!output.exists());

        let indirect = temp.path().join(".").join("out.md");
        assert!(execute(Cli {
            command: Command::Timeline {
                facts,
                format: TimelineFormat::Markdown,
                output,
                receipt: Some(indirect),
            },
        })
        .is_err());
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

    #[cfg(unix)]
    #[test]
    fn query_refuses_a_fifo_output_without_blocking() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let fifo = temp.path().join("fifo");
        let path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(write_bytes(&fifo, b"body").is_err());
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
    fn failed_query_output_leaves_no_temporary_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("out");
        std::fs::create_dir(&target).unwrap();
        let before = std::fs::read_dir(temp.path()).unwrap().count();

        assert!(write_bytes(&target, b"body").is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), before);
    }

    #[test]
    fn query_output_replaces_an_existing_regular_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("out.json");
        std::fs::write(&path, b"old").unwrap();

        write_bytes(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
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
