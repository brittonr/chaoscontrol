# Bounded local execution observations; not lifecycle completion authority.
let P = import "../../../../contracts/evidence/profile-primitives.ncl" in
let B3 = std.contract.from_predicate (fun value => std.is_string value && std.string.is_match "^b3:[0-9a-f]{64}$" value) in
let Artifact = { path | P.non_empty_string, digest | B3, bytes | P.u64 } in
let Receipt = { schema | P.exact_string "chaoscontrol.persisted-debugger-proof.v1", session_id | B3, configuration_identity | B3, artifacts | Array Artifact, source_artifacts | Array Artifact, .. } in
({
"schema" = "chaoscontrol.persisted-debugger-proof.v1",
"verdict" = "bounded-single-vcpu-pass-smp-blocked",
"git_revision" = "b6fa2efa521e867624f39be5e9da952346809163",
"source_state" = "Uncommitted implementation over this base revision. Source and executable digests are observations, not a source-to-binary attestation. Producer used the earlier executable; final replay/inspection used the rebuilt boundary fix.",
"maintenance_owner" = "ChaosControl replay, explorer, fault engine, and VMM maintainers",
"consumer" = "Local operator inspecting a reproduced first-party Rust workload finding",
"durable_capability" = "One bounded persisted-session intake shared by explorer reproduction, exact replay, inspector, and moment navigation; capability-rooted artifact storage and canonical KVM observation settlement.",
"local_artifacts" = "/tmp/cc-persisted-debugger-0tw7wr4k",
"limits_profile" = "exact-local-v1",
"parameters" = {
"seed" = 42,
"vms" = 1,
"vcpus" = 1,
"memory_mib" = 128,
"quantum" = 64,
"requested_ticks" = 20,
"checkpoint_interval" = 1,
"bootstrap_budget" = 10000
},
"session_id" = "b3:b89564f6ecfa3ad892ad8d32a3410c202d0560b307b52ca7f04d2cf7b5c79f0d",
"configuration_identity" = "b3:88d3d7c148723658e374d936b529f1bc9650f18e0ee02e8df1c048f1b2e95681",
"retained_ticks" = [
1835,
1836
],
"checkpoint_ticks" = [
1835,
1836
],
"original_reproduction" = {
"replay_class" = "snapshot_backed_reproduced",
"assertion_id" = 3143219316,
"passed" = 5,
"failed" = 1,
"unexercised" = 0,
"note" = "This class belongs to the recorded reproduce verdict, not to debugger navigation."
},
"observations" = {
"sequence" = [
1836,
1835,
1836
],
"registers_all_equal_to_continuation_control" = true,
"memory_all_equal_to_continuation_control" = true,
"endpoint_repeat_identical" = true,
"rip_start" = "0x00007ffff7f5063c",
"rip_end" = "0x00007ffff7f4fd0f",
"memory_address" = "0x6ec9000",
"memory_bytes" = 16,
"memory_hex" = "00000000000000000000000000000000",
"verified_original" = true,
"counterfactual" = false,
"script" = "goto 1836\nstate\nregs 0 0\nmem 0 0x6ec9000 16\ngoto 1835\nstate\nregs 0 0\nmem 0 0x6ec9000 16\ngoto 1836\nstate\nregs 0 0\nmem 0 0x6ec9000 16\nquit\n",
"control" = "Separate one-checkpoint recording replays the same completed round; compare its full register dump and selected memory at each declared target with the two-checkpoint direct restore."
},
"bootstrap_capture" = {
"job" = 76,
"session_id" = "b3:552d3f787fb4929de23bd4613e4bcee82c4f65216cab158f5ec9199b3ffa2c7f",
"retained_start_tick" = 0,
"retained_end_tick" = 1180,
"checkpoint_ticks" = [
0,
1175,
1180
],
"quantum" = 100,
"history_ticks" = 10,
"checkpoint_interval" = 5
},
"checks" = {
"portable_targets" = 168,
"passed" = 2703,
"failed" = 0,
"ignored" = 14,
"clippy_warnings_denied" = "pass",
"rust_format" = "pass",
"nickel_workspace_and_session_profiles" = "pass",
"repaired_nix_octet" = "warning-only",
"octet_warnings" = 3879,
"octet_errors" = 0,
"real_kvm_pending_out_boundary" = "pass"
},
"negative_matrix" = {
"tampered_trace_replay" = 1,
"tampered_trace_inspect" = 1,
"missing_snapshot_replay" = 1,
"missing_snapshot_inspect" = 1,
"tampered_trace_diagnostic" = "session artifact identity drift: recording.json",
"missing_snapshot_diagnostic" = "session artifact is missing",
"unverified_position_exposed" = false
},
"commands" = [
{
"job" = 67,
"command" = "timeout 240 /home/brittonr/.cargo-target/debug/chaoscontrol-explore reproduce --kernel /nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/pc595gjw00myl6vfzlz07cshsnxa1287-chaoscontrol-initrd-rust-workload --bug /tmp/cc-persisted-debugger-0tw7wr4k/campaign-single-vcpu/bug_0.json --seed 42 --vms 1 --vcpus 1 --memory-mb 128 --ticks 80 --quantum 64 --bootstrap-budget 10000 --extra-cmdline 'rust_workload_bug=snapshot_replay_probe rust_workload_snapshot_probe_fail_after=1' --debug-recording-output /tmp/cc-persisted-debugger-0tw7wr4k/reproduced-session --debug-recording-profile exact-local-v1 --debug-recording-checkpoint-interval 10 --verdict-output /tmp/cc-persisted-debugger-0tw7wr4k/reproduce-verdict.json",
"exit" = 0
},
{
"job" = 69,
"command" = "timeout 240 /home/brittonr/.cargo-target/debug/chaoscontrol-explore reproduce --kernel /nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/pc595gjw00myl6vfzlz07cshsnxa1287-chaoscontrol-initrd-rust-workload --bug /tmp/cc-persisted-debugger-0tw7wr4k/campaign-single-vcpu/bug_0.json --seed 42 --vms 1 --vcpus 1 --memory-mb 128 --ticks 20 --quantum 64 --bootstrap-budget 10000 --extra-cmdline 'rust_workload_bug=snapshot_replay_probe rust_workload_snapshot_probe_fail_after=1' --debug-recording-output /tmp/cc-persisted-debugger-0tw7wr4k/reproduced-session-every-tick --debug-recording-profile exact-local-v1 --debug-recording-checkpoint-interval 1 --verdict-output /tmp/cc-persisted-debugger-0tw7wr4k/reproduce-every-tick-verdict.json",
"exit" = 0
},
{
"job" = 70,
"command" = "timeout 240 /home/brittonr/.cargo-target/debug/chaoscontrol-replay replay --recording /tmp/cc-persisted-debugger-0tw7wr4k/reproduced-session",
"exit" = 0
},
{
"job" = 76,
"command" = "timeout 600 /home/brittonr/.cargo-target/debug/query-events capture-session --kernel /nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/pc595gjw00myl6vfzlz07cshsnxa1287-chaoscontrol-initrd-rust-workload --seed 42 --vms 1 --memory-mib 128 --quantum 100 --bootstrap-ticks 10000 --history-ticks 10 --checkpoint-interval 5 /tmp/cc-persisted-debugger-0tw7wr4k/captured-session-complete",
"exit" = 0
},
{
"job" = 77,
"command" = "nix develop -c cargo test -p chaoscontrol-replay -p chaoscontrol-explore -p chaoscontrol-evidence -p chaoscontrol-smr -p chaoscontrol-vmm -p chaoscontrol-fault -p chaoscontrol-protocol --all-targets --no-fail-fast && nix develop -c cargo clippy -p chaoscontrol-replay -p chaoscontrol-explore -p chaoscontrol-evidence -p chaoscontrol-smr -p chaoscontrol-vmm -p chaoscontrol-fault -p chaoscontrol-protocol --all-targets -- -D warnings && nix develop -c cargo build -p chaoscontrol-replay -p chaoscontrol-explore --bins",
"exit" = 0
},
{
"job" = 78,
"command" = "nix build .#checks.x86_64-linux.debug-query-workspace-contracts .#checks.x86_64-linux.tigerstyle-chaoscontrol-focused -L --no-link --builders ''",
"exit" = 0
},
{
"job" = 80,
"command" = "timeout 240 /home/brittonr/.cargo-target/debug/chaoscontrol-replay inspect --recording /tmp/cc-persisted-debugger-0tw7wr4k/reproduced-session-every-tick --script /tmp/cc-persisted-debugger-0tw7wr4k/inspect-proof.txt",
"exit" = 0
},
{
"job" = 81,
"command" = "timeout 240 /home/brittonr/.cargo-target/debug/chaoscontrol-replay inspect --recording /tmp/cc-persisted-debugger-0tw7wr4k/reproduced-session --script /tmp/cc-persisted-debugger-0tw7wr4k/inspect-proof.txt",
"exit" = 0
},
{
"job" = 82,
"command" = "timeout 60 /home/brittonr/.cargo-target/debug/chaoscontrol-replay replay --recording /tmp/cc-persisted-debugger-0tw7wr4k/tampered-trace",
"exit" = 1
},
{
"job" = 83,
"command" = "timeout 60 /home/brittonr/.cargo-target/debug/chaoscontrol-replay inspect --recording /tmp/cc-persisted-debugger-0tw7wr4k/tampered-trace --script /tmp/cc-persisted-debugger-0tw7wr4k/inspect-proof.txt",
"exit" = 1
},
{
"job" = 84,
"command" = "timeout 60 /home/brittonr/.cargo-target/debug/chaoscontrol-replay replay --recording /tmp/cc-persisted-debugger-0tw7wr4k/missing-snapshot",
"exit" = 1
},
{
"job" = 85,
"command" = "timeout 60 /home/brittonr/.cargo-target/debug/chaoscontrol-replay inspect --recording /tmp/cc-persisted-debugger-0tw7wr4k/missing-snapshot --script /tmp/cc-persisted-debugger-0tw7wr4k/inspect-proof.txt",
"exit" = 1
}
],
"artifacts" = [
{
"path" = "/home/brittonr/.cargo-target/debug/chaoscontrol-replay",
"digest" = "b3:17fe16cb8dc6ea33bb39a567db49f3e41927876c6cc5d63cc6a9de3239efe497",
"bytes" = 150361040
},
{
"path" = "/home/brittonr/.cargo-target/debug/chaoscontrol-explore",
"digest" = "b3:a8a3748f12ae0debd638e905485e0d4d025409bd40906c458677ce5121a69b00",
"bytes" = 247422032
},
{
"path" = "/nix/store/5aya7zvynanjw5mh3xvz40sga3zfsmn8-chaoscontrol-vmlinux/vmlinux",
"digest" = "b3:f2abae8a52c83c975ed15de504535445e2ac159cc0198f8028867b0c18cdf67c",
"bytes" = 441789520
},
{
"path" = "/nix/store/pc595gjw00myl6vfzlz07cshsnxa1287-chaoscontrol-initrd-rust-workload",
"digest" = "b3:cd223f1619198c3d9b36d4e125d76b6a85b29da5b96d02f6e681e6fd3030b30c",
"bytes" = 400691
},
{
"path" = "/tmp/cc-persisted-debugger-0tw7wr4k/reproduced-session-every-tick/session.json",
"digest" = "b3:4be5d007b029a6c17ef1eb7c93327f5aaba2fcd7339e18c03e98dbd6cc753186",
"bytes" = 8022
},
{
"path" = "/tmp/cc-persisted-debugger-0tw7wr4k/reproduce-every-tick-verdict.json",
"digest" = "b3:df45600a86859caa7985085577b7ed482476e3768d2260e72228f38a683e4636",
"bytes" = 6076
}
],
"source_artifacts" = [
{
"path" = "crates/chaoscontrol-replay/src/session.rs",
"digest" = "b3:5b198348be33f338314587bbb5c1413b7e53ad4549b647344a14554336075efd",
"bytes" = 76151
},
{
"path" = "crates/chaoscontrol-replay/src/recording.rs",
"digest" = "b3:209412d1e62c85262e78d79b618e5504630097dfd996efdfa551724044dbe77b",
"bytes" = 69166
},
{
"path" = "crates/chaoscontrol-replay/src/replay.rs",
"digest" = "b3:5c8c2a342ac172935b1d24aa19ff44cf8426b2b42a65e573886fd8a59401a272",
"bytes" = 73316
},
{
"path" = "crates/chaoscontrol-replay/src/debugger.rs",
"digest" = "b3:90f68800e433a35f751020b47353bb07a4157abc69935ddc6e9e96577d54f9cb",
"bytes" = 36644
},
{
"path" = "crates/chaoscontrol-replay/src/snapshot_store.rs",
"digest" = "b3:3409460ea4b1f6a315afb626557dbdda0400b276f002d9ce377427dcb912f8b7",
"bytes" = 53887
},
{
"path" = "crates/chaoscontrol-vmm/src/vm.rs",
"digest" = "b3:8897b67bf239aea8cd05de54fd973f2f4116cc79cc351f3c47f8e6feb627dab9",
"bytes" = 249603
},
{
"path" = "crates/chaoscontrol-vmm/src/controller.rs",
"digest" = "b3:8e066cc56b269b92152eaeefe6ca8be781b6138e77dea446d96400c77e5aaae6",
"bytes" = 280523
},
{
"path" = "crates/chaoscontrol-explore/src/bin/chaoscontrol-explore.rs",
"digest" = "b3:6b1c0753d95fe5393a6e041b53d7693d372939f28dda2c612d04a206b851198f",
"bytes" = 202539
},
{
"path" = "crates/chaoscontrol-explore/src/bin/query-events/workspace.rs",
"digest" = "b3:0650bb8284a2607a76f3efe8b1b583d598030479c944286ca2b1b43f9c3fbe0b",
"bytes" = 16050
},
{
"path" = "contracts/persisted-debugger/generated/exact-local-v1.json",
"digest" = "b3:ccdb7c23b49ff165e8ed390a1b53ef02a90bea6ff2c2bed3f5ed62bb0bb7da46",
"bytes" = 419
}
],
"lifecycle" = {
  validate = "pass",
  proposal_gate = "pass",
  design_gate = "pass",
  tasks_gate = "pass-structural-only",
  dependent_sync_dry_run = "blocked: add-persisted-debugger-replay is not archived",
  sync_executed = false,
  archive_executed = false,
},
"known_gaps" = [
"Required non-default SMP Linux finding proof remains open: job39 panicked in Linux MP-BIOS IOAPIC timer setup; explicit no_timer_check job40 exceeded its 240-second bound while booting. No proven replacement profile was found in repository receipts.",
"The before-source-change baseline task remains unchecked: its complete prerequisite evidence is not established by this receipt; post-change passing checks are not substituted for it.",
"Persisted-debugger sync/archive and dependent local workspace closure are not performed.",
"Raw VM/session/log artifacts are local temporary retention, not a published or remotely recoverable archive.",
"Direct cargo octet hit the old Cargo package-id parser panic; repository repaired-Cargo Nix Octet rail passed with --builders empty after the configured remote build hook failed."
],
"non_claims" = [
"No non-default SMP Linux execution proof.",
"No universal determinism, whole-system correctness, cross-host equivalence, causal proof, public installation, production readiness, or release eligibility.",
"No debugger seek promotes replay evidence or original pre-parent history.",
"No power-loss durability or hostile same-UID staging mutation claim.",
"No lifecycle completion or archive approval follows from passing structural gates."
]
}) | Receipt
