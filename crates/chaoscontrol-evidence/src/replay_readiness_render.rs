//! Pure rendering from validated replay-readiness models.

// r[impl chaoscontrol.architecture_modules.evidence]
// r[impl chaoscontrol.architecture_modules.boundary]

pub const README_START_MARKER: &str = "<!-- replay-readiness-status:start -->";
pub const README_END_MARKER: &str = "<!-- replay-readiness-status:end -->";

pub fn render_readme_status_block(summary_line: &str) -> String {
    format!("{README_START_MARKER}\n> **Replay readiness checks:** `{summary_line}`\n>\n> This status reports bounded static gate execution. Historical workload rows remain blocked until fresh admitted v2 KVM evidence exists. A passed status does not promote a workload. It is not a claim of universal determinism.\n{README_END_MARKER}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_preserves_validated_summary_without_reclassification() {
        let summary = "replay-readiness status=passed";
        let rendered = render_readme_status_block(summary);
        assert!(rendered.contains(summary));
        assert!(rendered.starts_with(README_START_MARKER));
        assert!(rendered.ends_with(README_END_MARKER));
    }

    #[test]
    fn renderer_does_not_promote_failed_text() {
        let summary = "replay-readiness status=failed";
        let rendered = render_readme_status_block(summary);
        assert!(rendered.contains("status=failed"));
        assert!(!rendered.contains("status=passed"));
    }
}
