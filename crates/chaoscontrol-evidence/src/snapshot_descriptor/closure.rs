use sha2::Digest as _;

const HEX_ALPHABET_BYTES: usize = 16;
const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HEX_HIGH_NIBBLE_SHIFT: u32 = 4;
const HEX_LOW_NIBBLE_MASK: u8 = 0x0f;
const HEX_DIGITS: &[u8; HEX_ALPHABET_BYTES] = b"0123456789abcdef";

// r[impl chaoscontrol.snapshot_descriptor.closure]
pub fn monolithic_closure_from_file(
    path: impl AsRef<std::path::Path>,
    maximum_bytes: u64,
) -> crate::EvidenceResult<::chaoscontrol_snapshot_descriptor::PayloadClosure> {
    let path = path.as_ref();
    let metadata = std::fs::metadata(path)?;
    if metadata.len() == 0 || metadata.len() > maximum_bytes {
        return Err(crate::EvidenceError::new(format!(
            "snapshot payload length {} is outside the declared bound {maximum_bytes}",
            metadata.len()
        )));
    }
    let bytes = std::fs::read(path)?;
    let logical_payload = ::chaoscontrol_snapshot_descriptor::ContentIdentity {
        digest: ::chaoscontrol_snapshot_descriptor::digest_bytes(
            ::chaoscontrol_snapshot_descriptor::DigestAlgorithm::Sha256,
            &bytes,
        ),
        length_bytes: metadata.len(),
        codec: ::chaoscontrol_snapshot_descriptor::CURRENT_PAYLOAD_CODEC.to_string(),
    };
    let closure = ::chaoscontrol_snapshot_descriptor::PayloadClosure {
        kind: ::chaoscontrol_snapshot_descriptor::ClosureKind::Monolithic,
        logical_payload: logical_payload.clone(),
        manifest: None,
        members: vec![::chaoscontrol_snapshot_descriptor::ClosureMember {
            order: 0,
            role: ::chaoscontrol_snapshot_descriptor::ClosureRole::SnapshotPayload,
            content: logical_payload,
        }],
    };
    ::chaoscontrol_snapshot_descriptor::validate_payload_closure(&closure)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    Ok(closure)
}

pub fn chunked_closure_from_manifest(
    manifest_path: impl AsRef<std::path::Path>,
    artifact_root: impl AsRef<std::path::Path>,
    maximum_bytes: u64,
) -> crate::EvidenceResult<::chaoscontrol_snapshot_descriptor::PayloadClosure> {
    let manifest_path = manifest_path.as_ref();
    let artifact_root = artifact_root.as_ref();
    let manifest_bytes = std::fs::read(manifest_path)?;
    let manifest: crate::SnapshotChunkManifest = serde_json::from_slice(&manifest_bytes)?;
    manifest.validate_shape()?;
    if manifest.original_size == 0 || manifest.original_size > maximum_bytes {
        return Err(crate::EvidenceError::new(format!(
            "chunked snapshot length {} is outside the declared bound {maximum_bytes}",
            manifest.original_size
        )));
    }
    if manifest.chunks.len() > ::chaoscontrol_snapshot_descriptor::MAX_CLOSURE_MEMBERS {
        return Err(crate::EvidenceError::new(
            "snapshot chunk count exceeds the public bound",
        ));
    }
    let mut aggregate = sha2::Sha256::new();
    let mut total_bytes = 0_u64;
    let mut members = Vec::with_capacity(manifest.chunks.len());
    for (index, chunk) in manifest.chunks.iter().enumerate() {
        let path = artifact_root.join(&chunk.path);
        let metadata = std::fs::metadata(&path).map_err(|error| {
            crate::EvidenceError::new(format!(
                "snapshot chunk missing {}: {error}",
                path.display()
            ))
        })?;
        if metadata.len() != chunk.size || metadata.len() > maximum_bytes {
            return Err(crate::EvidenceError::new(format!(
                "snapshot chunk length mismatch for {}",
                path.display()
            )));
        }
        let bytes = std::fs::read(&path)?;
        let actual = ::chaoscontrol_snapshot_descriptor::digest_bytes(
            ::chaoscontrol_snapshot_descriptor::DigestAlgorithm::Sha256,
            &bytes,
        );
        if !actual.hex.eq_ignore_ascii_case(&chunk.sha256) {
            return Err(crate::EvidenceError::new(format!(
                "snapshot chunk digest mismatch for {}",
                path.display()
            )));
        }
        aggregate.update(&bytes);
        total_bytes = total_bytes.checked_add(metadata.len()).ok_or_else(|| {
            crate::EvidenceError::new("snapshot chunk length accumulation overflowed")
        })?;
        let order = u32::try_from(index)
            .map_err(|_| crate::EvidenceError::new("snapshot chunk order exceeds u32"))?;
        members.push(::chaoscontrol_snapshot_descriptor::ClosureMember {
            order,
            role: ::chaoscontrol_snapshot_descriptor::ClosureRole::SnapshotChunk,
            content: ::chaoscontrol_snapshot_descriptor::ContentIdentity {
                digest: actual,
                length_bytes: metadata.len(),
                codec: ::chaoscontrol_snapshot_descriptor::SNAPSHOT_CHUNK_CODEC.to_string(),
            },
        });
    }
    let aggregate_hex = hex_lower(aggregate.finalize().as_slice());
    if total_bytes != manifest.original_size
        || !aggregate_hex.eq_ignore_ascii_case(&manifest.original_sha256)
    {
        return Err(crate::EvidenceError::new(
            "chunked snapshot aggregate length or digest mismatch",
        ));
    }
    let closure = ::chaoscontrol_snapshot_descriptor::PayloadClosure {
        kind: ::chaoscontrol_snapshot_descriptor::ClosureKind::Chunked,
        logical_payload: ::chaoscontrol_snapshot_descriptor::ContentIdentity {
            digest: ::chaoscontrol_snapshot_descriptor::TaggedDigest {
                algorithm: ::chaoscontrol_snapshot_descriptor::DigestAlgorithm::Sha256,
                hex: aggregate_hex,
            },
            length_bytes: manifest.original_size,
            codec: ::chaoscontrol_snapshot_descriptor::CURRENT_PAYLOAD_CODEC.to_string(),
        },
        manifest: Some(::chaoscontrol_snapshot_descriptor::ContentIdentity {
            digest: ::chaoscontrol_snapshot_descriptor::digest_bytes(
                ::chaoscontrol_snapshot_descriptor::DigestAlgorithm::Blake3,
                &manifest_bytes,
            ),
            length_bytes: u64::try_from(manifest_bytes.len())
                .map_err(|_| crate::EvidenceError::new("manifest length exceeds u64"))?,
            codec: ::chaoscontrol_snapshot_descriptor::CHUNK_MANIFEST_CODEC.to_string(),
        }),
        members,
    };
    ::chaoscontrol_snapshot_descriptor::validate_payload_closure(&closure)
        .map_err(|error| crate::EvidenceError::new(error.to_string()))?;
    Ok(closure)
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
