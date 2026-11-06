let P = import "../../../../contracts/evidence/profile-primitives.ncl" in
({
  "schema" = "chaoscontrol.event-entry-exact-admission-proof.v1",
  "source" = {
    "base_revision" = "b6fa2efa521e867624f39be5e9da952346809163",
    "uncommitted_worktree" = true,
    "identity_scope" = "Selected source and executable identities below; not a complete repository closure or clean-source release cohort.",
  },
  "owner" = "ChaosControl VMM, sim-core and evidence maintainers",
  "immediate_outcome" = "Portable production SMP fails closed before machine, guest-input or output mutation; single-vCPU restored guest observation remains executable.",
  "durable_capability" = "Event-entry exact capability admission plus actual-run-only SMP release authority; coarse scheduling remains a separately reviewed spec-only plan.",
  "checks" = [{
    "command" = "nix develop -c cargo test -p chaoscontrol-sim-core -p chaoscontrol-vmm -p chaoscontrol-evidence --all-targets",
    "queue_job" = 40,
    "exit" = 0,
    "passed" = 1392,
    "failed" = 0,
    "ignored" = 13,
  }, {
    "command" = "nix develop -c cargo build -p chaoscontrol-explore -p chaoscontrol-replay -p chaoscontrol-vmm --bins",
    "queue_job" = 40,
    "exit" = 0,
  }, {
    "command" = "nix develop -c cargo build --release -p chaoscontrol-explore --bin query-events",
    "queue_job" = 41,
    "exit" = 0,
  }, {
    "command" = "nix develop -c cargo run -p chaoscontrol-evidence --bin check-kvm-release-matrix -- --root .",
    "queue_job" = 41,
    "exit" = 0,
    "observed" = "profile=required-kvm-release-v1 rows=7 positive=1 negative=13; fixture validation, not actual full-matrix execution",
  }, {
    "command" = "nix develop -c cargo clippy -p chaoscontrol-sim-core -p chaoscontrol-vmm -p chaoscontrol-evidence --all-targets -- -D warnings",
    "exit" = 0,
  }, {
    "command" = "nix develop -c cargo fmt --all --check",
    "exit" = 0,
  }],
  "recovery_history" = ["Job37 failed compiling a private step call in the new integration fixture; corrected to public run_bounded and observed AX increment.", "Job40 tests/builds passed; formatting check failed, then formatter applied.", "Job43 Clippy found redundant guard; replaced by equivalent literal pattern. Job45 caught formatting of that pattern. Final formatter, Clippy and formatting check all passed."],
  "strict_cli_negatives" = [{
    "argv" = ["/home/brittonr/.cargo-target/debug/smp_test", "/missing-strict-kernel", "/missing-strict-initrd"],
    "exit" = 1,
    "stdout" = "",
    "stderr" = "error: smp_test: cannot create 2-vCPU VM: Deterministic schedule rejected input: MissingCapability { capability: \"exact guest-instruction boundaries across IRQ/NMI/synchronous exception entry\" }\n",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/smp4_test", "/missing-strict-kernel", "/missing-strict-initrd"],
    "exit" = 1,
    "stdout" = "\n=== Testing 4 vCPUs ===\n",
    "stderr" = "error: smp4_test: cannot create 4-vCPU VM: Deterministic schedule rejected input: MissingCapability { capability: \"exact guest-instruction boundaries across IRQ/NMI/synchronous exception entry\" }\n",
  }, {
    "argv" = ["/home/brittonr/.cargo-target/debug/determinism_stress", "/missing-strict-kernel", "/missing-strict-initrd", "2", "--case", "single-vm-2vcpu", "--receipt", "/tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/must-not-exist.json", "--dlog-dir", "/tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/must-not-exist-dlogs"],
    "exit" = 1,
    "stdout" = "",
    "stderr" = "error: determinism_stress: selected SMP case cannot be admitted: Deterministic schedule rejected input: MissingCapability { capability: \"exact guest-instruction boundaries across IRQ/NMI/synchronous exception entry\" }\n",
    "no_receipt_or_dlog_mutation" = true,
  }],
  "live_single_vcpu" = {
    "queue_job" = 42,
    "commands" = ["timeout 600 /home/brittonr/.cargo-target/release/query-events query --session /tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-session /tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-proof/query.json /tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/query-result.json --receipt /tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/query-receipt.json", "timeout 600 /home/brittonr/.cargo-target/release/query-events validate --session /tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-session /tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-proof/query.json /tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/query-result.json /tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/query-receipt.json", "timeout 600 /home/brittonr/.cargo-target/release/query-events observe --session /tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-session --checkpoint 2 --vm-index 0 --profile /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/profile.json --admitted-profiles /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/observation-profiles.json /tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/observation.json", "timeout 600 /home/brittonr/.cargo-target/release/query-events validate-observation --session /tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-session --checkpoint 2 --vm-index 0 --profile /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/profile.json --admitted-profiles /nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/observation-profiles.json /tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/observation.json"],
    "exit" = 0,
    "num_vms" = 1,
    "num_vcpus" = 1,
    "seed" = 42,
    "memory_mib" = 128,
    "quantum" = 100,
    "session" = "/tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-session",
    "session_schema" = 2,
    "query_scanned" = 374,
    "query_matched" = 374,
    "query_truncated" = false,
    "query_receipt_validated" = true,
    "observation_receipt_validated" = true,
    "checkpoint" = 2,
    "checkpoint_tick" = 1210,
    "observation_end_tick" = 1211,
    "executed_rounds" = 1,
    "stdout_bytes" = 10,
    "stderr_bytes" = 0,
    "stdout_limit" = 1024,
    "stderr_limit" = 1024,
  },
  "irq_counterexample" = {
    "diagnostic" = "diagnostic_irq_entry_does_not_meet_exact_instruction_contract",
    "disposition" = "Known failing instruction-boundary expectation retained as explicit ignored diagnostic. Existing observed IRQ counterexample was not rerun to confirm it, not weakened, and not promoted into positive evidence.",
  },
  "planning" = {
    "path" = ".cairn/changes/plan-kvm-debug-boundary-scheduling",
    "implementation" = false,
    "gates" = "Proposal/design passed only structural advisory checks with ambient Cairn x37 cohort; no selected-cohort closure claim.",
  },
  "artifacts" = [{
    "path" = "crates/chaoscontrol-sim-core/src/scheduler/core.rs",
    "digest" = "blake3:4a71e4c5b52f1e265dbeb2a371b7420f0d5fbf111cb592d42d990f4dcf9b92bb",
  }, {
    "path" = "crates/chaoscontrol-vmm/src/vm.rs",
    "digest" = "blake3:a10f4c1c0517edd7484554efb6f5bc16faf1bcd89e3dc7551f715c2f559abb3d",
  }, {
    "path" = "crates/chaoscontrol-vmm/src/controller.rs",
    "digest" = "blake3:a3d95ac80e471ec65ca5a7acd0d7cd54f4ec09143caf598e9dd9d9d47d46b861",
  }, {
    "path" = "crates/chaoscontrol-vmm/src/vm/diagnostic_fixtures.rs",
    "digest" = "blake3:f156999eb9b02a00af953d3e78139fcd59092aeb1a98cc79cbdf2655a20f5e92",
  }, {
    "path" = "crates/chaoscontrol-vmm/tests/deterministic_smp_kvm.rs",
    "digest" = "blake3:f69b84163f13a5dc4493ede058828ddbdd5f370d49f86dc363aee76a77780c42",
  }, {
    "path" = "contracts/kvm-release/matrix.ncl",
    "digest" = "blake3:5876fb4ea8967a61c8c290364b8a4587d82661c4f9de981f488ca5eaf2406f0e",
  }, {
    "path" = "contracts/kvm-release/matrix.json",
    "digest" = "blake3:5b2d2648a3adc2ed8843dd08da4084d99dfc7a0f6b9f42f30c67d61c94c034f4",
  }, {
    "path" = "crates/chaoscontrol-evidence/src/kvm_release/mod.rs",
    "digest" = "blake3:7a78ff0b830255d4726f7a805932fe7392bf16aaab15d8f84e9260fd696b53f2",
  }, {
    "path" = "/home/brittonr/.cargo-target/debug/smp_test",
    "digest" = "blake3:0da377d8f861ccbc54dc264db898f1190d0586fb3222e45a84f7f1a095a7a1cf",
  }, {
    "path" = "/home/brittonr/.cargo-target/debug/smp4_test",
    "digest" = "blake3:da2399714397635007db4fdd007d3e3a94c6a7644bd05dad7e9c45df73686937",
  }, {
    "path" = "/home/brittonr/.cargo-target/debug/determinism_stress",
    "digest" = "blake3:41a6459c81e43bdc2f15f8ef0caba5d2a43427c4784c127770bf0a22379804eb",
  }, {
    "path" = "/home/brittonr/.cargo-target/release/query-events",
    "digest" = "blake3:f14bccc2bb5ad59a6b8b945f18123839f414f46fd33ebc53f0e65ec9fd7eff82",
  }, {
    "path" = "/tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-session/session.json",
    "digest" = "blake3:9490c4872c0812922df9415a8f9411f923e5a576401b4edb86ca6499bb54ac97",
  }, {
    "path" = "/tmp/cc-persisted-debugger-0tw7wr4k/workspace-schema2-proof/query.json",
    "digest" = "blake3:eff042498e056573c7c19313622c6dbcecdad7549d0d88a6996052a269ab4631",
  }, {
    "path" = "/tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/query-result.json",
    "digest" = "blake3:3d1fe598d18fe74d8f0f79ce7572142bbd74117a80d494f73ebc6a7c96fd92d9",
  }, {
    "path" = "/tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/query-receipt.json",
    "digest" = "blake3:9ef20902536735b856d9f2568ab9f806830a0e63b5008843fe7d350a794fbf3e",
  }, {
    "path" = "/tmp/cc-persisted-debugger-0tw7wr4k/strict-admission-smoke/observation.json",
    "digest" = "blake3:e9f8cd8b49f522d4245d4d78e4ed41eea08471fe792b5e7ad4f5529ad0103a8d",
  }, {
    "path" = "/nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/profile.json",
    "digest" = "blake3:085fcf9b24315208bf91dc8b37f6bb155a88ea9151c0085fdcf7cc1cdee7d50a",
  }, {
    "path" = "/nix/store/c42zhkj24irbrbbqblq448qa84b4b9jd-chaoscontrol-debug-observation-bundle/observation-profiles.json",
    "digest" = "blake3:f54391c538db7af2e5ca79a12b66886a0fd861e88a458ae8289f8ac7eba97e7c",
  }],
  "known_gaps" = ["Strict SMP execution acceptance remains blocked on a backend guarantee through IRQ/NMI/synchronous exception entry.", "Selected Cairn a1e8 packaged CLI lcvbwhsl29irl1yhw6zg57fn69anjbaa was absent; nix-store realisation had no substitute. No verified selected-cohort lifecycle closure.", "Full KVM release matrix and full workspace/Nix/MCP readiness were not executed for this repair.", "Thirteen ignored tests are not passing execution evidence; raw queue logs and temporary retained sessions remain local."],
  "non_claims" = ["No coarse-mode implementation or replay qualification", "No instruction-exact SMP execution support on this backend", "No production or release approval", "No archive or accepted strict-spec weakening", "No kernel installation"],
} | { schema | P.exact_string "chaoscontrol.event-entry-exact-admission-proof.v1", artifacts | Array { path | String, digest | P.blake3 }, .. })
