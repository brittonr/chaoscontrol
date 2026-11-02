//! Bounded mapping from admitted protocol-novelty identities to explorer guidance.
//!
//! Evidence keeps the full BLAKE3 identity; the explorer only needs a stable
//! compact slot so branches that reach the same protocol state rank together.
use std::collections::{BTreeMap, BTreeSet};

/// Maximum retained full identities before the index refuses more.
pub const MAX_RETAINED_IDENTITIES: usize = 4096;

const DOMAIN: &[u8] = b"chaoscontrol.protocol-novelty-slot.v1\0";

/// Deterministic coverage slot for one full protocol-novelty identity.
///
/// Returns `None` for anything that is not an exact lowercase BLAKE3 reference.
#[must_use]
pub fn novelty_slot(novelty_ref: &str) -> Option<usize> {
    let hex = novelty_ref.strip_prefix("blake3:")?;
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return None;
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(DOMAIN);
    hasher.update(novelty_ref.as_bytes());
    let digest = hasher.finalize();
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&digest.as_bytes()[..8]);
    Some((u64::from_be_bytes(prefix) as usize) % crate::coverage::MAP_SIZE)
}

/// Full protocol-novelty identities grouped by compact guidance slot.
///
/// A slot is guidance only. Two identities may share a slot, and the index keeps
/// both, so a slot collision never claims equal protocol state.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProtocolNoveltyIndex {
    identities: BTreeSet<String>,
    slots: BTreeMap<usize, BTreeSet<String>>,
}

impl ProtocolNoveltyIndex {
    /// Record one full identity and return whether it is newly retained.
    ///
    /// # Errors
    ///
    /// Rejects a malformed identity and any identity beyond the retention bound.
    pub fn record(&mut self, novelty_ref: &str) -> Result<bool, String> {
        let slot = novelty_slot(novelty_ref)
            .ok_or_else(|| String::from("protocol novelty identity is not BLAKE3"))?;
        if !self.identities.contains(novelty_ref)
            && self.identities.len() >= MAX_RETAINED_IDENTITIES
        {
            return Err(String::from("protocol novelty identity bound exceeded"));
        }
        let is_new = self.identities.insert(novelty_ref.to_string());
        self.slots
            .entry(slot)
            .or_default()
            .insert(novelty_ref.to_string());
        Ok(is_new)
    }

    /// Number of distinct compact slots in use.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Full identities retained for one compact slot.
    #[must_use]
    pub fn identities_in(&self, slot: usize) -> Option<&BTreeSet<String>> {
        self.slots.get(&slot)
    }

    /// Whether one full identity is retained.
    #[must_use]
    pub fn retains(&self, novelty_ref: &str) -> bool {
        self.identities.contains(novelty_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(value: &str) -> String {
        format!("blake3:{}", blake3::hash(value.as_bytes()).to_hex())
    }

    #[test]
    fn slots_are_deterministic_and_bounded() {
        let identity = reference("protocol-state");
        let slot = novelty_slot(&identity).expect("valid identity");
        assert!(slot < crate::coverage::MAP_SIZE);
        assert_eq!(Some(slot), novelty_slot(&identity));
    }

    #[test]
    fn malformed_identities_are_rejected() {
        for invalid in ["", "blake3:", "sha256:0000", "blake3:ABCDEF", "blake3:zzzz"] {
            assert!(novelty_slot(invalid).is_none(), "{invalid}");
            assert!(ProtocolNoveltyIndex::default().record(invalid).is_err());
        }
    }

    #[test]
    fn collisions_retain_every_full_identity() {
        let mut index = ProtocolNoveltyIndex::default();
        let mut by_slot: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for value in (0..256).map(|n| format!("state-{n}")) {
            let identity = reference(&value);
            assert!(index.record(&identity).expect("record"));
            by_slot
                .entry(novelty_slot(&identity).expect("slot"))
                .or_default()
                .push(identity);
        }
        for (slot, identities) in by_slot {
            let retained = index.identities_in(slot).expect("slot retained");
            for identity in &identities {
                assert!(retained.contains(identity));
                assert!(index.retains(identity));
            }
            assert_eq!(retained.len(), identities.len());
        }
        assert!(!index.record(&reference("state-0")).expect("duplicate"));
    }
}
