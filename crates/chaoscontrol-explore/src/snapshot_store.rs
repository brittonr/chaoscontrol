//! Persisted replay parent snapshot artifact references and stores.
//!
//! The public evidence boundary is a small JSON reference. Snapshot bytes stay
//! Rust-derived runtime artifacts; Nickel/checkers validate refs, digests, and
//! bounded paths rather than owning VM internals.

use chaoscontrol_vmm::controller::SimulationSnapshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snafu::Snafu;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub use chaoscontrol_replay_evidence_core::dto::ReplayParentSnapshotRef;
use chaoscontrol_replay_evidence_core::validate as core_validate;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = core_validate::CURRENT_SNAPSHOT_SCHEMA_VERSION;
pub const SNAPSHOT_CODEC: &str = core_validate::CURRENT_SNAPSHOT_CODEC;
pub const FILE_STORE_KIND: &str = core_validate::FILE_STORE_KIND;
pub const SNAPSHOT_DIR: &str = "snapshots";
const BYTES_PER_KIB: u64 = 1024;
const KIB_PER_MIB: u64 = 1024;
const BYTES_PER_MIB: u64 = BYTES_PER_KIB * KIB_PER_MIB;
const MAX_COMPRESSED_SNAPSHOT_BYTES: u64 = 256 * BYTES_PER_MIB;
const MAX_DECOMPRESSED_SNAPSHOT_BYTES: u64 = 2048 * BYTES_PER_MIB;
const READ_LIMIT_SENTINEL_BYTES: u64 = 1;
const SNAPSHOT_COMPRESSION_LEVEL: i32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotArtifactEnvelope {
    pub schema_version: u32,
    pub codec: String,
    pub replay_parent_depth: u32,
    pub tick: u64,
    pub vm_count: usize,
    pub snapshot: SimulationSnapshot,
}

#[derive(Debug, Snafu)]
pub enum SnapshotStoreError {
    #[snafu(display("snapshot ref uses unsupported store '{store}'"))]
    UnsupportedStore { store: String },
    #[snafu(display("snapshot ref uses unsupported codec '{codec}'"))]
    UnsupportedCodec { codec: String },
    #[snafu(display("snapshot ref uses unsupported schema version {version}"))]
    UnsupportedSchema { version: u32 },
    #[snafu(display("snapshot path escapes store root: {path}"))]
    PathEscape { path: String },
    #[snafu(display("snapshot artifact missing: {path}"))]
    Missing { path: String },
    #[snafu(display("snapshot artifact is not a regular file: {path}"))]
    NotRegular { path: String },
    #[snafu(display("snapshot artifact exceeds {limit} bytes: {path}"))]
    TooLarge { path: String, limit: u64 },
    #[snafu(display("snapshot decompression exceeds {limit} bytes"))]
    DecompressedTooLarge { limit: u64 },
    #[snafu(display("snapshot artifact metadata mismatch: {field}"))]
    MetadataMismatch { field: &'static str },
    #[snafu(display(
        "snapshot artifact digest mismatch for {path}: expected {expected}, got {actual}"
    ))]
    DigestMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[snafu(display("snapshot artifact I/O error: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("snapshot artifact JSON error: {source}"))]
    Json { source: serde_json::Error },
    #[snafu(display("snapshot artifact CBOR encode error: {source}"))]
    CborEncode {
        source: ciborium::ser::Error<std::io::Error>,
    },
    #[snafu(display("snapshot artifact CBOR decode error: {source}"))]
    CborDecode {
        source: ciborium::de::Error<std::io::Error>,
    },
}

fn encode_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, SnapshotStoreError> {
    let mut encoded = Vec::new();
    ciborium::ser::into_writer(value, &mut encoded)
        .map_err(|source| SnapshotStoreError::CborEncode { source })?;
    Ok(encoded)
}

fn decode_cbor<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, SnapshotStoreError> {
    ciborium::de::from_reader(std::io::Cursor::new(bytes))
        .map_err(|source| SnapshotStoreError::CborDecode { source })
}

fn read_snapshot_bytes(path: &Path, display_path: &str) -> Result<Vec<u8>, SnapshotStoreError> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                SnapshotStoreError::Missing {
                    path: display_path.to_string(),
                }
            } else if source.raw_os_error() == Some(libc::ELOOP) {
                SnapshotStoreError::NotRegular {
                    path: display_path.to_string(),
                }
            } else {
                SnapshotStoreError::Io { source }
            }
        })?;
    let metadata = file
        .metadata()
        .map_err(|source| SnapshotStoreError::Io { source })?;
    if !metadata.file_type().is_file() {
        return Err(SnapshotStoreError::NotRegular {
            path: display_path.to_string(),
        });
    }
    if metadata.len() > MAX_COMPRESSED_SNAPSHOT_BYTES {
        return Err(SnapshotStoreError::TooLarge {
            path: display_path.to_string(),
            limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
        });
    }
    let read_limit = MAX_COMPRESSED_SNAPSHOT_BYTES
        .checked_add(READ_LIMIT_SENTINEL_BYTES)
        .ok_or(SnapshotStoreError::TooLarge {
            path: display_path.to_string(),
            limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
        })?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|source| SnapshotStoreError::Io { source })?;
    if bytes.len() as u64 > MAX_COMPRESSED_SNAPSHOT_BYTES {
        return Err(SnapshotStoreError::TooLarge {
            path: display_path.to_string(),
            limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
        });
    }
    Ok(bytes)
}

fn decompress_snapshot(bytes: &[u8], maximum_bytes: u64) -> Result<Vec<u8>, SnapshotStoreError> {
    let decoder = zstd::stream::read::Decoder::new(std::io::Cursor::new(bytes))
        .map_err(|source| SnapshotStoreError::Io { source })?;
    let read_limit = maximum_bytes.checked_add(READ_LIMIT_SENTINEL_BYTES).ok_or(
        SnapshotStoreError::DecompressedTooLarge {
            limit: maximum_bytes,
        },
    )?;
    let mut decompressed = Vec::new();
    decoder
        .take(read_limit)
        .read_to_end(&mut decompressed)
        .map_err(|source| SnapshotStoreError::Io { source })?;
    if decompressed.len() as u64 > maximum_bytes {
        return Err(SnapshotStoreError::DecompressedTooLarge {
            limit: maximum_bytes,
        });
    }
    Ok(decompressed)
}

pub trait SnapshotStore {
    fn put_snapshot(
        &self,
        snapshot: &SimulationSnapshot,
        replay_parent_depth: u32,
    ) -> Result<ReplayParentSnapshotRef, SnapshotStoreError>;

    fn get_snapshot_artifact(
        &self,
        reference: &ReplayParentSnapshotRef,
    ) -> Result<SnapshotArtifactEnvelope, SnapshotStoreError>;

    fn get_snapshot(
        &self,
        reference: &ReplayParentSnapshotRef,
    ) -> Result<SimulationSnapshot, SnapshotStoreError> {
        Ok(self.get_snapshot_artifact(reference)?.snapshot)
    }

    fn has_snapshot(&self, reference: &ReplayParentSnapshotRef) -> bool;

    fn gc_unreferenced(
        &self,
        keep: &[ReplayParentSnapshotRef],
    ) -> Result<usize, SnapshotStoreError>;
}

#[derive(Debug, Clone)]
pub struct FileSnapshotStore {
    root: PathBuf,
}

impl FileSnapshotStore {
    pub fn new(run_output_dir: impl AsRef<Path>) -> Self {
        Self {
            root: run_output_dir.as_ref().to_path_buf(),
        }
    }

    fn snapshots_dir(&self) -> PathBuf {
        self.root.join(SNAPSHOT_DIR)
    }

    fn resolve_ref(
        &self,
        reference: &ReplayParentSnapshotRef,
    ) -> Result<PathBuf, SnapshotStoreError> {
        validate_ref_shape(reference)?;
        let rel = Path::new(&reference.path);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || !reference.path.starts_with("snapshots/")
        {
            return Err(SnapshotStoreError::PathEscape {
                path: reference.path.clone(),
            });
        }
        Ok(self.root.join(rel))
    }
}

impl SnapshotStore for FileSnapshotStore {
    fn put_snapshot(
        &self,
        snapshot: &SimulationSnapshot,
        replay_parent_depth: u32,
    ) -> Result<ReplayParentSnapshotRef, SnapshotStoreError> {
        fs::create_dir_all(self.snapshots_dir())
            .map_err(|source| SnapshotStoreError::Io { source })?;
        let envelope = SnapshotArtifactEnvelope {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            codec: SNAPSHOT_CODEC.to_string(),
            replay_parent_depth,
            tick: snapshot.tick,
            vm_count: snapshot.vm_snapshots.len(),
            snapshot: snapshot.clone(),
        };
        let uncompressed = encode_cbor(&envelope)?;
        if uncompressed.len() as u64 > MAX_DECOMPRESSED_SNAPSHOT_BYTES {
            return Err(SnapshotStoreError::DecompressedTooLarge {
                limit: MAX_DECOMPRESSED_SNAPSHOT_BYTES,
            });
        }
        let bytes = zstd::stream::encode_all(
            std::io::Cursor::new(uncompressed),
            SNAPSHOT_COMPRESSION_LEVEL,
        )
        .map_err(|source| SnapshotStoreError::Io { source })?;
        if bytes.len() as u64 > MAX_COMPRESSED_SNAPSHOT_BYTES {
            return Err(SnapshotStoreError::TooLarge {
                path: "generated snapshot".to_string(),
                limit: MAX_COMPRESSED_SNAPSHOT_BYTES,
            });
        }
        let digest = digest_bytes(&bytes);
        let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
        let rel = format!("{SNAPSHOT_DIR}/{hex}.snapshot.bin");
        let final_path = self.root.join(&rel);
        let tmp = final_path.with_extension("snapshot.bin.tmp");
        fs::write(&tmp, &bytes).map_err(|source| SnapshotStoreError::Io { source })?;
        fs::rename(&tmp, &final_path).map_err(|source| SnapshotStoreError::Io { source })?;
        Ok(ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest,
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: rel,
        })
    }

    fn get_snapshot_artifact(
        &self,
        reference: &ReplayParentSnapshotRef,
    ) -> Result<SnapshotArtifactEnvelope, SnapshotStoreError> {
        let path = self.resolve_ref(reference)?;
        let bytes = read_snapshot_bytes(&path, &reference.path)?;
        let actual = digest_bytes(&bytes);
        if actual != reference.digest {
            return Err(SnapshotStoreError::DigestMismatch {
                path: reference.path.clone(),
                expected: reference.digest.clone(),
                actual,
            });
        }
        let decompressed = decompress_snapshot(&bytes, MAX_DECOMPRESSED_SNAPSHOT_BYTES)?;
        let artifact: SnapshotArtifactEnvelope = decode_cbor(&decompressed)?;
        if artifact.codec != reference.codec {
            return Err(SnapshotStoreError::MetadataMismatch { field: "codec" });
        }
        if artifact.schema_version != reference.schema_version {
            return Err(SnapshotStoreError::MetadataMismatch {
                field: "schema_version",
            });
        }
        if artifact.tick != artifact.snapshot.tick {
            return Err(SnapshotStoreError::MetadataMismatch { field: "tick" });
        }
        if artifact.vm_count != artifact.snapshot.vm_snapshots.len() {
            return Err(SnapshotStoreError::MetadataMismatch { field: "vm_count" });
        }
        Ok(artifact)
    }

    fn has_snapshot(&self, reference: &ReplayParentSnapshotRef) -> bool {
        self.get_snapshot_artifact(reference).is_ok()
    }

    fn gc_unreferenced(
        &self,
        keep: &[ReplayParentSnapshotRef],
    ) -> Result<usize, SnapshotStoreError> {
        let keep: std::collections::BTreeSet<String> =
            keep.iter().map(|r| r.path.clone()).collect();
        let mut removed = 0;
        let dir = self.snapshots_dir();
        if !dir.exists() {
            return Ok(0);
        }
        for entry in fs::read_dir(&dir).map_err(|source| SnapshotStoreError::Io { source })? {
            let entry = entry.map_err(|source| SnapshotStoreError::Io { source })?;
            let rel = format!("{SNAPSHOT_DIR}/{}", entry.file_name().to_string_lossy());
            if !keep.contains(&rel) {
                fs::remove_file(entry.path())
                    .map_err(|source| SnapshotStoreError::Io { source })?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

pub fn validate_ref_shape(reference: &ReplayParentSnapshotRef) -> Result<(), SnapshotStoreError> {
    // Admissibility decisions live in the shared core; this shell maps them
    // onto the store error variants for stable diagnostics.
    if reference.store != core_validate::FILE_STORE_KIND {
        return Err(SnapshotStoreError::UnsupportedStore {
            store: reference.store.clone(),
        });
    }
    if reference.codec != core_validate::CURRENT_SNAPSHOT_CODEC {
        return Err(SnapshotStoreError::UnsupportedCodec {
            codec: reference.codec.clone(),
        });
    }
    if reference.schema_version != core_validate::CURRENT_SNAPSHOT_SCHEMA_VERSION {
        return Err(SnapshotStoreError::UnsupportedSchema {
            version: reference.schema_version,
        });
    }
    Ok(())
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{:x}", h.finalize())
}

/// Optional host-side redb store/index seam. It is intentionally feature-less in
/// the baseline so the public evidence format never requires redb inspection.
pub mod host_snapshot_store_redb {
    pub const STORE_NAME: &str = "host-snapshot-store-redb";
    pub const ROLE: &str = "optional host-side index/store; not a public evidence boundary and unrelated to chaoscontrol-redb-guest";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_snapshot(tick: u64) -> SimulationSnapshot {
        let network_state = chaoscontrol_vmm::controller::NetworkFabric::new(2, 42);
        let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());

        SimulationSnapshot {
            tick,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
            process_fault_attempt: vec![],
            pending_process_observations: Default::default(),
            fault_operation_sequence: 0,
        }
    }

    fn write_artifact(dir: &Path, artifact: &SnapshotArtifactEnvelope) -> ReplayParentSnapshotRef {
        let bytes = zstd::stream::encode_all(
            std::io::Cursor::new(encode_cbor(artifact).unwrap()),
            SNAPSHOT_COMPRESSION_LEVEL,
        )
        .unwrap();
        let digest = digest_bytes(&bytes);
        let hex = digest.strip_prefix("sha256:").unwrap();
        let rel = format!("snapshots/{hex}.snapshot.bin");
        fs::create_dir_all(dir.join("snapshots")).unwrap();
        fs::write(dir.join(&rel), bytes).unwrap();
        ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest,
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: rel,
        }
    }

    #[test]
    fn file_store_validates_metadata_and_digest() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = write_artifact(
            dir.path(),
            &SnapshotArtifactEnvelope {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                codec: SNAPSHOT_CODEC.to_string(),
                replay_parent_depth: 2,
                tick: 17,
                vm_count: 0,
                snapshot: dummy_snapshot(17),
            },
        );
        let artifact = store.get_snapshot_artifact(&reference).unwrap();
        assert_eq!(artifact.tick, 17);
        assert_eq!(store.get_snapshot(&reference).unwrap().tick, 17);
        assert_eq!(artifact.replay_parent_depth, 2);
        assert!(store.has_snapshot(&reference));
    }

    #[test]
    fn digest_mismatch_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let mut reference = write_artifact(
            dir.path(),
            &SnapshotArtifactEnvelope {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                codec: SNAPSHOT_CODEC.to_string(),
                replay_parent_depth: 2,
                tick: 17,
                vm_count: 3,
                snapshot: dummy_snapshot(17),
            },
        );
        reference.digest =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string();
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn path_escape_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: "sha256:00".to_string(),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: "../escape.snapshot".to_string(),
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::PathEscape { .. })
        ));
    }

    #[test]
    fn missing_artifact_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: "snapshots/0000000000000000000000000000000000000000000000000000000000000000.snapshot.bin".to_string(),
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::Missing { .. })
        ));
    }

    #[test]
    fn symlink_snapshot_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let snapshots = dir.path().join(SNAPSHOT_DIR);
        fs::create_dir_all(&snapshots).unwrap();
        let target = snapshots.join("target.snapshot.bin");
        fs::write(&target, b"target").unwrap();
        let link = snapshots.join("link.snapshot.bin");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: digest_bytes(b"target"),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: format!("{SNAPSHOT_DIR}/link.snapshot.bin"),
        };

        assert!(matches!(
            FileSnapshotStore::new(dir.path()).get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::NotRegular { .. })
        ));
    }

    #[test]
    fn oversized_snapshot_is_rejected_before_read() {
        let dir = tempfile::tempdir().unwrap();
        let snapshots = dir.path().join(SNAPSHOT_DIR);
        fs::create_dir_all(&snapshots).unwrap();
        let path = snapshots.join("oversized.snapshot.bin");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_COMPRESSED_SNAPSHOT_BYTES + READ_LIMIT_SENTINEL_BYTES)
            .unwrap();
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: digest_bytes(&[]),
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: format!("{SNAPSHOT_DIR}/oversized.snapshot.bin"),
        };

        assert!(matches!(
            FileSnapshotStore::new(dir.path()).get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::TooLarge { .. })
        ));
    }

    #[test]
    fn decompression_limit_is_enforced() {
        const TEST_DECOMPRESSED_LIMIT: u64 = 16;
        let expanded = vec![0_u8; (TEST_DECOMPRESSED_LIMIT + READ_LIMIT_SENTINEL_BYTES) as usize];
        let compressed =
            zstd::stream::encode_all(std::io::Cursor::new(expanded), SNAPSHOT_COMPRESSION_LEVEL)
                .unwrap();

        assert!(matches!(
            decompress_snapshot(&compressed, TEST_DECOMPRESSED_LIMIT),
            Err(SnapshotStoreError::DecompressedTooLarge { .. })
        ));
    }

    #[test]
    fn snapshot_envelope_metadata_mismatch_is_rejected() {
        const FORGED_VM_COUNT: usize = 3;
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = write_artifact(
            dir.path(),
            &SnapshotArtifactEnvelope {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                codec: SNAPSHOT_CODEC.to_string(),
                replay_parent_depth: 1,
                tick: 17,
                vm_count: FORGED_VM_COUNT,
                snapshot: dummy_snapshot(17),
            },
        );

        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::MetadataMismatch { field: "vm_count" })
        ));
    }

    #[test]
    fn unsupported_codec_and_schema_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest: "sha256:00".to_string(),
            codec: "unsupported".to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: "snapshots/00.snapshot.json".to_string(),
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::UnsupportedCodec { .. })
        ));
        let reference = ReplayParentSnapshotRef {
            schema_version: SNAPSHOT_SCHEMA_VERSION + 1,
            codec: SNAPSHOT_CODEC.to_string(),
            ..reference
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn corrupt_binary_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("snapshots")).unwrap();
        let bytes = b"not zstd-compressed bincode";
        let digest = digest_bytes(bytes);
        let hex = digest.strip_prefix("sha256:").unwrap();
        let rel = format!("snapshots/{hex}.snapshot.bin");
        fs::write(dir.path().join(&rel), bytes).unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let reference = ReplayParentSnapshotRef {
            store: FILE_STORE_KIND.to_string(),
            digest,
            codec: SNAPSHOT_CODEC.to_string(),
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            path: rel,
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::Io { .. })
        ));
    }

    #[test]
    fn saved_bug_fixture_loads_restorable_parent_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSnapshotStore::new(dir.path());
        let snapshot = dummy_snapshot(99);
        let reference = store.put_snapshot(&snapshot, 1).unwrap();
        let bug = crate::checkpoint::SerializableBug {
            bug_id: 7,
            assertion_id: 42,
            assertion_identity: Some(crate::test_support::assertion_identity(42)),
            assertion_location: "fixture assertion".to_string(),
            schedule: (&chaoscontrol_fault::schedule::FaultSchedule::new()).into(),
            tick: 101,
            replay_parent_depth: 1,
            replay_parent_snapshot_ref: Some(reference.clone()),
            dedup_key: None,
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        };
        let bug_path = dir.path().join("bug_0.json");
        fs::write(&bug_path, serde_json::to_vec_pretty(&bug).unwrap()).unwrap();

        let loaded_bug: crate::checkpoint::SerializableBug =
            serde_json::from_slice(&fs::read(&bug_path).unwrap()).unwrap();
        let loaded = store
            .get_snapshot(loaded_bug.replay_parent_snapshot_ref.as_ref().unwrap())
            .unwrap();

        assert_eq!(loaded.tick, 99);
        assert_eq!(loaded.vm_snapshots.len(), 0);
    }
}
