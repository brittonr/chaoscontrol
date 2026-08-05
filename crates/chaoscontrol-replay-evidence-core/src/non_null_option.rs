//! Serde helper: deserialize an optional field that rejects explicit null.
//!
//! Used for evidence identity fields where a JSON `null` is a malformed
//! carrier, while an absent field means a legacy record.

use serde::{Deserialize, Deserializer};

pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
