//! Persisted replay parent snapshot artifact references and stores.
//!
//! The public evidence boundary is a small JSON reference. Snapshot bytes stay
//! Rust-derived runtime artifacts; Nickel/checkers validate refs, digests, and
//! bounded paths rather than owning VM internals.

use chaoscontrol_vmm::controller::SimulationSnapshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snafu::Snafu;
use std::fs;
use std::path::{Path, PathBuf};

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const SNAPSHOT_CODEC: &str = "simulation-snapshot-json-v1";
pub const FILE_STORE_KIND: &str = "file-content-addressed";
pub const SNAPSHOT_DIR: &str = "snapshots";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayParentSnapshotRef {
    pub store: String,
    pub digest: String,
    pub codec: String,
    pub schema_version: u32,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let bytes = serde_json::to_vec_pretty(&envelope)
            .map_err(|source| SnapshotStoreError::Json { source })?;
        let digest = digest_bytes(&bytes);
        let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
        let rel = format!("{SNAPSHOT_DIR}/{hex}.snapshot.json");
        let final_path = self.root.join(&rel);
        let tmp = final_path.with_extension("snapshot.json.tmp");
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
        if !path.exists() {
            return Err(SnapshotStoreError::Missing {
                path: reference.path.clone(),
            });
        }
        let bytes = fs::read(&path).map_err(|source| SnapshotStoreError::Io { source })?;
        let actual = digest_bytes(&bytes);
        if actual != reference.digest {
            return Err(SnapshotStoreError::DigestMismatch {
                path: reference.path.clone(),
                expected: reference.digest.clone(),
                actual,
            });
        }
        let artifact: SnapshotArtifactEnvelope =
            serde_json::from_slice(&bytes).map_err(|source| SnapshotStoreError::Json { source })?;
        if artifact.codec != reference.codec {
            return Err(SnapshotStoreError::UnsupportedCodec {
                codec: reference.codec.clone(),
            });
        }
        if artifact.schema_version != reference.schema_version {
            return Err(SnapshotStoreError::UnsupportedSchema {
                version: reference.schema_version,
            });
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
    if reference.store != FILE_STORE_KIND {
        return Err(SnapshotStoreError::UnsupportedStore {
            store: reference.store.clone(),
        });
    }
    if reference.codec != SNAPSHOT_CODEC {
        return Err(SnapshotStoreError::UnsupportedCodec {
            codec: reference.codec.clone(),
        });
    }
    if reference.schema_version != SNAPSHOT_SCHEMA_VERSION {
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
    use rand::SeedableRng;

    fn dummy_snapshot(tick: u64) -> SimulationSnapshot {
        let network_state = chaoscontrol_vmm::controller::NetworkFabric {
            partitions: Vec::new(),
            latency: vec![0, 0],
            jitter: vec![0, 0],
            bandwidth_bps: vec![0, 0],
            next_free_tick: vec![0, 0],
            in_flight: Vec::new(),
            packet_in_flight: Vec::new(),
            loss_rate_ppm: Vec::new(),
            corruption_rate_ppm: Vec::new(),
            reorder_window: Vec::new(),
            duplicate_rate_ppm: Vec::new(),
            rng: rand_chacha::ChaCha20Rng::seed_from_u64(42),
            stats: Default::default(),
        };
        let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());

        SimulationSnapshot {
            tick,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
        }
    }

    fn write_artifact(dir: &Path, artifact: &SnapshotArtifactEnvelope) -> ReplayParentSnapshotRef {
        let bytes = serde_json::to_vec_pretty(artifact).unwrap();
        let digest = digest_bytes(&bytes);
        let hex = digest.strip_prefix("sha256:").unwrap();
        let rel = format!("snapshots/{hex}.snapshot.json");
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
                vm_count: 3,
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
            path: "snapshots/0000000000000000000000000000000000000000000000000000000000000000.snapshot.json".to_string(),
        };
        assert!(matches!(
            store.get_snapshot_artifact(&reference),
            Err(SnapshotStoreError::Missing { .. })
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
    fn corrupt_json_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("snapshots")).unwrap();
        let bytes = b"not json";
        let digest = digest_bytes(bytes);
        let hex = digest.strip_prefix("sha256:").unwrap();
        let rel = format!("snapshots/{hex}.snapshot.json");
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
            Err(SnapshotStoreError::Json { .. })
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
