use chaoscontrol_protocol::assertion_identity::AssertionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalAssertionVerdict {
    Passed,
    Failed,
    Unexercised,
}

pub(crate) fn derive_local_verdict(
    kind: AssertionKind,
    success_count: u64,
    failure_count: u64,
) -> LocalAssertionVerdict {
    let exercised = success_count > 0 || failure_count > 0;
    match kind {
        AssertionKind::Always => {
            if failure_count > 0 {
                LocalAssertionVerdict::Failed
            } else if exercised {
                LocalAssertionVerdict::Passed
            } else {
                LocalAssertionVerdict::Unexercised
            }
        }
        AssertionKind::Sometimes => {
            if success_count > 0 {
                LocalAssertionVerdict::Passed
            } else if exercised {
                LocalAssertionVerdict::Failed
            } else {
                LocalAssertionVerdict::Unexercised
            }
        }
        AssertionKind::Reachable => {
            if success_count > 0 {
                LocalAssertionVerdict::Passed
            } else {
                LocalAssertionVerdict::Unexercised
            }
        }
        AssertionKind::Unreachable => {
            if failure_count > 0 {
                LocalAssertionVerdict::Failed
            } else {
                LocalAssertionVerdict::Passed
            }
        }
    }
}

pub(crate) fn report_kind(value: &str) -> Option<AssertionKind> {
    match value {
        "always" => Some(AssertionKind::Always),
        "sometimes" => Some(AssertionKind::Sometimes),
        "reachable" | "reachability" => Some(AssertionKind::Reachable),
        "unreachable" => Some(AssertionKind::Unreachable),
        _ => None,
    }
}

pub(crate) fn blocks_as_unobserved(kind: AssertionKind, observed: bool) -> bool {
    !observed && kind != AssertionKind::Unreachable
}

pub(crate) fn counts_match_kind(
    kind: AssertionKind,
    success_count: u64,
    failure_count: u64,
) -> bool {
    match kind {
        AssertionKind::Always | AssertionKind::Sometimes => true,
        AssertionKind::Reachable => failure_count == 0,
        AssertionKind::Unreachable => success_count == 0,
    }
}

pub(crate) fn sorted_strings(values: &[String]) -> Vec<String> {
    let mut sorted = values.to_vec();
    sorted.sort();
    sorted
}

pub(crate) fn sorted_owned(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_each_kind_without_counting_observations_as_assertions() {
        assert_eq!(
            derive_local_verdict(AssertionKind::Always, 1, 1),
            LocalAssertionVerdict::Failed
        );
        assert_eq!(
            derive_local_verdict(AssertionKind::Sometimes, 1, 1),
            LocalAssertionVerdict::Passed
        );
        assert_eq!(
            derive_local_verdict(AssertionKind::Reachable, 0, 0),
            LocalAssertionVerdict::Unexercised
        );
        assert_eq!(
            derive_local_verdict(AssertionKind::Unreachable, 0, 0),
            LocalAssertionVerdict::Passed
        );
        assert_eq!(
            derive_local_verdict(AssertionKind::Unreachable, 0, 1),
            LocalAssertionVerdict::Failed
        );
    }

    #[test]
    fn rejects_unknown_report_kind() {
        assert_eq!(report_kind("unknown"), None);
    }
}
