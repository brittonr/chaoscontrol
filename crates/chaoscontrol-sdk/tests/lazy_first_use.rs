#![cfg(feature = "full")]

use chaoscontrol_sdk::prelude::*;
use serde_json::{json, Value};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_MODE: &str = "CHAOSCONTROL_TEST_FIRST_USE";
const CHILD_DEADLINE: Duration = Duration::from_secs(10);
const CONCURRENT_CALLERS: usize = 4;

// These properties must be declared even though their bodies never execute.
#[allow(dead_code)]
fn unexecuted_properties() {
    cc_assert_always_gt!(2, 1, "unexecuted numeric property");
    cc_assert_sometimes_stable!(
        "org.example.lazy",
        "stable",
        "guest",
        "operation",
        true,
        "unexecuted stable property"
    );
}

// r[verify chaoscontrol.sdk_first_use.initialization]
// r[verify chaoscontrol.sdk_first_use.idempotence]
#[test]
fn first_use_initializes_once_before_observations() {
    for mode in [
        "event",
        "setup",
        "random",
        "index",
        "stable_guidance",
        "compat_guidance",
        "concurrent",
        "fast_paths",
    ] {
        let output = std::env::temp_dir().join(format!(
            "chaoscontrol-first-use-{}-{mode}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&output);
        let mut child = Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", "first_use_child", "--ignored", "--nocapture"])
            .env(CHILD_MODE, mode)
            .env(chaoscontrol_sdk::LOCAL_OUTPUT, &output)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn child");
        let deadline = Instant::now() + CHILD_DEADLINE;
        loop {
            if child.try_wait().expect("child status").is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill().expect("kill stalled child");
                let result = child.wait_with_output().expect("reap stalled child");
                panic!(
                    "first-use {mode} stalled (possible initialization recursion): {}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let result = child.wait_with_output().expect("child output");
        assert!(
            result.status.success(),
            "first-use {mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if mode == "fast_paths" {
            assert!(
                !output.exists(),
                "entropy-free operations must not initialize output"
            );
            continue;
        }
        let content = std::fs::read_to_string(&output).expect("local output");
        let records: Vec<Value> = content
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSON record"))
            .collect();
        assert_eq!(
            records
                .iter()
                .filter(|record| record.get("antithesis_sdk").is_some())
                .count(),
            1,
            "metadata once for {mode}"
        );
        assert!(
            records[0].get("antithesis_sdk").is_some(),
            "metadata precedes all observations"
        );
        let completed: Vec<_> = records
            .iter()
            .enumerate()
            .filter(|(_, record)| record["chaoscontrol_assertion_catalog"]["record"] == "complete")
            .collect();
        assert_eq!(completed.len(), 1, "catalog completed once for {mode}");
        let assertions: Vec<_> = records
            .iter()
            .filter_map(|record| record.get("antithesis_assert"))
            .collect();
        assert_eq!(assertions.len(), 2, "unexecuted properties remain visible");
        for assertion in assertions {
            assert_eq!(
                assertion["hit"], false,
                "initialization must not fabricate hits"
            );
            assert_eq!(assertion["catalog_status"], "accepted");
            assert_eq!(assertion["must_hit"], true);
        }
        let guidance: Vec<_> = records
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                record
                    .get("antithesis_guidance")
                    .map(|value| (index, value))
            })
            .collect();
        let declarations: Vec<_> = guidance
            .iter()
            .filter(|(_, record)| record["hit"] == false)
            .collect();
        assert_eq!(
            declarations.len(),
            1,
            "numeric guidance declaration once, without recursion"
        );
        assert_eq!(declarations[0].1["message"], "unexecuted numeric property");
        if matches!(mode, "stable_guidance" | "compat_guidance") {
            let hits: Vec<_> = guidance
                .iter()
                .filter(|(_, record)| record["hit"] == true)
                .collect();
            assert_eq!(hits.len(), 1);
            assert!(hits[0].0 > completed[0].0);
            assert_eq!(hits[0].1["guidance_data"], json!({"score": 7}));
            assert_eq!(hits[0].1["catalog_status"], "accepted");
        }
        if mode == "event" {
            let index = records
                .iter()
                .position(|record| record.get("first_event").is_some())
                .expect("event");
            assert!(index > completed[0].0);
            assert_eq!(records[index]["first_event"], json!({"step": 1}));
        }
        if mode == "setup" {
            let index = records
                .iter()
                .position(|record| record.get("antithesis_setup").is_some())
                .expect("setup");
            assert!(index > completed[0].0);
            assert_eq!(
                records[index]["antithesis_setup"]["details"],
                json!({"ready": true})
            );
        }
        if mode == "concurrent" {
            let mut workers = Vec::new();
            for (index, record) in records
                .iter()
                .enumerate()
                .filter(|(_, record)| record.get("worker").is_some())
            {
                assert!(index > completed[0].0);
                workers.push(record["worker"]["id"].as_u64().expect("worker ID"));
            }
            workers.sort_unstable();
            assert_eq!(workers, (0..CONCURRENT_CALLERS as u64).collect::<Vec<_>>());
        }
        std::fs::remove_file(output).expect("remove local output");
    }
}

#[test]
#[ignore = "fresh-process fixture, invoked by first_use_initializes_once_before_observations"]
fn first_use_child() {
    let mode = std::env::var(CHILD_MODE).expect("child mode");
    match mode.as_str() {
        "event" => send_event("first_event", &json!({"step": 1})),
        "setup" => setup_complete(&json!({"ready": true})),
        "random" => {
            let _ = get_random();
        }
        "index" => {
            assert!(random_choice_index(3) < 3);
        }
        "stable_guidance" => cc_guidance_json!(
            "org.example.lazy",
            "stable",
            AssertionKind::Sometimes,
            "unexecuted stable property",
            true,
            json!({"score": 7})
        ),
        "compat_guidance" => chaoscontrol_sdk::guidance::emit_json(
            chaoscontrol_sdk::assert::location_id("org.example.lazy:stable"),
            AssertionKind::Sometimes,
            "unexecuted stable property",
            module_path!(),
            "first_use_child",
            file!(),
            line!(),
            column!(),
            true,
            json!({"score": 7}),
        ),
        "concurrent" => {
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(CONCURRENT_CALLERS));
            let workers: Vec<_> = (0..CONCURRENT_CALLERS)
                .map(|id| {
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        barrier.wait();
                        send_event("worker", &json!({"id": id}));
                    })
                })
                .collect();
            for worker in workers {
                worker.join().expect("event caller");
            }
        }
        "fast_paths" => {
            assert_eq!(random_choice::<u8>(&[]), None);
            assert_eq!(random_choice(&[7]), Some(&7));
            assert_eq!(random_choice_index(1), 0);
            fill_bytes(&mut []);
            return;
        }
        _ => panic!("unexpected child mode"),
    }
    // Check before explicit initialization: random-only use emits no ordinary
    // output, so checking solely at process exit would miss this regression.
    let path = std::env::var(chaoscontrol_sdk::LOCAL_OUTPUT).expect("output path");
    let first_use =
        std::fs::read_to_string(path).expect("first operation must emit metadata/catalog");
    let first: Value =
        serde_json::from_str(first_use.lines().next().expect("first record")).unwrap();
    assert!(
        first.get("antithesis_sdk").is_some(),
        "metadata must precede first-use output"
    );
    chaoscontrol_init();
    chaoscontrol_init();
    send_event("after_explicit", &json!({"ok": true}));
}
