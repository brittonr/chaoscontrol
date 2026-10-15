let Positive = std.contract.from_predicate (fun n => std.is_number n && n >= 1 && n == std.number.floor n) in
let Attempt = {
  number | Positive,
  correction | String,
  before | { command | String, pueue_tasks | Array Number, outcome | String },
  after | { command | String, pueue_task | Number, outcome | String },
} in
{
  owner = "chaoscontrol-vmm",
  source_revision = "413713245198b19a80a62f0bcd4498480fd0d984",
  maximum_implementation_attempts = 20,
  used_implementation_attempts = 7,
  counting_rule = "One cohesive corrective implementation followed by verification; artifact-name correction, source reads, builds, and gate-only invocations are not additional implementation attempts.",
  attempts | Array Attempt = [
    {
      number = 1,
      correction = "Retain inherited overlay pages and accumulated guest writes; revert sibling writes even without an intervening capture; validate bounded pure bitmap plans.",
      before = {
        command = "cargo test --offline --locked -p chaoscontrol-vmm --lib vm::incremental_lineage -- --nocapture",
        pueue_tasks = [9],
        outcome = "exit 101; both KVM regressions failed: inherited memory missing at depth 2 and uncaptured sibling guest writes survived restore",
      },
      after = {
        command = "nix develop -c cargo test --offline --locked -p chaoscontrol-vmm --lib",
        pueue_task = 10,
        outcome = "exit 0; 504 passed, 9 ignored; inherited-page, sibling-restore, base-size and pure-plan regressions passed",
      },
    },
    {
      number = 2,
      correction = "Full overlay restore installs its base and retains overlay-page tracking; full-image handoffs reconcile memory against the next incremental base.",
      before = {
        command = "nix develop -c cargo test --offline --locked -p chaoscontrol-vmm --lib vm::incremental_lineage -- --nocapture; then the unchanged-base-sentinel case alone",
        pueue_tasks = [13, 14],
        outcome = "exit 101 twice; full-restore/incremental-recapture lost parent pages, and fresh full overlay restore omitted unchanged base memory",
      },
      after = {
        command = "nix develop -c cargo test --offline --locked -p chaoscontrol-vmm --lib",
        pueue_task = 15,
        outcome = "exit 0; 507 passed, 9 ignored; fresh base installation, overlay and full-image handoffs passed",
      },
    },
    {
      number = 3,
      correction = "Public dirty-log observation retains consumed guest pages; mutable observation API with existing mutable callers; raw drain remains private to restore and test bootstrap. Replace bitmap-length-only test with guest-memory regression.",
      before = {
        command = "nix develop -c cargo test --offline --locked -p chaoscontrol-vmm --lib vm::incremental_lineage::public_dirty_log_observation_preserves_incremental_memory -- --exact --nocapture",
        pueue_tasks = [17],
        outcome = "exit 101; incremental child differed from full reference after a public dirty-log read",
      },
      after = {
        command = "nix develop -c cargo test --offline --locked -p chaoscontrol-vmm --lib incremental && nix develop -c cargo build --offline --locked -p chaoscontrol-explore --example raft_lineage_proof --bin chaoscontrol-explore",
        pueue_task = 18,
        outcome = "exit 0; 10 focused tests passed, none ignored; real Raft proof executable and explorer built",
      },
    },
    {
      number = 4,
      correction = "Register ten existing active Cairn change intents without promoting capability states; regenerate the Nickel projection and product-scope documents.",
      before = {
        command = "nix flake check --no-update-lock-file --builders '' --option secret-key-files '' -L",
        pueue_tasks = [20],
        outcome = "exit 1; evidence-contracts rejected add-counterfactual-replay-witnesses because active-change scope metadata was missing",
      },
      after = {
        command = "nickel export --format json contracts/product-scope/registry.ncl; nix develop -c cargo run --offline --locked -p chaoscontrol-evidence --bin check-product-scope -- --root . --write && nix develop -c cargo run --offline --locked -p chaoscontrol-evidence --bin check-product-scope -- --root .",
        pueue_task = 24,
        outcome = "exit 0; contract-validated projection contains 15 unchanged capabilities and 34 change intents; generated documents and active-change coverage pass",
      },
    },
    {
      number = 5,
      correction = "Drain bootstrap dirty pages against the captured base; retain full-memory comparisons at every depth but persist only the depth-four replay parent, avoiding redundant snapshot encoding.",
      before = {
        command = "timeout 600s /home/brittonr/.cargo-target/debug/examples/raft_lineage_proof --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/d37s9di5xnzc3jwv38lp4x9gc56ipyip-chaoscontrol-initrd-raft --output target/production-readiness-20261015/raft-lineage-1",
        pueue_tasks = [26],
        outcome = "exit 124; the original proof exceeded its 600-second bound after writing five snapshots",
      },
      after = {
        command = "nix develop -c cargo test --offline --locked --workspace --all-targets && nix develop -c cargo clippy --offline --locked --workspace --all-targets -- -D warnings && nix develop -c cargo fmt --all -- --check && nix develop -c cargo build --offline --locked -p chaoscontrol-explore --example raft_lineage_proof",
        pueue_task = 28,
        outcome = "exit 0; workspace all-target tests, Clippy, formatting, and proof build passed. Runtime task 29 completed four full-memory comparisons and retained depth-four memory, then exited 1 because its clean-run initialization reset setup admission and selected no fault.",
      },
    },
    {
      number = 6,
      correction = "Use the established restored-branch begin_counterfactual_fault_run API instead of cold-run initialization, preserving the guests' completed setup admission while assigning the new fault schedule.",
      before = {
        command = "timeout 600s /home/brittonr/.cargo-target/debug/examples/raft_lineage_proof --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/d37s9di5xnzc3jwv38lp4x9gc56ipyip-chaoscontrol-initrd-raft --output target/production-readiness-20261015/raft-lineage-2",
        pueue_tasks = [29],
        outcome = "exit 1; four depths matched full memory, but the emitted fault ledger had zero selected/applied/observed attempts because start_fault_run_at resets setup_complete",
      },
      after = {
        command = "nix develop -c cargo build --offline --locked -p chaoscontrol-explore --example raft_lineage_proof && timeout 600s /home/brittonr/.cargo-target/debug/examples/raft_lineage_proof --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/d37s9di5xnzc3jwv38lp4x9gc56ipyip-chaoscontrol-initrd-raft --output target/production-readiness-20261015/raft-lineage-3",
        pueue_task = 31,
        outcome = "exit 0; runtime proof passed: four full-memory comparisons, persisted depth-four parent, exact full-restore/incremental recapture, two matching 500-tick execution traces and every admitted assertion outcome, safety invariants hold, and one attributed observed CPU stall per continuation",
      },
    },
    {
      number = 7,
      correction = "Raise the evidence campaign round bound from five to the existing non-short threshold of ten, without changing runtime stop semantics or the admitted 600-second operational timeout. This configuration correction is counted conservatively against the attempt cap.",
      before = {
        command = "timeout 600s /home/brittonr/.cargo-target/debug/chaoscontrol-explore run --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/vxl9fvbh9mii71ngf1q8sk0gpmcfnh62-chaoscontrol-initrd-raft --seed 42 --vms 3 --vcpus 1 --memory-mb 128 --rounds 5 --branches 2 --ticks 500 --quantum 100 --max-frontier 50 --bootstrap-budget 10000 --mode hybrid --workers 1 --extra-cmdline raft_bug=none --output target/production-readiness-20261015/raft-control-campaign --emit-metrics",
        pueue_tasks = [27, 30],
        outcome = "Initial control/planted labels cc-production-raft-control-campaign and cc-production-raft-planted-campaign both exited 1 after one round/two branches because the existing short-run policy stops on sometimes findings; detailed inputs and all retained bug records are in campaign-initial-observations.json",
      },
      after = {
        command = "timeout 600s /home/brittonr/.cargo-target/debug/chaoscontrol-explore run --kernel /nix/store/8ki40sdb3cb8zyg7zsvjlmybgir46z6b-chaoscontrol-vmlinux/vmlinux --initrd /nix/store/vxl9fvbh9mii71ngf1q8sk0gpmcfnh62-chaoscontrol-initrd-raft --seed 42 --vms 3 --vcpus 1 --memory-mb 128 --rounds 10 --branches 2 --ticks 500 --quantum 100 --max-frontier 50 --bootstrap-budget 10000 --mode hybrid --workers 1 --extra-cmdline raft_bug=none --output target/production-readiness-20261015/raft-control-campaign-ten-round --emit-metrics",
        pueue_task = 26,
        outcome = "Labels cc-production-raft-control-ten-round (task 26) and cc-production-raft-planted-ten-round (task 27) both exited 124. Each emitted metrics through round three and retained a valid two-round/three-branch checkpoint with eight depth-one sometimes records and no retained safety finding. These are partial operational-timeout observations, not completed depth-four campaigns or accepted deep replay. Exact commands and checkpoint digests are in campaign-observations.json.",
      },
    },
  ],
  evidence_boundary = "These are implementation/regression observations, not completed workload replay or release-readiness authority. The enclosing receipt records those results separately. Intermediate source trees were not separately content-hashed; the final source identity belongs to the enclosing receipt.",
}
