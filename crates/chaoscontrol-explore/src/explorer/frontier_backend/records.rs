//! Journal record kinds of the Campaign frontier and epoch replay.
//!
//! Journals record decisions, not inputs:
//!
//! | kind | record | payload |
//! |------|--------|---------|
//! | 1 | `GENESIS` | product epoch header (seed, bounds, previous epoch digest) |
//! | 2 | `MOMENT` | Choregraph moment event (claimed id + draft) |
//! | 3 | `CONTROL` | Campaign control event (authenticated mirror) |
//! | 4 | `ENTRY` | product frontier entry of an admitted candidate |
//! | 5 | `SELECTION` | recorded draws and round of a published selection |
//! | 6 | `ROUND` | product settlement facts of one round |
//!
//! Replay re-admits every moment under its claimed identity, re-plans every
//! control event through Campaign's publication planner in record order, and
//! seals each segment, so codec drift, reordering, or a rewritten record fails
//! as a typed replay error instead of producing another history.

use super::session::{history_limits, ControlSession};
use super::FrontierError;
use crate::campaign_adapter::wire::WireId;
use crate::campaign_adapter::{
    child_moment_id, decode_control_event, decode_moment, readmit_moment, root_moment_id,
    AdapterError, AdapterIdentity, AdmittedMoment, BranchSlot, WireError,
};
use crate::explorer::journal::{JournalRecord, RecordKind, RecoveredEpoch};
use campaign_choregraph::{id_from_digest, HistoryGraph, HistoryLimits};
use campaign_core::{CampaignControlEvent, Id, IdKind};
use std::collections::BTreeMap;

/// Epoch header.
pub(crate) const GENESIS: RecordKind = RecordKind::new(1);
/// Choregraph moment event.
pub(crate) const MOMENT: RecordKind = RecordKind::new(2);
/// Campaign control event.
pub(crate) const CONTROL: RecordKind = RecordKind::new(3);
/// Product frontier entry.
pub(crate) const ENTRY: RecordKind = RecordKind::new(4);
/// Published selection facts.
pub(crate) const SELECTION: RecordKind = RecordKind::new(5);
/// Round settlement facts.
pub(crate) const ROUND: RecordKind = RecordKind::new(6);
/// Schema of the genesis record.
pub(crate) const GENESIS_SCHEMA: &str = "chaoscontrol.campaign-genesis.v1";
const SLOT_PROBE: u8 = 1;
const SLOT_VARIANT: u8 = 2;
const SLOT_ALTERNATIVE: u8 = 3;

/// Stable code of one branch slot.
pub(crate) const fn slot_code(slot: BranchSlot) -> (u8, u32) {
    match slot {
        BranchSlot::Probe => (SLOT_PROBE, 0),
        BranchSlot::Variant(index) => (SLOT_VARIANT, index),
        BranchSlot::Alternative(index) => (SLOT_ALTERNATIVE, index),
    }
}

/// Epoch header: the product facts that bind one Campaign admission.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GenesisRecord {
    /// [`GENESIS_SCHEMA`].
    pub(crate) schema: String,
    /// Journal epoch.
    pub(crate) epoch: u32,
    /// Exploration seed (binds the entropy source).
    pub(crate) seed: u64,
    /// History event bound of the epoch.
    pub(crate) max_history_events: u64,
    /// Final segment digest of the previous epoch, if one exists.
    pub(crate) previous_epoch_digest: Option<[u8; 32]>,
}

/// One admitted candidate's product entry.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EntryRecord {
    /// Campaign candidate.
    pub(crate) candidate: WireId,
    /// Parent moment of the candidate.
    pub(crate) moment: WireId,
    /// Product sequence number.
    pub(crate) seq: u64,
    /// IEEE-754 bits of the legacy score.
    pub(crate) score_bits: u64,
    /// Product depth of the entry.
    pub(crate) depth: u32,
    /// Whether a marker created the entry.
    pub(crate) marker: bool,
}

/// One published selection's recorded draws.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectionRecord {
    /// Loop index of the round.
    pub(crate) round: u64,
    /// Round sub-mode name.
    pub(crate) mode: String,
    /// Bits of the recorded `gen::<f64>` draw.
    pub(crate) unit_bits: u64,
    /// Recorded `gen_range` draw on the explore branch.
    pub(crate) index: Option<u64>,
    /// Selected candidate.
    pub(crate) candidate: WireId,
    /// Published selection.
    pub(crate) selection: WireId,
    /// Product sequence number of the selected entry.
    pub(crate) seq: u64,
}

/// One settled round.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RoundRecord {
    /// Loop index of the round.
    pub(crate) round: u64,
    /// Settled selection.
    pub(crate) selection: WireId,
    /// Digest of the round's product manifest.
    pub(crate) manifest: [u8; 32],
    /// Sequence numbers admitted by the round.
    pub(crate) admitted: Vec<u64>,
    /// Sequence numbers pruned by the round.
    pub(crate) pruned: Vec<u64>,
}

/// Encodes one product record as canonical CBOR.
pub(crate) fn encode<T: serde::Serialize>(
    kind: RecordKind,
    value: &T,
) -> Result<JournalRecord, FrontierError> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes)
        .map_err(|_| FrontierError::Encoding("journal product record"))?;
    Ok(JournalRecord::new(kind, bytes))
}

/// Decodes one product record, rejecting trailing bytes and non-canonical input.
pub(crate) fn decode<T: serde::Serialize + serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, ReplayFailure> {
    let mut reader = bytes;
    let value: T = ciborium::de::from_reader(&mut reader).map_err(|_| ReplayFailure::Malformed)?;
    if !reader.is_empty() {
        return Err(ReplayFailure::Malformed);
    }
    let mut canonical = Vec::with_capacity(bytes.len());
    ciborium::ser::into_writer(&value, &mut canonical).map_err(|_| ReplayFailure::Malformed)?;
    if canonical != bytes {
        return Err(ReplayFailure::Malformed);
    }
    Ok(value)
}

/// Why a recovered record cannot be replayed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayFailure {
    /// The epoch's first record is not a genesis record.
    MissingGenesis,
    /// A genesis record appears after the first record, or names another epoch.
    MisplacedGenesis,
    /// The genesis schema is unknown.
    UnknownSchema,
    /// The root moment or the admission is missing or out of order.
    OutOfOrder,
    /// The record kind is unknown.
    UnknownKind(u16),
    /// A product record is malformed or non-canonical.
    Malformed,
    /// A moment or control record failed to decode or authenticate.
    Wire(WireError),
    /// Campaign or Choregraph rejected the recorded history.
    Adapter(AdapterError),
    /// Replaying the recorded history diverged from the recorded state.
    Diverged,
}

/// Position and reason of one replay failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayError {
    /// Epoch replayed.
    pub epoch: u32,
    /// Segment sequence.
    pub seq: u64,
    /// Record index within the segment.
    pub record: usize,
    /// Reason.
    pub failure: ReplayFailure,
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "epoch {} segment {} record {}: {:?}",
            self.epoch, self.seq, self.record, self.failure
        )
    }
}

impl From<FrontierError> for ReplayFailure {
    fn from(error: FrontierError) -> Self {
        match error {
            FrontierError::Adapter(error) => Self::Adapter(error),
            _ => Self::Diverged,
        }
    }
}

impl From<WireError> for ReplayFailure {
    fn from(error: WireError) -> Self {
        Self::Wire(error)
    }
}

/// One replayed epoch.
pub(crate) struct ReplayedEpoch {
    /// Adapter identity of the epoch's seed.
    pub(crate) identity: AdapterIdentity,
    /// Control session after the last segment; `None` for an epoch whose
    /// genesis never committed.
    pub(crate) session: Option<ControlSession>,
    /// Product sequence number of every recorded candidate.
    pub(crate) entries: BTreeMap<Id, u64>,
}

enum Cursor {
    Genesis,
    RootMoment(GenesisRecord),
    Admission(GenesisRecord, AdmittedMoment),
    Live,
}

fn moment_id_of(
    draft: &campaign_choregraph::EventDraft,
    event: campaign_choregraph::EventId,
) -> Id {
    if draft.parents.is_empty() {
        root_moment_id(event)
    } else {
        child_moment_id(event)
    }
}

/// Replays one recovered epoch into a control session.
///
/// Returns `Ok(None)` for an epoch without segments.
pub(crate) fn replay_epoch(epoch: &RecoveredEpoch) -> Result<Option<ReplayedEpoch>, ReplayError> {
    let mut cursor = Cursor::Genesis;
    let mut replayed: Option<ReplayedEpoch> = None;
    for segment in epoch.segments() {
        for (index, record) in segment.records().iter().enumerate() {
            let at = |failure| ReplayError {
                epoch: epoch.epoch(),
                seq: segment.seq(),
                record: index,
                failure,
            };
            cursor = step(cursor, &mut replayed, epoch.epoch(), record).map_err(at)?;
        }
        if let Some(session) = replayed.as_mut().and_then(|epoch| epoch.session.as_mut()) {
            session.seal().map_err(|error| ReplayError {
                epoch: epoch.epoch(),
                seq: segment.seq(),
                record: segment.records().len(),
                failure: error.into(),
            })?;
        }
    }
    if !matches!(cursor, Cursor::Live | Cursor::Genesis) {
        let seq = epoch.segments().last().map_or(0, |segment| segment.seq());
        return Err(ReplayError {
            epoch: epoch.epoch(),
            seq,
            record: 0,
            failure: ReplayFailure::OutOfOrder,
        });
    }
    Ok(replayed)
}

/// History bounds of a recorded epoch. The genesis records the whole history
/// bound, which is also the epoch's Choregraph projection bound.
fn epoch_limits(genesis: &GenesisRecord) -> Result<HistoryLimits, ReplayFailure> {
    history_limits(genesis.max_history_events, genesis.max_history_events)
        .map_err(ReplayFailure::from)
}

fn step(
    cursor: Cursor,
    replayed: &mut Option<ReplayedEpoch>,
    epoch: u32,
    record: &JournalRecord,
) -> Result<Cursor, ReplayFailure> {
    let kind = record.kind();
    match cursor {
        Cursor::Genesis => {
            if kind != GENESIS {
                return Err(ReplayFailure::MissingGenesis);
            }
            let genesis: GenesisRecord = decode(record.payload())?;
            if genesis.schema != GENESIS_SCHEMA {
                return Err(ReplayFailure::UnknownSchema);
            }
            if genesis.epoch != epoch {
                return Err(ReplayFailure::MisplacedGenesis);
            }
            Ok(Cursor::RootMoment(genesis))
        }
        Cursor::RootMoment(genesis) => {
            if kind != MOMENT {
                return Err(ReplayFailure::OutOfOrder);
            }
            let (event, draft) = decode_moment(record.payload())?;
            if !draft.parents.is_empty() {
                return Err(ReplayFailure::OutOfOrder);
            }
            let limits = epoch_limits(&genesis)?;
            let empty =
                HistoryGraph::new(limits).map_err(|error| ReplayFailure::Adapter(error.into()))?;
            let payload_id = id_from_digest(IdKind::EventPayload, draft.payload_identity);
            let graph = readmit_moment(&empty, event, draft).map_err(ReplayFailure::Adapter)?;
            Ok(Cursor::Admission(
                genesis,
                AdmittedMoment {
                    graph,
                    event,
                    moment_id: root_moment_id(event),
                    event_payload_id: payload_id,
                },
            ))
        }
        Cursor::Admission(genesis, root) => {
            if kind != CONTROL {
                return Err(ReplayFailure::OutOfOrder);
            }
            let event = decode_control_event(record.payload())?;
            if !matches!(event, CampaignControlEvent::CampaignAdmitted { .. }) {
                return Err(ReplayFailure::OutOfOrder);
            }
            let identity = AdapterIdentity::for_seed(genesis.seed);
            let limits = epoch_limits(&genesis)?;
            let session = ControlSession::genesis(identity.clone(), limits, &root, event)?;
            *replayed = Some(ReplayedEpoch {
                identity,
                session: Some(session),
                entries: BTreeMap::new(),
            });
            Ok(Cursor::Live)
        }
        Cursor::Live => {
            let epoch_state = replayed.as_mut().ok_or(ReplayFailure::OutOfOrder)?;
            let session = epoch_state
                .session
                .as_mut()
                .ok_or(ReplayFailure::OutOfOrder)?;
            match kind {
                GENESIS => return Err(ReplayFailure::MisplacedGenesis),
                MOMENT => {
                    let (event, draft) = decode_moment(record.payload())?;
                    let moment_id = moment_id_of(&draft, event);
                    let graph = readmit_moment(session.graph(), event, draft)
                        .map_err(ReplayFailure::Adapter)?;
                    session.readmit_moment(moment_id, event, graph);
                }
                CONTROL => {
                    let event = decode_control_event(record.payload())?;
                    session.publish(event)?;
                }
                ENTRY => {
                    let entry: EntryRecord = decode(record.payload())?;
                    let candidate = Id::try_from(entry.candidate).map_err(ReplayFailure::Wire)?;
                    epoch_state.entries.insert(candidate, entry.seq);
                }
                SELECTION => {
                    decode::<SelectionRecord>(record.payload())?;
                }
                ROUND => {
                    decode::<RoundRecord>(record.payload())?;
                }
                other => return Err(ReplayFailure::UnknownKind(other.get())),
            }
            Ok(Cursor::Live)
        }
    }
}
