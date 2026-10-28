use std::path::Path;

use chaoscontrol_snapshot_descriptor as snapshot_core;
use sha2::Digest as _;

use crate::{EvidenceError, EvidenceResult, SnapshotChunkManifest};

const HEX_ALPHABET_BYTES: usize = 16;
const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HEX_HIGH_NIBBLE_SHIFT: u32 = 4;
const HEX_LOW_NIBBLE_MASK: u8 = 0x0f;
const HEX_DIGITS: &[u8; HEX_ALPHABET_BYTES] = b"0123456789abcdef";

// r[impl chaoscontrol.snapshot_descriptor.closure]
pub fn monolithic_closure_from_file(
    path: impl AsRef<Path>,
    maximum_bytes: u64,
) -> EvidenceResult<snapshot_core::PayloadClosure> {
    let path = path.as_ref();
    let bytes = crate::bounded_file::read_bounded_regular_bytes(path, maximum_bytes)?;
    if bytes.is_empty() {
        return Err(EvidenceError::new(
            "snapshot payload length 0 is outside the declared bound",
        ));
    }
    let logical_payload = snapshot_core::ContentIdentity {
        digest: snapshot_core::digest_bytes(snapshot_core::DigestAlgorithm::Sha256, &bytes),
        length_bytes: bytes.len() as u64,
        codec: snapshot_core::CURRENT_PAYLOAD_CODEC.to_string(),
    };
    let closure = snapshot_core::PayloadClosure {
        kind: snapshot_core::ClosureKind::Monolithic,
        logical_payload: logical_payload.clone(),
        manifest: None,
        members: vec![snapshot_core::ClosureMember {
            order: 0,
            role: snapshot_core::ClosureRole::SnapshotPayload,
            content: logical_payload,
        }],
    };
    snapshot_core::validate_payload_closure(&closure)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    Ok(closure)
}

pub fn chunked_closure_from_manifest(
    manifest_path: impl AsRef<Path>,
    artifact_root: impl AsRef<Path>,
    maximum_bytes: u64,
) -> EvidenceResult<snapshot_core::PayloadClosure> {
    let manifest_path = manifest_path.as_ref();
    let artifact_root = artifact_root.as_ref();
    let manifest_bytes = crate::bounded_file::read_bounded_regular_bytes(
        manifest_path,
        crate::MAX_EVIDENCE_JSON_BYTES,
    )?;
    let manifest: SnapshotChunkManifest = serde_json::from_slice(&manifest_bytes)?;
    manifest.validate_shape()?;
    if manifest.original_size == 0 || manifest.original_size > maximum_bytes {
        return Err(EvidenceError::new(format!(
            "chunked snapshot length {} is outside the declared bound {maximum_bytes}",
            manifest.original_size
        )));
    }
    if manifest.chunks.len() > snapshot_core::MAX_CLOSURE_MEMBERS {
        return Err(EvidenceError::new(
            "snapshot chunk count exceeds the public bound",
        ));
    }
    let mut aggregate = sha2::Sha256::new();
    let mut total_bytes = 0_u64;
    let mut members = Vec::with_capacity(manifest.chunks.len());
    for (index, chunk) in manifest.chunks.iter().enumerate() {
        let path = artifact_root.join(&chunk.path);
        ensure_chunk_within_root(artifact_root, &path).map_err(|error| {
            EvidenceError::new(format!(
                "snapshot chunk missing {}: {error}",
                path.display()
            ))
        })?;
        let bytes = crate::bounded_file::read_bounded_regular_bytes(&path, maximum_bytes).map_err(
            |error| {
                EvidenceError::new(format!(
                    "snapshot chunk missing {}: {error}",
                    path.display()
                ))
            },
        )?;
        if bytes.len() as u64 != chunk.size {
            return Err(EvidenceError::new(format!(
                "snapshot chunk length mismatch for {}",
                path.display()
            )));
        }
        let actual = snapshot_core::digest_bytes(snapshot_core::DigestAlgorithm::Sha256, &bytes);
        if !actual.hex.eq_ignore_ascii_case(&chunk.sha256) {
            return Err(EvidenceError::new(format!(
                "snapshot chunk digest mismatch for {}",
                path.display()
            )));
        }
        aggregate.update(&bytes);
        total_bytes = total_bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| EvidenceError::new("snapshot chunk length accumulation overflowed"))?;
        let order = u32::try_from(index)
            .map_err(|_| EvidenceError::new("snapshot chunk order exceeds u32"))?;
        members.push(snapshot_core::ClosureMember {
            order,
            role: snapshot_core::ClosureRole::SnapshotChunk,
            content: snapshot_core::ContentIdentity {
                digest: actual,
                length_bytes: bytes.len() as u64,
                codec: snapshot_core::SNAPSHOT_CHUNK_CODEC.to_string(),
            },
        });
    }
    let aggregate_hex = hex_lower(aggregate.finalize().as_slice());
    if total_bytes != manifest.original_size
        || !aggregate_hex.eq_ignore_ascii_case(&manifest.original_sha256)
    {
        return Err(EvidenceError::new(
            "chunked snapshot aggregate length or digest mismatch",
        ));
    }
    let closure = snapshot_core::PayloadClosure {
        kind: snapshot_core::ClosureKind::Chunked,
        logical_payload: snapshot_core::ContentIdentity {
            digest: snapshot_core::TaggedDigest {
                algorithm: snapshot_core::DigestAlgorithm::Sha256,
                hex: aggregate_hex,
            },
            length_bytes: manifest.original_size,
            codec: snapshot_core::CURRENT_PAYLOAD_CODEC.to_string(),
        },
        manifest: Some(snapshot_core::ContentIdentity {
            digest: snapshot_core::digest_bytes(
                snapshot_core::DigestAlgorithm::Blake3,
                &manifest_bytes,
            ),
            length_bytes: u64::try_from(manifest_bytes.len())
                .map_err(|_| EvidenceError::new("manifest length exceeds u64"))?,
            codec: snapshot_core::CHUNK_MANIFEST_CODEC.to_string(),
        }),
        members,
    };
    snapshot_core::validate_payload_closure(&closure)
        .map_err(|error| EvidenceError::new(error.to_string()))?;
    Ok(closure)
}

fn ensure_chunk_within_root(root: &Path, path: &Path) -> EvidenceResult<()> {
    let root = std::fs::canonicalize(root)?;
    let path = std::fs::canonicalize(path)?;
    if !path.starts_with(&root) {
        return Err(EvidenceError::new(
            "snapshot chunk escapes the artifact root",
        ));
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    let capacity = bytes.len().saturating_mul(HEX_CHARACTERS_PER_BYTE);
    let mut output = String::with_capacity(capacity);
    for byte in bytes {
        output.push(char::from(
            HEX_DIGITS[usize::from(byte >> HEX_HIGH_NIBBLE_SHIFT)],
        ));
        output.push(char::from(
            HEX_DIGITS[usize::from(byte & HEX_LOW_NIBBLE_MASK)],
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_paths_cannot_escape_the_artifact_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("chunk.bin");
        std::fs::write(&outside_file, b"outside").unwrap();
        std::fs::create_dir(root.path().join("snapshots")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("snapshots/link")).unwrap();
        assert!(ensure_chunk_within_root(
            root.path(),
            &root.path().join("snapshots/link/chunk.bin")
        )
        .is_err());
    }

    #[test]
    fn monolithic_closure_rejects_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("payload.bin");
        std::fs::write(&target, b"snapshot").unwrap();
        let link = root.path().join("payload-link.bin");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(monolithic_closure_from_file(&link, 1024).is_err());
        assert!(monolithic_closure_from_file(&target, 1024).is_ok());
    }
}
