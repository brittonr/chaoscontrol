//! Pure journal format: names, bounds, segment codec, and chain verification.
//!
//! Nothing here touches the filesystem; the journal shell supplies bytes and
//! names and acts on the typed results.

pub(super) mod bounds;
mod codec;
pub(super) mod layout;
pub(super) mod segment;
pub(super) mod verify;
