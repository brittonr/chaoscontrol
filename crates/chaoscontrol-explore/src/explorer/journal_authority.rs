//! Pure admission of the caller's journal root.
//!
//! Admission supplies a root name, not filesystem access or durable evidence.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum JournalAuthorityError {
    Missing,
    Empty,
    ContainsNul,
}

impl JournalAuthorityError {
    pub(super) const fn message(self) -> &'static str {
        match self {
            Self::Missing => {
                "Campaign requires an explicit journal root. Set output_dir before exploration."
            }
            Self::Empty => "The journal root is empty. Set output_dir to an explicit directory.",
            Self::ContainsNul => {
                "The journal root contains a NUL byte. Supply a valid directory name."
            }
        }
    }
}

pub(super) fn admit_root(output_dir: Option<&str>) -> Result<&str, JournalAuthorityError> {
    let root = output_dir.ok_or(JournalAuthorityError::Missing)?;
    if root.is_empty() {
        return Err(JournalAuthorityError::Empty);
    }
    if root.contains('\0') {
        return Err(JournalAuthorityError::ContainsNul);
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::{admit_root, JournalAuthorityError};

    #[test]
    fn explicit_roots_remain_exact() {
        for root in [".", "reports/run", "/reports/run", " spaced directory "] {
            assert_eq!(admit_root(Some(root)), Ok(root));
        }
    }

    #[test]
    fn absent_and_malformed_authority_are_rejected() {
        for (root, error) in [
            (None, JournalAuthorityError::Missing),
            (Some(""), JournalAuthorityError::Empty),
            (Some("reports\0run"), JournalAuthorityError::ContainsNul),
        ] {
            assert_eq!(admit_root(root), Err(error));
            assert!(!error.message().is_empty());
        }
    }
}
