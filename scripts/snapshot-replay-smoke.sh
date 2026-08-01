#!/usr/bin/env bash
set -euo pipefail

: "${CHAOSCONTROL_EXPLORE:=chaoscontrol-explore}"
: "${CHECK_REPLAY_VERDICT:=check-replay-verdict-artifact}"
: "${KERNEL:?KERNEL must point to vmlinux}"
: "${INITRD:?INITRD must point to the Raft initrd}"
: "${OUT:=}"
: "${RUN_TIMEOUT_SECONDS:=360}"
: "${REPRO_TIMEOUT_SECONDS:=180}"
: "${VM_COUNT:=3}"
: "${RUN_ROUNDS:=3}"
: "${RUN_BRANCHES:=2}"
: "${BRANCH_TICKS:=500}"
: "${RUN_SEED:=42}"
: "${EXPLORATION_MODE:=hybrid}"
: "${BOOTSTRAP_BUDGET:=10000}"
: "${MEMORY_MIB:=128}"
: "${ASSERTION_ALIAS:=3463273124}"
: "${PROBE_FAIL_AFTER:=0}"

readonly TIMEOUT_EXIT_STATUS=124
readonly BUG_FOUND_EXIT_STATUS=1
readonly MINIMUM_REPLAY_PARENT_DEPTH=1
readonly MAXIMUM_EXPORTED_BUGS=1

workdir="${WORKDIR:-$(mktemp -d)}"
run_dir="$workdir/run"
run_log="$workdir/run.log"
export_log="$workdir/export.log"
reproduce_log="$workdir/reproduce.log"
verdict_path="$run_dir/replay-verdict.json"

rm -rf "$run_dir"
mkdir -p "$run_dir"

set +e
timeout "$RUN_TIMEOUT_SECONDS" "$CHAOSCONTROL_EXPLORE" run \
  --kernel "$KERNEL" \
  --initrd "$INITRD" \
  --output "$run_dir" \
  --vms "$VM_COUNT" \
  --rounds "$RUN_ROUNDS" \
  --branches "$RUN_BRANCHES" \
  --ticks "$BRANCH_TICKS" \
  --seed "$RUN_SEED" \
  --mode "$EXPLORATION_MODE" \
  --bootstrap-budget "$BOOTSTRAP_BUDGET" \
  --memory-mb "$MEMORY_MIB" \
  --extra-cmdline "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=$PROBE_FAIL_AFTER" \
  >"$run_log" 2>&1
run_rc=$?
set -e

if [[ "$run_rc" != 0 && "$run_rc" != "$BUG_FOUND_EXIT_STATUS" && "$run_rc" != "$TIMEOUT_EXIT_STATUS" ]]; then
  echo "snapshot replay smoke: run failed with rc=$run_rc" >&2
  tail -100 "$run_log" >&2 || true
  exit "$run_rc"
fi
if [[ ! -f "$run_dir/checkpoint.json" ]]; then
  echo "snapshot replay smoke: missing checkpoint.json after run rc=$run_rc" >&2
  tail -100 "$run_log" >&2 || true
  exit 1
fi

"$CHAOSCONTROL_EXPLORE" export-bugs \
  --checkpoint "$run_dir/checkpoint.json" \
  --output "$run_dir" \
  --no-overwrite \
  --assertion-id "$ASSERTION_ALIAS" \
  --min-replay-parent-depth "$MINIMUM_REPLAY_PARENT_DEPTH" \
  --max-bugs "$MAXIMUM_EXPORTED_BUGS" \
  >"$export_log" 2>&1

shopt -s nullglob
bug_files=("$run_dir"/bug_*.json)
shopt -u nullglob
if [[ "${#bug_files[@]}" != "$MAXIMUM_EXPORTED_BUGS" ]]; then
  echo "snapshot replay smoke: expected one filtered bug, found ${#bug_files[@]}" >&2
  tail -100 "$run_log" >&2 || true
  tail -100 "$run_dir/report.txt" >&2 || true
  tail -100 "$export_log" >&2 || true
  exit 1
fi
bug_path="${bug_files[0]}"

set +e
timeout "$REPRO_TIMEOUT_SECONDS" "$CHAOSCONTROL_EXPLORE" reproduce \
  --kernel "$KERNEL" \
  --initrd "$INITRD" \
  --bug "$bug_path" \
  --vms "$VM_COUNT" \
  --bootstrap-budget "$BOOTSTRAP_BUDGET" \
  --memory-mb "$MEMORY_MIB" \
  --extra-cmdline "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=$PROBE_FAIL_AFTER" \
  --verdict-output "$verdict_path" \
  >"$reproduce_log" 2>&1
reproduce_rc=$?
set -e
if [[ "$reproduce_rc" != 0 ]]; then
  echo "snapshot replay smoke: reproduce failed with rc=$reproduce_rc" >&2
  tail -100 "$reproduce_log" >&2 || true
  exit "$reproduce_rc"
fi

"$CHECK_REPLAY_VERDICT" --verdict "$verdict_path" --bug "$bug_path"
summary="snapshot replay smoke ok: $(basename "$bug_path")"
echo "$summary"

if [[ -n "$OUT" ]]; then
  mkdir -p "$OUT"
  {
    echo "$summary"
    echo "run_rc=$run_rc"
    echo "bug=$bug_path"
    echo "verdict=$verdict_path"
  } >"$OUT/snapshot-replay-smoke.txt"
fi
