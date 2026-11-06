# Dated bounded workspace proof; Rust validators establish runtime bindings.
let P = import "../../../../contracts/evidence/profile-primitives.ncl" in
let B3 = std.contract.from_predicate (fun value => std.is_string value && std.string.is_match "^b3:[0-9a-f]{64}$" value) in
let FalseOnly = std.contract.from_predicate (fun value => value == false) in
let Artifact = { path | P.non_empty_string, digest | B3, bytes | P.u64 } in
{
  schema | P.exact_string "chaoscontrol.debug-query-workspace-proof.v1" = "chaoscontrol.debug-query-workspace-proof.v1",
  workspace_verdict = "bounded-pass",
  repository_verdict = "known-gap",
  git_revision = "3d2abb09e6b68ee4403557ed76a281fe21654a9b",
  source_state = "Uncommitted session changes over this HEAD; CLI digest sampled after the proof, not a source-to-binary attestation.",
  maintenance_owner = "ChaosControl replay, SDK supervisor, and VMM maintainers",
  consumer = "First-party Rust cooperating-process workload and local query-events operators",
  durable_capability = "Exact local checkpoint/moment queries, bounded static graphs, and allowlisted restored-guest observations with validated receipts",
  local_artifacts = "/tmp/cc-debug-query-8s9axtnv",
  config_identity | B3 = "b3:38c7c6e479820badf2c58b12031fdf44d235078c855958b4706066b234ba08fc",
  session_identity | B3 = "b3:a2ea50f834604c9be54f3f28e3919edc312c9ad0fd1878f7ed1b2a6354d878fd",
  parameters = {
    "seed" = 42,
    "vms" = 1,
    "memory_mib" = 128,
    "quantum" = 100,
    "bootstrap_tick_limit" = 10000,
    "retained_history_ticks" = 40,
    "checkpoint_interval" = 20,
    "setup_completed_tick" = 1170,
    "end_tick" = 1210,
    "checkpoint_ticks" = [1190, 1210],
    "observation_checkpoint_id" = 1,
    "observation_deadline_tick" = 5000,
  },
  query = {
    "ast" = "All",
    "scanned" = 374,
    "matched" = 374,
    "rows" = 374,
    "serial_rows" = 369,
    "fault_stage_rows" = 5,
    "truncated" = false,
    "result_and_receipt_rerun_byte_identical" = true,
  },
  timeline = {
    "formats" = ["markdown", "dot"],
    "nodes" = 378,
    "edges" = 2,
    "event_rows" = 374,
    "truncated" = false,
    "missing_artifacts" = ["frontier trace not retained by explicit session capture"],
  },
  expected_stdout = {
    "bytes" = 10,
    "digest" = "b3:790ae6bf0978f2a53b34e71f5ede2be9baa7aac0d7b66071bbcabc4c7170b51f",
    "meaning" = "committed followed by one newline",
  },
  artifact_hashes | Array Artifact = [{
    "path" = "/home/brittonr/.cargo-target/debug/query-events",
    "digest" = "b3:ec46ad86d005c312b797c93289cf78433358878433f1b7886d4c6f784c05c5e2",
    "bytes" = 232921624,
  }, {
    "path" = "/nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux",
    "digest" = "b3:f2abae8a52c83c975ed15de504535445e2ac159cc0198f8028867b0c18cdf67c",
    "bytes" = 441789520,
  }, {
    "path" = "/nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/initrd",
    "digest" = "b3:bdab04c42d49ae3f75468e1c2e6408c4829bcd5c17d8792ac2e090eeeeb972fa",
    "bytes" = 600898,
  }, {
    "path" = "/nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/profile.json",
    "digest" = "b3:085fcf9b24315208bf91dc8b37f6bb155a88ea9151c0085fdcf7cc1cdee7d50a",
    "bytes" = 528,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/session/session.json",
    "digest" = "b3:233a4c42b3280a3db19b7a958a10481952cab235b40cd5773ae3e903a7db9874",
    "bytes" = 81899,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/session/recording.json",
    "digest" = "b3:d49d1a37e8499ac5500a7d0d45e14940cca3d44c33ca2b5f9cbe93a312ada591",
    "bytes" = 44354,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/result.json",
    "digest" = "b3:b33814745ed958a765328f3fe0918dacbe3c03ec07e0243b067e32c49c511ef4",
    "bytes" = 173589,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/query-receipt.json",
    "digest" = "b3:c5eef3762d835a4e28f13bc75b95eed366cf396a76d54f053957cc852f6c52d0",
    "bytes" = 1220,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/workspace.json",
    "digest" = "b3:cb0bc7dff0fd93cce9f833f1649db7ab53364f44bcc007b96bbe0cab2b5895c7",
    "bytes" = 348757,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/timeline.md",
    "digest" = "b3:18bffc962201ec9e183d2c19fda9622343d8ccbd70bd33acea3f4122ef9ae5b3",
    "bytes" = 65882,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/timeline-receipt.json",
    "digest" = "b3:a4fadcefb083efda8d85b53e0446c202cd0d7e11cab38d3dd42cab8f9e613ede",
    "bytes" = 1856,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/timeline.dot",
    "digest" = "b3:3751f0cd5b9ae84ab3497bb30bc96ba740cfe46339d5c970960664b63e53f2aa",
    "bytes" = 65276,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/dot-receipt.json",
    "digest" = "b3:cbeb5c8222318a668940fdad03493ebc11b1c1e147184904a079a98dac823610",
    "bytes" = 1853,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/observation.json",
    "digest" = "b3:ba67e5cb19edd2eddeb5da7bd96ba5aef3c2e07d52226a3a46cc11f75562ce72",
    "bytes" = 1749,
  }, {
    "path" = "/tmp/cc-debug-query-8s9axtnv/schedule.json",
    "digest" = "b3:b8140be08cf9c5340bd27df9ce43426efc7d349568237c6b6a6132b2ad72c6e8",
    "bytes" = 125,
  }],
  assertion_coverage = {
    "status" = "not-collected",
    "counts" = null,
    "nonclaim" = "This observation is not an assertion-quality campaign receipt.",
  },
  bug_reports = [],
  replay_attempts = [],
  execution_class = "bounded snapshot-restored observation; not bug replay or snapshot_backed_reproduced",
  checks = {
    "affected_matrix" = {
      "passed" = 2726,
      "failed" = 6,
      "ignored" = 16,
      "targets" = 196,
      "exit_status" = 101,
      "failed_targets" = ["chaoscontrol-explore/tests/assertion_summary_v2.rs", "chaoscontrol-explore/tests/assertion_summary_writer.rs"],
      "cause" = "Existing assertion-summary producer includes distinct_failing_assertions/liveness_gaps; evidence consumer denies these fields.",
    },
    "clippy_warnings_denied_exit_status" = 0,
    "rust_format_exit_status" = 0,
    "nix_format_exit_status" = 0,
    "nix_check_graph_no_build_exit_status" = 0,
    "nickel_fixture_gate_exit_status" = 0,
    "nickel_valid_fixtures" = 2,
    "nickel_contract_rejections" = 6,
    "octet" = {
      "exit_status" = 2,
      "warnings" = 395,
      "errors" = 1,
      "blocking_file" = "crates/chaoscontrol-smr/src/storage_schedule.rs:306",
      "blocking_lint" = "explicit_defaults",
    },
  },
  positive_commands = [{
    "command" = "nix build .#debug-observation-bundle --builders '' --no-link --print-out-paths -L",
    "pueue_task" = 71,
    "exit_status" = 0,
  }, {
    "command" = "nix build .#checks.x86_64-linux.debug-query-workspace-contracts --builders '' --no-link --print-out-paths -L",
    "pueue_task" = 27,
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events capture-session --kernel /nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/initrd --seed 42 --vms 1 --memory-mib 128 --quantum 100 --bootstrap-ticks 10000 --history-ticks 40 --checkpoint-interval 20 --schedule /tmp/cc-debug-query-8s9axtnv/schedule.json /tmp/cc-debug-query-8s9axtnv/session",
    "pueue_task" = 73,
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events query --session /tmp/cc-debug-query-8s9axtnv/session /tmp/cc-debug-query-8s9axtnv/query.json /tmp/cc-debug-query-8s9axtnv/result.json --receipt /tmp/cc-debug-query-8s9axtnv/query-receipt.json",
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events validate --session /tmp/cc-debug-query-8s9axtnv/session /tmp/cc-debug-query-8s9axtnv/query.json /tmp/cc-debug-query-8s9axtnv/result.json /tmp/cc-debug-query-8s9axtnv/query-receipt.json",
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events prepare-timeline --session /tmp/cc-debug-query-8s9axtnv/session /tmp/cc-debug-query-8s9axtnv/query.json /tmp/cc-debug-query-8s9axtnv/workspace.json --missing-artifact \"frontier trace not retained by explicit session capture\"",
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events observe --session /tmp/cc-debug-query-8s9axtnv/session --checkpoint 1 --vm-index 0 --profile /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/profile.json --admitted-profiles /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/observation-profiles.json /tmp/cc-debug-query-8s9axtnv/observation.json",
    "pueue_task" = 74,
    "exit_status" = null,
    "outcome" = "Validated complete receipt; queue reset removed the command-completion log.",
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events validate-observation --session /tmp/cc-debug-query-8s9axtnv/session --checkpoint 1 --vm-index 0 --profile /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/profile.json --admitted-profiles /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/observation-profiles.json /tmp/cc-debug-query-8s9axtnv/observation.json",
    "exit_status" = 0,
  }],
  adversarial_and_stdout_commands = [{
    "argv" = ["/home/brittonr/.cargo-target/debug/query-events", "observe", "--session", "/tmp/cc-debug-query-8s9axtnv/session", "--checkpoint", "1", "--vm-index", "0", "--profile", "/tmp/cc-debug-query-8s9axtnv/profile-escape.json", "--admitted-profiles", "/nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/observation-profiles.json", "/tmp/cc-debug-query-8s9axtnv/escape-must-not-exist.json"],
    "exit" = 1,
    "stderr" = "query-events: Profile(\"observation path must be normalized and relative\")",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/query-events", "observe", "--session", "/tmp/cc-debug-query-8s9axtnv/session", "--checkpoint", "1", "--vm-index", "0", "--profile", "/tmp/cc-debug-query-8s9axtnv/profile-unadmitted.json", "--admitted-profiles", "/nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/observation-profiles.json", "/tmp/cc-debug-query-8s9axtnv/unadmitted-must-not-exist.json"],
    "exit" = 1,
    "stderr" = "query-events: UnadmittedCommand",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/query-events", "timeline", "--format", "markdown", "/tmp/cc-debug-query-8s9axtnv/workspace-accounting.json", "/tmp/cc-debug-query-8s9axtnv/accounting-must-not-exist.md"],
    "exit" = 1,
    "stderr" = "query-events: event query result does not match a fresh evaluation",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/query-events", "query", "--recording", "/tmp/cc-debug-query-8s9axtnv/session/recording.json", "--moment", "/tmp/cc-debug-query-8s9axtnv/future-moment.json", "/tmp/cc-debug-query-8s9axtnv/query.json", "/tmp/cc-debug-query-8s9axtnv/future-must-not-exist.json"],
    "exit" = 1,
    "stderr" = "query-events: moment requires a validated checkpoint at or before its tick",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/query-events", "query", "--events", "/tmp/cc-debug-query-8s9axtnv/events-small.json", "/tmp/cc-debug-query-8s9axtnv/query.json", "/tmp/cc-debug-query-8s9axtnv/small-result.json"],
    "exit" = 0,
    "stderr" = "",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/query-events", "validate", "--events", "/tmp/cc-debug-query-8s9axtnv/events-small.json", "/tmp/cc-debug-query-8s9axtnv/query.json", "/tmp/cc-debug-query-8s9axtnv/small-result.json", "/tmp/cc-debug-query-8s9axtnv/small-receipt.json"],
    "exit" = 0,
    "stderr" = "",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/query-events", "validate", "--events", "/tmp/cc-debug-query-8s9axtnv/events-small.json", "/tmp/cc-debug-query-8s9axtnv/query.json", "/tmp/cc-debug-query-8s9axtnv/small-result.json", "/tmp/cc-debug-query-8s9axtnv/small-receipt.json"],
    "exit" = 1,
    "stderr" = "query-events: event query result does not match a fresh evaluation",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/query-events", "timeline", "--format", "markdown", "/tmp/cc-debug-query-8s9axtnv/workspace-overclaim.json", "/tmp/cc-debug-query-8s9axtnv/overclaim-must-not-exist.md"],
    "exit" = 1,
    "stderr" = "query-events: /tmp/cc-debug-query-8s9axtnv/workspace-overclaim.json: unknown variant `complete_history`, expected `retained_facts_only` at line 1 column 348759",
  }],
  additional_negative = "validate-observation rejected certifies_original_history=true with Overclaim and exit status 1.",
  known_gaps = ["Six broad-matrix assertion-summary consumer failures and one unrelated SMR Octet error remain; no all-green repository or release claim.", "Large exact session artifacts and rendered raw-history outputs remain local/ignored; this retained receipt alone cannot restore the VM.", "No assertion-coverage campaign, bug search, reduction, or replay verdict was requested or inferred.", "Legacy snapshots missing persistent diagnostic_in_flight state fail closed.", "Nix input portability and host/kernel security remain outside this bounded proof."],
  certifies_original_history | FalseOnly = false,
  complete_capture | FalseOnly = false,
  causal_attribution | FalseOnly = false,
  source_correctness | FalseOnly = false,
  replay_promotion | FalseOnly = false,
  release_eligible | FalseOnly = false,
  observation | (import "../../../../contracts/debug-query-workspace/observation-receipt.ncl") = import "./2026-11-06-observation-receipt.json",
  query_receipt = import "./2026-11-06-query-receipt.json",
  markdown_receipt = import "./2026-11-06-timeline-receipt.json",
  dot_receipt = import "./2026-11-06-dot-receipt.json",
  static_commands = [{
    "command" = "/home/brittonr/.cargo-target/debug/query-events timeline --format markdown /tmp/cc-debug-query-8s9axtnv/workspace.json /tmp/cc-debug-query-8s9axtnv/timeline.md --receipt /tmp/cc-debug-query-8s9axtnv/timeline-receipt.json",
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events validate-timeline --format markdown /tmp/cc-debug-query-8s9axtnv/workspace.json /tmp/cc-debug-query-8s9axtnv/timeline.md /tmp/cc-debug-query-8s9axtnv/timeline-receipt.json",
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events timeline --format dot /tmp/cc-debug-query-8s9axtnv/workspace.json /tmp/cc-debug-query-8s9axtnv/timeline.dot --receipt /tmp/cc-debug-query-8s9axtnv/dot-receipt.json",
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events validate-timeline --format dot /tmp/cc-debug-query-8s9axtnv/workspace.json /tmp/cc-debug-query-8s9axtnv/timeline.dot /tmp/cc-debug-query-8s9axtnv/dot-receipt.json",
    "exit_status" = 0,
  }, {
    "command" = "/home/brittonr/.cargo-target/debug/query-events query --session /tmp/cc-debug-query-8s9axtnv/session /tmp/cc-debug-query-8s9axtnv/query.json /tmp/cc-debug-query-8s9axtnv/result-rerun.json --receipt /tmp/cc-debug-query-8s9axtnv/query-receipt-rerun.json",
    "exit_status" = 0,
  }, {
    "command" = "cmp /tmp/cc-debug-query-8s9axtnv/result.json /tmp/cc-debug-query-8s9axtnv/result-rerun.json && cmp /tmp/cc-debug-query-8s9axtnv/query-receipt.json /tmp/cc-debug-query-8s9axtnv/query-receipt-rerun.json",
    "exit_status" = 0,
  }],
  lifecycle = {
    "validate" = "pass",
    "proposal_gate" = "pass",
    "design_gate" = "pass",
    "tasks_gate" = "pass",
    "sync" = "blocked-not-executed",
    "archive" = "not-attempted",
    "blocker" = "change_dependency.unsatisfied_dependency: dependency add-persisted-debugger-replay is not archived",
    "traceability" = "blocked by unsynced new requirement IDs; five dangling markers under accepted-spec-only policy",
    "policy_change" = "none; dependency was preserved",
  },
}
