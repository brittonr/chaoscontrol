//! Pure journal directory layout and entry-name classification.
//!
//! The `epoch-` and `seg-` name prefixes are reserved: an entry that uses one
//! must be canonical. Every other name is unrelated and left untouched.

/// Journal directory created under the admitted `output_dir`.
pub const JOURNAL_DIRECTORY_NAME: &str = "campaign-journal";
/// Exclusive writer lock file inside the journal directory.
pub const LOCK_FILE_NAME: &str = "LOCK";
/// Prefix of epoch directory names (`epoch-NNNN`).
pub const EPOCH_DIRECTORY_PREFIX: &str = "epoch-";
/// Decimal digits in a canonical epoch directory name.
pub const EPOCH_NAME_DIGITS: usize = 4;
/// Largest epoch representable by [`EPOCH_NAME_DIGITS`] digits.
pub const MAX_EPOCH: u32 = 9_999;
/// Prefix of segment file names (`seg-NNNNNNNN.cbor`).
pub const SEGMENT_FILE_PREFIX: &str = "seg-";
/// Suffix of segment file names.
pub const SEGMENT_FILE_SUFFIX: &str = ".cbor";
/// Decimal digits in a canonical segment file name.
pub const SEGMENT_NAME_DIGITS: usize = 8;
/// Largest segment sequence representable by [`SEGMENT_NAME_DIGITS`] digits.
pub const MAX_SEGMENT_SEQ: u64 = 99_999_999;
/// Prefix of staged segment files. Recovery ignores and never removes them.
pub const STAGE_FILE_PREFIX: &str = ".seg-stage-";

/// Canonical epoch directory name.
pub fn epoch_directory_name(epoch: u32) -> String {
    format!("{EPOCH_DIRECTORY_PREFIX}{epoch:0EPOCH_NAME_DIGITS$}")
}

/// Canonical segment file name.
pub fn segment_file_name(seq: u64) -> String {
    format!("{SEGMENT_FILE_PREFIX}{seq:0SEGMENT_NAME_DIGITS$}{SEGMENT_FILE_SUFFIX}")
}

/// Classification of one journal-root entry name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::explorer::journal) enum RootEntry {
    Lock,
    Epoch(u32),
    /// A reserved epoch name with a non-canonical digit count.
    EpochAlias(u64),
    MalformedEpoch,
    Unrelated,
}

/// Classification of one epoch-directory entry name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::explorer::journal) enum EpochEntry {
    Segment(u64),
    /// A reserved segment name with a non-canonical digit count.
    SegmentAlias(u64),
    MalformedSegment,
    Stage,
    Unrelated,
}

pub(in crate::explorer::journal) fn classify_root_entry(name: &[u8]) -> RootEntry {
    if name == LOCK_FILE_NAME.as_bytes() {
        return RootEntry::Lock;
    }
    let Some(digits) = name.strip_prefix(EPOCH_DIRECTORY_PREFIX.as_bytes()) else {
        return RootEntry::Unrelated;
    };
    match parse_decimal(digits) {
        None => RootEntry::MalformedEpoch,
        Some(epoch) if digits.len() == EPOCH_NAME_DIGITS => {
            u32::try_from(epoch).map_or(RootEntry::MalformedEpoch, RootEntry::Epoch)
        }
        Some(epoch) => RootEntry::EpochAlias(epoch),
    }
}

pub(in crate::explorer::journal) fn classify_epoch_entry(name: &[u8]) -> EpochEntry {
    if name.starts_with(STAGE_FILE_PREFIX.as_bytes()) {
        return EpochEntry::Stage;
    }
    let Some(rest) = name.strip_prefix(SEGMENT_FILE_PREFIX.as_bytes()) else {
        return EpochEntry::Unrelated;
    };
    let parsed = rest
        .strip_suffix(SEGMENT_FILE_SUFFIX.as_bytes())
        .and_then(|digits| parse_decimal(digits).map(|seq| (digits.len(), seq)));
    match parsed {
        None => EpochEntry::MalformedSegment,
        Some((SEGMENT_NAME_DIGITS, seq)) => EpochEntry::Segment(seq),
        Some((_, seq)) => EpochEntry::SegmentAlias(seq),
    }
}

fn parse_decimal(digits: &[u8]) -> Option<u64> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0_u64, |value, digit| {
        let digit = char::from(*digit).to_digit(10)?;
        value.checked_mul(10)?.checked_add(u64::from(digit))
    })
}
