use chaoscontrol_evidence::{check_sdk_assertion_quality_path, summarize_sdk_local_jsonl};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;

const EVIDENCE_CLASS: &str = "instrumentation-dry-run";
const MAX_SDK_JSONL_BYTES: u64 = 16 * 1024 * 1024;
const FIFO_MODE: libc::mode_t = 0o600;

fn legacy_report() -> serde_json::Value {
    summarize_sdk_local_jsonl(
        "{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"id\":\"1\",\"message\":\"legacy\",\"details\":{}}}\n",
        EVIDENCE_CLASS,
        None,
    )
    .expect("legacy report")
}

#[test]
fn quality_path_uses_one_regular_no_follow_file() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let report_path = temp.path().join("report.json");
    std::fs::write(
        &report_path,
        serde_json::to_vec(&legacy_report()).expect("report JSON"),
    )
    .expect("write report");
    assert!(
        !check_sdk_assertion_quality_path(&report_path)
            .expect("regular report")
            .passed
    );

    let symlink_path = temp.path().join("report-link.json");
    symlink(&report_path, &symlink_path).expect("create symlink");
    assert!(check_sdk_assertion_quality_path(&symlink_path).is_err());
    assert!(check_sdk_assertion_quality_path(temp.path()).is_err());

    let fifo_path = temp.path().join("report.fifo");
    let fifo = CString::new(fifo_path.as_os_str().as_bytes()).expect("FIFO path");
    let result = unsafe { libc::mkfifo(fifo.as_ptr(), FIFO_MODE) };
    assert_eq!(result, 0, "create FIFO");
    assert!(check_sdk_assertion_quality_path(&fifo_path).is_err());
    assert!(check_sdk_assertion_quality_path("/dev/null").is_err());

    let oversized_path = temp.path().join("oversized.json");
    let oversized = std::fs::File::create(&oversized_path).expect("create sparse fixture");
    oversized
        .set_len(MAX_SDK_JSONL_BYTES + 1)
        .expect("size sparse fixture");
    assert!(check_sdk_assertion_quality_path(&oversized_path).is_err());
}
