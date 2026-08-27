use crate::model::{
    ClosureKind, ClosureRole, PayloadClosure, CHUNK_MANIFEST_CODEC, CURRENT_PAYLOAD_CODEC,
    SNAPSHOT_CHUNK_CODEC,
};
use crate::validation::DescriptorError;

use super::{invalid, require_equal, validate_content};

// r[impl chaoscontrol.snapshot_descriptor.closure]
pub fn validate_payload_closure(closure: &PayloadClosure) -> Result<(), DescriptorError> {
    validate_content(&closure.logical_payload)?;
    require_equal(
        "payload-codec",
        &closure.logical_payload.codec,
        CURRENT_PAYLOAD_CODEC,
    )?;
    if closure.members.is_empty() || closure.members.len() > crate::model::MAX_CLOSURE_MEMBERS {
        return invalid("closure-members", "closure is empty or too large");
    }
    match closure.kind {
        ClosureKind::Monolithic => validate_monolithic(closure),
        ClosureKind::Chunked => validate_chunked(closure),
    }
}

fn validate_monolithic(closure: &PayloadClosure) -> Result<(), DescriptorError> {
    if closure.manifest.is_some() || closure.members.len() != 1 {
        return invalid(
            "monolithic-closure",
            "monolithic closure must contain one payload member",
        );
    }
    let member = &closure.members[0];
    if member.order != 0
        || member.role != ClosureRole::SnapshotPayload
        || member.content != closure.logical_payload
    {
        return invalid(
            "monolithic-closure",
            "monolithic payload member does not match logical payload",
        );
    }
    Ok(())
}

fn validate_chunked(closure: &PayloadClosure) -> Result<(), DescriptorError> {
    let manifest = closure.manifest.as_ref().ok_or_else(|| {
        DescriptorError::new("chunk-manifest", "chunk manifest is missing".into())
    })?;
    validate_content(manifest)?;
    require_equal(
        "chunk-manifest-codec",
        &manifest.codec,
        CHUNK_MANIFEST_CODEC,
    )?;
    let mut total = 0_u64;
    for (index, member) in closure.members.iter().enumerate() {
        let order = u32::try_from(index)
            .map_err(|_| DescriptorError::new("chunk-order", "chunk order exceeds u32".into()))?;
        if member.order != order || member.role != ClosureRole::SnapshotChunk {
            return invalid(
                "chunk-order",
                "chunk members are missing, reordered, or have the wrong role",
            );
        }
        validate_content(&member.content)?;
        require_equal("chunk-codec", &member.content.codec, SNAPSHOT_CHUNK_CODEC)?;
        total = total
            .checked_add(member.content.length_bytes)
            .ok_or_else(|| {
                DescriptorError::new("chunk-length", "chunk length sum overflowed".into())
            })?;
    }
    if total != closure.logical_payload.length_bytes {
        return invalid(
            "chunk-length",
            "chunk lengths do not cover the logical payload",
        );
    }
    Ok(())
}
