//! Bounded local event query and static timeline tool.

use chaoscontrol_protocol::branch_marker::MarkerEvidenceContext;
use chaoscontrol_replay::event_query::{
    envelopes_from_dlog_bytes, envelopes_from_markers, envelopes_from_recording_source,
    envelopes_from_sdk_lifecycle, evaluate_at_moment, render_workspace_timeline,
    validate_workspace_query_receipt, validate_workspace_timeline_receipt, workspace_query_receipt,
    workspace_timeline_receipt, DlogContext, EventEnvelope, EventQuery, EventQueryLimits,
    EventQueryResult, WorkspaceQueryReceipt, WorkspaceTimelineInput, WorkspaceTimelineReceipt,
    MAX_TIMELINE_BYTES,
};
use chaoscontrol_replay::moment::{
    admit_moment, MomentContext, MomentRequest, MomentSelector, RetainedHistory,
};
use chaoscontrol_replay::session::{load_session, ExactSession, SessionLimits};
use clap::{Parser, Subcommand, ValueEnum};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn emit_receipt<T: Serialize>(value: &T) -> Result<(), String> {
    let mut writer = BoundedJsonOutput {
        bytes: Vec::new(),
        limit: MAX_INPUT_BYTES as usize,
    };
    serde_json::to_writer(&mut writer, value).map_err(|error| error.to_string())?;
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&writer.bytes)
        .and_then(|()| stdout.write_all(b"\n"))
        .map_err(|error| error.to_string())
}

#[path = "query-events/publication.rs"]
mod publication;
#[path = "query-events/workspace.rs"]
mod workspace;

const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Parser)]
#[command(
    about = "Query retained events, render static timelines, or observe an exact restored guest"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Default, clap::Args)]
struct SourceArgs {
    /// Retained canonical event envelopes.
    #[arg(long)]
    events: Option<PathBuf>,
    /// Metadata recording; mutually exclusive with an exact session recording.
    #[arg(long)]
    recording: Option<PathBuf>,
    #[arg(long)]
    dlog: Option<PathBuf>,
    /// Explicit VM/tick anchors; hardware TSC is not a simulation tick.
    #[arg(long, requires = "dlog")]
    dlog_context: Option<PathBuf>,
    #[arg(long)]
    markers: Option<PathBuf>,
    /// Actual retained supervisor lifecycle receipt artifact.
    #[arg(long, requires_all = ["process_manifest", "sdk_vm_index"])]
    sdk_lifecycle: Option<PathBuf>,
    /// Admitted owner manifest for the lifecycle receipt.
    #[arg(long, requires = "sdk_lifecycle")]
    process_manifest: Option<PathBuf>,
    /// Explicit VM owner for the lifecycle receipt.
    #[arg(long, requires = "sdk_lifecycle")]
    sdk_vm_index: Option<u32>,
    /// Exact persisted session, required for future/restored moment admission.
    #[arg(long, conflicts_with = "recording")]
    session: Option<PathBuf>,
    /// Explicit selector and past/future horizons; absent means all retained past.
    #[arg(long)]
    moment: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Query admitted retained sources within one explicit moment horizon.
    Query {
        #[command(flatten)]
        sources: SourceArgs,
        query: PathBuf,
        output: PathBuf,
        /// Write the bounded receipt here; otherwise emit it as JSON on stdout.
        #[arg(long)]
        receipt: Option<PathBuf>,
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    /// Re-admit current artifacts/moment and validate the exact result and receipt.
    Validate {
        #[command(flatten)]
        sources: SourceArgs,
        query: PathBuf,
        result: PathBuf,
        receipt: PathBuf,
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    /// Validate a static workspace timeline and its bound source/horizon receipt.
    ValidateTimeline {
        /// WorkspaceTimelineInput JSON, including query, graph, gaps and limits.
        facts: PathBuf,
        #[arg(long, value_enum, default_value_t = TimelineFormat::Markdown)]
        format: TimelineFormat,
        rendered: PathBuf,
        receipt: PathBuf,
    },
    /// Render a self-contained static workspace timeline or branch graph.
    Timeline {
        facts: PathBuf,
        #[arg(long, value_enum, default_value_t = TimelineFormat::Markdown)]
        format: TimelineFormat,
        output: PathBuf,
        #[arg(long)]
        receipt: Option<PathBuf>,
    },
    /// Build a bound query/timeline input from the actual retained session graph.
    PrepareTimeline {
        #[command(flatten)]
        sources: SourceArgs,
        query: PathBuf,
        output: PathBuf,
        #[arg(long)]
        limits: Option<PathBuf>,
        #[arg(long)]
        timeline_limits: Option<PathBuf>,
        /// Retained frontier trace; missing virtual time/parentage remain explicit gaps.
        #[arg(long)]
        frontier: Option<PathBuf>,
        #[arg(long)]
        missing_artifact: Vec<String>,
    },
    #[command(flatten)]
    Workspace(workspace::WorkspaceCommand),
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
            sources,
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
            let mut inputs = sources.input_paths();
            inputs.push(("query", Some(query.as_path())));
            inputs.push(("limits", limits.as_deref()));
            ensure_outputs_do_not_overwrite_inputs(&outputs, &inputs)?;
            sources.reject_session_outputs(&outputs)?;
            let publication =
                publication::Publications::admit(&outputs, sources.session.as_deref())?;
            let limits = read_limits(limits)?;
            let (events, moment, _) = load_workspace_inputs(&sources, &limits)?;
            let query: EventQuery = read_json(&query)?;
            let result = evaluate_at_moment(&query, &events, &limits, &moment)
                .map_err(|error| error.to_string())?;
            let evidence = workspace_query_receipt(&query, &events, &result, &limits, &moment)
                .map_err(|error| error.to_string())?;
            publication.json(&output, &result, MAX_INPUT_BYTES)?;
            if let Some(path) = receipt {
                publication.json(&path, &evidence, MAX_INPUT_BYTES)?;
            } else {
                emit_receipt(&evidence)?;
            }
        }
        Command::Validate {
            sources,
            query,
            result,
            receipt,
            limits,
        } => {
            let limits = read_limits(limits)?;
            let (events, moment, _) = load_workspace_inputs(&sources, &limits)?;
            let query: EventQuery = read_json(&query)?;
            let result: EventQueryResult = read_json(&result)?;
            let receipt: WorkspaceQueryReceipt = read_json(&receipt)?;
            validate_workspace_query_receipt(&receipt, &query, &events, &result, &limits, &moment)
                .map_err(|error| error.to_string())?;
            println!("workspace query receipt validated");
        }
        Command::ValidateTimeline {
            facts,
            format,
            rendered,
            receipt,
        } => {
            let input: WorkspaceTimelineInput = read_json(&facts)?;
            let rendered =
                String::from_utf8(read_bytes_with_limit(&rendered, MAX_TIMELINE_BYTES as u64)?)
                    .map_err(|_| "rendered timeline is not UTF-8".to_string())?;
            let receipt: WorkspaceTimelineReceipt = read_json(&receipt)?;
            validate_workspace_timeline_receipt(&receipt, &input, format.as_str(), &rendered)
                .map_err(|error| error.to_string())?;
            println!("workspace timeline receipt validated");
        }
        Command::Timeline {
            facts,
            format,
            output,
            receipt,
        } => {
            ensure_distinct_outputs(&output, &receipt)?;
            let mut outputs = vec![("rendered", output.as_path())];
            if let Some(path) = receipt.as_deref() {
                outputs.push(("receipt", path));
            }
            ensure_outputs_do_not_overwrite_inputs(
                &outputs,
                &[("workspace input", Some(facts.as_path()))],
            )?;
            let publication = publication::Publications::admit(&outputs, None)?;
            let input: WorkspaceTimelineInput = read_json(&facts)?;
            let rendered = render_workspace_timeline(&input, format.as_str())
                .map_err(|error| error.to_string())?;
            let evidence = workspace_timeline_receipt(&input, format.as_str(), &rendered)
                .map_err(|error| error.to_string())?;
            publication.bytes(&output, rendered.as_bytes())?;
            if let Some(path) = receipt {
                publication.json(&path, &evidence, MAX_INPUT_BYTES)?;
            } else {
                emit_receipt(&evidence)?;
            }
        }
        Command::PrepareTimeline {
            sources,
            query,
            output,
            limits,
            timeline_limits,
            frontier,
            missing_artifact,
        } => {
            let outputs = [("workspace timeline input", output.as_path())];
            let mut inputs = sources.input_paths();
            inputs.extend([
                ("query", Some(query.as_path())),
                ("query limits", limits.as_deref()),
                ("timeline limits", timeline_limits.as_deref()),
            ]);
            inputs.push(("frontier trace", frontier.as_deref()));
            ensure_outputs_do_not_overwrite_inputs(&outputs, &inputs)?;
            sources.reject_session_outputs(&outputs)?;
            let publication =
                publication::Publications::admit(&outputs, sources.session.as_deref())?;
            let limits = read_limits(limits)?;
            let (events, mut moment, mut facts) = load_workspace_inputs(&sources, &limits)?;
            let timeline_limits: chaoscontrol_replay::event_query::TimelineLimits =
                match timeline_limits {
                    Some(path) => read_json(&path)?,
                    None => chaoscontrol_replay::event_query::TimelineLimits::default(),
                };
            let mut gaps = missing_artifact;
            let mut source_truncated = false;
            if let Some(path) = frontier {
                let bytes = read_bytes(&path)?;
                let projection =
                    chaoscontrol_explore::frontier_timeline::timeline_from_frontier_trace(
                        &bytes,
                        &timeline_limits,
                    )
                    .map_err(|error| error.to_string())?;
                moment.source_ids.push(format!(
                    "b3:{}",
                    blake3::Hash::from(projection.source_id).to_hex()
                ));
                moment.source_ids.sort();
                moment.source_ids.dedup();
                source_truncated = projection.truncated;
                facts.extend(projection.facts);
                gaps.extend(projection.missing_artifacts);
            }
            let query: EventQuery = read_json(&query)?;
            let result = evaluate_at_moment(&query, &events, &limits, &moment)
                .map_err(|error| error.to_string())?;
            let evidence = workspace_query_receipt(&query, &events, &result, &limits, &moment)
                .map_err(|error| error.to_string())?;
            let input = WorkspaceTimelineInput {
                facts,
                events,
                query_receipt: evidence.query,
                query_result: result,
                moment,
                missing_artifacts: gaps,
                source_truncated,
                limits: timeline_limits,
                claim: chaoscontrol_replay::event_query::TimelineClaim::RetainedFactsOnly,
            };
            render_workspace_timeline(&input, "markdown").map_err(|error| error.to_string())?;
            publication.json(&output, &input, MAX_INPUT_BYTES)?;
        }
        Command::Workspace(command) => workspace::execute(command)?,
    }
    Ok(())
}

fn same_output_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (
        publication::resolved_entry(left),
        publication::resolved_entry(right),
    ) {
        (Some(left), Some(right)) => left == right,
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

impl SourceArgs {
    fn input_paths(&self) -> Vec<(&'static str, Option<&Path>)> {
        vec![
            ("events", self.events.as_deref()),
            ("recording", self.recording.as_deref()),
            ("dlog", self.dlog.as_deref()),
            ("dlog context", self.dlog_context.as_deref()),
            ("markers", self.markers.as_deref()),
            ("SDK lifecycle", self.sdk_lifecycle.as_deref()),
            ("process manifest", self.process_manifest.as_deref()),
            ("moment", self.moment.as_deref()),
        ]
    }

    fn reject_session_outputs(&self, outputs: &[(&str, &Path)]) -> Result<(), String> {
        if let Some(session) = &self.session {
            reject_session_outputs(session, outputs)?;
        }
        Ok(())
    }
}

fn reject_session_outputs(session: &Path, outputs: &[(&str, &Path)]) -> Result<(), String> {
    let root = session.canonicalize().map_err(|error| error.to_string())?;
    for (_, output) in outputs {
        let parent = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent.canonicalize().map_err(|error| error.to_string())?;
        if parent.starts_with(&root) {
            return Err(
                "query/observation output must not modify its exact session artifacts".into(),
            );
        }
    }
    Ok(())
}

struct LoadedSources {
    events: Vec<EventEnvelope>,
    identities: std::collections::BTreeSet<[u8; 32]>,
    recording: Option<chaoscontrol_replay::recording::Recording>,
}

impl LoadedSources {
    fn add_identity(&mut self, identity: [u8; 32]) -> Result<(), String> {
        self.identities.insert(identity);
        if self.identities.len() > chaoscontrol_replay::moment::MAX_MOMENT_SOURCES {
            return Err("retained source identity inventory exceeds its bound".into());
        }
        Ok(())
    }
    fn read(&mut self, path: &Path, bound: u64) -> Result<Vec<u8>, String> {
        let bytes = read_bytes_with_limit(path, bound)?;
        self.add_identity(*blake3::hash(&bytes).as_bytes())?;
        Ok(bytes)
    }

    fn append(
        &mut self,
        rows: Vec<EventEnvelope>,
        limits: &EventQueryLimits,
    ) -> Result<(), String> {
        if self
            .events
            .len()
            .checked_add(rows.len())
            .is_none_or(|count| count > limits.max_scanned_events)
        {
            return Err("combined source event scan bound exceeded".into());
        }
        for row in &rows {
            self.add_identity(row.source_id)?;
        }
        self.events.extend(rows);
        Ok(())
    }
}

fn source_digest(identity: &str) -> Result<[u8; 32], String> {
    let hex = identity
        .strip_prefix("b3:")
        .ok_or("source identity is not BLAKE3")?;
    Ok(*blake3::Hash::from_hex(hex)
        .map_err(|error| error.to_string())?
        .as_bytes())
}

fn load_workspace_inputs(
    sources: &SourceArgs,
    limits: &EventQueryLimits,
) -> Result<
    (
        Vec<EventEnvelope>,
        MomentContext,
        Vec<chaoscontrol_replay::event_query::TimelineFact>,
    ),
    String,
> {
    limits.validate().map_err(|error| error.to_string())?;
    if sources.session.is_some() && sources.recording.is_some() {
        return Err(
            "an exact session cannot be combined with a different metadata recording".into(),
        );
    }
    if sources.dlog.is_some() != sources.dlog_context.is_some() {
        return Err("--dlog requires --dlog-context with exact VM/tick anchors".into());
    }
    let mut loaded = LoadedSources {
        events: Vec::new(),
        identities: std::collections::BTreeSet::new(),
        recording: None,
    };
    let session: Option<ExactSession> = sources
        .session
        .as_ref()
        .map(|path| load_session(path, SessionLimits::default()).map_err(|error| error.to_string()))
        .transpose()?;
    if let Some(session) = &session {
        loaded.add_identity(source_digest(session.recording_identity())?)?;
        loaded.add_identity(source_digest(&session.manifest().session_id)?)?;
        loaded.append(
            envelopes_from_recording_source(
                session.recording(),
                source_digest(session.recording_identity())?,
            )
            .map_err(|error| error.to_string())?,
            limits,
        )?;
    }
    if let Some(path) = &sources.events {
        let bytes = loaded.read(path, MAX_INPUT_BYTES)?;
        loaded.append(
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?,
            limits,
        )?;
    }
    if let Some(path) = &sources.recording {
        let bytes = loaded.read(
            path,
            chaoscontrol_replay::serialize::MAX_RECORDING_JSON_BYTES as u64,
        )?;
        let recording = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        chaoscontrol_replay::recording::validate_recording(&recording)
            .map_err(|error| format!("invalid recording: {error:?}"))?;
        loaded.append(
            envelopes_from_recording_source(&recording, *blake3::hash(&bytes).as_bytes())
                .map_err(|error| error.to_string())?,
            limits,
        )?;
        loaded.recording = Some(recording);
    }
    if let (Some(path), Some(context)) = (&sources.dlog, &sources.dlog_context) {
        let bytes = loaded.read(path, MAX_INPUT_BYTES)?;
        let context_bytes = loaded.read(context, MAX_INPUT_BYTES)?;
        let context: DlogContext =
            serde_json::from_slice(&context_bytes).map_err(|error| error.to_string())?;
        loaded.append(
            envelopes_from_dlog_bytes(&bytes, limits.max_scanned_events, &context)
                .map_err(|error| error.to_string())?,
            limits,
        )?;
    }
    if let Some(path) = &sources.markers {
        let bytes = loaded.read(path, MAX_INPUT_BYTES)?;
        let markers: Vec<MarkerEvidenceContext> =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        loaded.append(
            envelopes_from_markers(*blake3::hash(&bytes).as_bytes(), &markers)
                .map_err(|error| error.to_string())?,
            limits,
        )?;
    }
    if let Some(path) = &sources.sdk_lifecycle {
        let bytes = loaded.read(path, MAX_INPUT_BYTES)?;
        let manifest_path = sources
            .process_manifest
            .as_deref()
            .ok_or("--sdk-lifecycle requires --process-manifest")?;
        let vm_index = sources
            .sdk_vm_index
            .ok_or("--sdk-lifecycle requires --sdk-vm-index")?;
        let manifest_bytes = loaded.read(manifest_path, MAX_INPUT_BYTES)?;
        let manifest =
            serde_json::from_slice(&manifest_bytes).map_err(|error| error.to_string())?;
        let receipt = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        loaded.append(
            envelopes_from_sdk_lifecycle(
                *blake3::hash(&bytes).as_bytes(),
                vm_index,
                &manifest,
                &receipt,
            )
            .map_err(|error| error.to_string())?,
            limits,
        )?;
    }
    if loaded.identities.is_empty() {
        return Err("provide at least one retained source or exact session".into());
    }
    let identities = loaded
        .identities
        .into_iter()
        .map(|id| format!("b3:{}", blake3::Hash::from(id).to_hex()))
        .collect();
    let history = if let Some(session) = &session {
        RetainedHistory::from_session(session, identities).map_err(|error| error.to_string())?
    } else if let Some(recording) = &loaded.recording {
        RetainedHistory::from_recording(recording, identities).map_err(|error| error.to_string())?
    } else {
        RetainedHistory {
            source_ids: identities,
            start_tick: loaded
                .events
                .iter()
                .map(|event| event.tick)
                .min()
                .unwrap_or(0),
            end_tick: loaded
                .events
                .iter()
                .map(|event| event.tick)
                .max()
                .unwrap_or(0),
            checkpoints: Vec::new(),
            branches: Vec::new(),
        }
    };
    let request = match &sources.moment {
        Some(path) => read_json(path)?,
        None => MomentRequest {
            selector: MomentSelector::Tick(history.end_tick),
            past_ticks: history
                .end_tick
                .checked_sub(history.start_tick)
                .ok_or("invalid retained horizon")?,
            future_ticks: 0,
            requires_restored_state: false,
        },
    };
    let moment =
        admit_moment(&request, &history, session.as_ref()).map_err(|error| error.to_string())?;
    let facts = session
        .as_ref()
        .map(ExactSession::branch_graph_facts)
        .unwrap_or_default();
    Ok((loaded.events, moment, facts))
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
    if bytes.len() as u64 != metadata.len() {
        return Err(format!("{} changed while reading", path.display()));
    }
    Ok(bytes)
}

#[cfg(test)]
fn write_json_with_limit<T: Serialize>(path: &Path, value: &T, limit: u64) -> Result<(), String> {
    let limit = usize::try_from(limit).map_err(|_| "JSON output bound is not addressable")?;
    let mut writer = BoundedJsonOutput {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    write_bytes(path, &writer.bytes)
}

struct BoundedJsonOutput {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedJsonOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length > self.limit)
        {
            return Err(std::io::Error::other("JSON output byte bound exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    publication::Publications::admit(&[("output", path)], None)?.bytes(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_timeline_input(path: &Path) {
        use chaoscontrol_replay::event_query::{
            TimelineClaim, TimelineFact, TimelineLimits, EVENT_ENVELOPE_SCHEMA,
        };
        let source = [1; 32];
        let events = vec![EventEnvelope {
            schema: EVENT_ENVELOPE_SCHEMA.to_string(),
            source_id: source,
            source_kind: chaoscontrol_replay::event_query::EventSourceKind::Recording,
            source_sequence: 0,
            tick: 0,
            vm_index: Some(0),
            process: None,
            thread: None,
            kind: "root".into(),
            text: "root".into(),
            payload_digest: *blake3::hash(b"root").as_bytes(),
            details: None,
            derived: std::collections::BTreeMap::new(),
        }];
        let moment = MomentContext {
            tick: 0,
            start_tick: 0,
            end_tick: 0,
            retained_start_tick: 0,
            retained_end_tick: 0,
            source_ids: vec![format!("b3:{}", blake3::Hash::from(source).to_hex())],
            session_id: None,
            checkpoint_id: None,
            branch_id: None,
        };
        let query = EventQuery::All;
        let limits = EventQueryLimits::default();
        let result = evaluate_at_moment(&query, &events, &limits, &moment).unwrap();
        let evidence = workspace_query_receipt(&query, &events, &result, &limits, &moment).unwrap();
        let input = WorkspaceTimelineInput {
            facts: vec![TimelineFact {
                id: 1,
                parent: None,
                tick: 0,
                kind: "root".into(),
                label: "root".into(),
            }],
            events,
            query_receipt: evidence.query,
            query_result: result,
            moment,
            missing_artifacts: Vec::new(),
            limits: TimelineLimits::default(),
            claim: TimelineClaim::RetainedFactsOnly,
            source_truncated: false,
        };
        std::fs::write(path, serde_json::to_vec(&input).unwrap()).unwrap();
    }

    #[test]
    fn input_reads_reject_length_changes() {
        assert!(read_bytes(Path::new("/proc/self/cmdline")).is_err());
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
            r#"[{"schema":"chaoscontrol.event-envelope.v1","source_id":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"source_kind":"recording","source_sequence":0,"tick":10,"vm_index":0,"kind":"assert","text":"one","payload_digest":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]}]"#,
        )
        .unwrap();
        std::fs::write(&query, r#""All""#).unwrap();
        execute(Cli {
            command: Command::Query {
                sources: SourceArgs {
                    events: Some(events.clone()),
                    ..SourceArgs::default()
                },
                query: query.clone(),
                output: result.clone(),
                receipt: Some(receipt.clone()),
                limits: None,
            },
        })
        .unwrap();
        execute(Cli {
            command: Command::Validate {
                sources: SourceArgs {
                    events: Some(events.clone()),
                    ..SourceArgs::default()
                },
                query: query.clone(),
                result: result.clone(),
                receipt: receipt.clone(),
                limits: None,
            },
        })
        .unwrap();

        let mut tampered: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
        tampered["query"]["rows"] =
            serde_json::json!(tampered["query"]["rows"].as_u64().unwrap() + 1);
        std::fs::write(&receipt, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(execute(Cli {
            command: Command::Validate {
                sources: SourceArgs {
                    events: Some(events),
                    ..SourceArgs::default()
                },
                query,
                result,
                receipt,
                limits: None,
            },
        })
        .is_err());
    }

    // Reproduce the retained bootstrap shape: empty rounds before and after
    // four CPU-throttle stages at tick 1171, with the horizon at tick 1210.
    fn bootstrap_recording() -> chaoscontrol_replay::recording::Recording {
        use chaoscontrol_fault::faults::Fault;
        use chaoscontrol_fault::outcomes::{
            fault_run_id, transition_fault_outcome, FaultAttempt, FaultAttemptSource,
            FaultObservation, FaultObservationEffect, FaultObservationSubsystem,
            FaultOutcomeLedger, FaultPlanEffect, FaultStageKind, NANOSECONDS_PER_SIMULATION_TICK,
        };
        use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
        use chaoscontrol_replay::recording::{RecordedEvent, Recorder, RecordingConfig};
        use chaoscontrol_vmm::controller::RoundResult;

        const SEED: u64 = 42;
        const RUN_SEQUENCE: u64 = 1;
        const SELECTION_TICK: u64 = 1171;
        const HORIZON: u64 = 1210;
        const DURATION_TICKS: u64 = 5;
        let fault = Fault::CpuThrottle {
            target: 0,
            budget_units: 20,
            duration_ticks: DURATION_TICKS,
        };
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(0, fault.clone()));
        let schedule_id = schedule.identity();
        let attempt = FaultAttempt::new_with_source(
            fault_run_id(SEED, RUN_SEQUENCE, schedule_id),
            RUN_SEQUENCE,
            schedule_id,
            0,
            SELECTION_TICK * NANOSECONDS_PER_SIMULATION_TICK,
            FaultAttemptSource::Scheduled {
                entry_index: 0,
                scheduled_at_ns: 0,
            },
            fault.clone(),
        );
        let effect = FaultPlanEffect::CpuThrottle {
            target: 0,
            budget_units: 20,
            release_at_tick: SELECTION_TICK + DURATION_TICKS,
        };
        let mut recorder = Recorder::new_for_run(
            RecordingConfig {
                num_vms: 1,
                vm_memory_size: 128 * 1024 * 1024,
                tsc_khz: 3_000_000,
                kernel_path: "/test/vmlinux".into(),
                initrd_path: None,
                quantum: 100,
                checkpoint_interval: 20,
                disk_image_path: None,
                random_faults: false,
            },
            schedule,
            SEED,
            RUN_SEQUENCE,
        );
        let mut ledger = FaultOutcomeLedger::default();
        for tick in 1..=HORIZON {
            let mut faults_fired = Vec::new();
            let mut fault_outcomes = Vec::new();
            if tick == SELECTION_TICK {
                ledger = transition_fault_outcome(
                    &ledger,
                    Some(&attempt),
                    attempt.id,
                    FaultStageKind::Selected,
                )
                .unwrap();
                for kind in [
                    FaultStageKind::Applicable {
                        effect: effect.clone(),
                    },
                    FaultStageKind::Applied {
                        effect: effect.clone(),
                    },
                    FaultStageKind::Observed {
                        observation: FaultObservation::new(
                            attempt.id,
                            FaultObservationSubsystem::Cpu,
                            0,
                            FaultObservationEffect::CpuThrottleActivated,
                        ),
                    },
                ] {
                    ledger = transition_fault_outcome(&ledger, None, attempt.id, kind).unwrap();
                }
                faults_fired.push(fault.clone());
                fault_outcomes = ledger.events.clone();
            }
            recorder
                .record_round(
                    &RoundResult {
                        tick,
                        vms_running: 1,
                        vms_halted: 0,
                        faults_fired,
                        fault_outcomes,
                        messages_delivered: 0,
                        schedule_traces: Vec::new(),
                    },
                    &ledger,
                )
                .unwrap();
            if tick == 1 {
                recorder.record_event(RecordedEvent::SerialOutput {
                    tick,
                    vm_index: 0,
                    data: "[    0.000000] Linux version 6.19.0 (nixbld@".into(),
                });
            }
        }
        recorder.recording().clone()
    }

    #[test]
    fn query_bootstrap_empty_rounds_preserves_all_stages_and_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let recording_path = temp.path().join("recording.json");
        let query = temp.path().join("query.json");
        let result = temp.path().join("result.json");
        let receipt = temp.path().join("receipt.json");
        let recording = bootstrap_recording();
        chaoscontrol_replay::recording::validate_recording(&recording).unwrap();
        let bytes = serde_json::to_vec(&recording).unwrap();
        let source_id = *blake3::hash(&bytes).as_bytes();
        std::fs::write(&recording_path, bytes).unwrap();
        std::fs::write(&query, r#""All""#).unwrap();
        let sources = SourceArgs {
            recording: Some(recording_path),
            ..SourceArgs::default()
        };
        execute(Cli {
            command: Command::Query {
                sources: sources.clone(),
                query: query.clone(),
                output: result.clone(),
                receipt: Some(receipt.clone()),
                limits: None,
            },
        })
        .unwrap();
        let rows: EventQueryResult = read_json(&result).unwrap();
        assert_eq!(rows.scanned, 6);
        assert_eq!(rows.matched, 6);
        assert_eq!(rows.rows.len(), 6);
        assert!(!rows.truncated);
        assert_eq!(rows.rows[0].kind, "serial_output");
        for (index, kind) in ["selected", "applicable", "applied", "observed"]
            .into_iter()
            .enumerate()
        {
            let row = &rows.rows[index + 2];
            assert_eq!(row.kind, format!("fault_stage_{kind}"));
            assert_eq!(row.tick, 1171);
            assert_eq!(row.source_id, source_id);
            assert_eq!(row.source_sequence, index as u64 + 2);
            assert_eq!(row.process.as_deref(), Some("fault_engine"));
            assert_eq!(
                row.payload_digest,
                *blake3::hash(&serde_json::to_vec(&recording.fault_stage_events[index]).unwrap())
                    .as_bytes()
            );
        }
        execute(Cli {
            command: Command::Validate {
                sources,
                query,
                result,
                receipt,
                limits: None,
            },
        })
        .unwrap();
    }

    #[test]
    fn query_bootstrap_rejects_reversed_empty_round_without_publication() {
        let temp = tempfile::tempdir().unwrap();
        let recording_path = temp.path().join("recording.json");
        let query = temp.path().join("query.json");
        let result = temp.path().join("result.json");
        let receipt = temp.path().join("receipt.json");
        let mut recording = bootstrap_recording();
        recording.fault_round_deltas[1171].event_end = 3;
        std::fs::write(&recording_path, serde_json::to_vec(&recording).unwrap()).unwrap();
        std::fs::write(&query, r#""All""#).unwrap();
        let error = execute(Cli {
            command: Command::Query {
                sources: SourceArgs {
                    recording: Some(recording_path),
                    ..SourceArgs::default()
                },
                query,
                output: result.clone(),
                receipt: Some(receipt.clone()),
                limits: None,
            },
        })
        .unwrap_err();
        assert!(error.contains("InvalidRoundDelta"), "{error}");
        assert!(!result.exists());
        assert!(!receipt.exists());
    }

    #[test]
    fn timeline_and_validate_timeline_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let facts = temp.path().join("facts.json");
        let rendered = temp.path().join("out.md");
        let receipt = temp.path().join("receipt.json");
        write_timeline_input(&facts);
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
        write_timeline_input(&facts);
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
        write_timeline_input(&facts);
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
                sources: SourceArgs {
                    events: Some(link),
                    ..SourceArgs::default()
                },
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
        write_timeline_input(&facts);
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
}
