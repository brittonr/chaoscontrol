#!/usr/bin/env bash
set -euo pipefail

: "${CHAOSCONTROL_EXPLORE:=chaoscontrol-explore}"
: "${KERNEL:?KERNEL must point to vmlinux}"
: "${INITRD:?INITRD must point to the Raft initrd}"
: "${OUT:=}"
: "${TIMEOUT:=240}"
: "${REPRO_TIMEOUT:=180}"

workdir="${WORKDIR:-$(mktemp -d)}"
mkdir -p "$workdir"
run_dir="$workdir/run"
run_log="$workdir/run.log"
export_log="$workdir/export.log"
reproduce_log="$workdir/reproduce.log"
selected_bug="$workdir/selected-bug-path.txt"

rm -rf "$run_dir"
mkdir -p "$run_dir"

set +e
timeout "$TIMEOUT" "$CHAOSCONTROL_EXPLORE" run \
  --kernel "$KERNEL" \
  --initrd "$INITRD" \
  --output "$run_dir" \
  --vms 3 \
  --rounds 3 \
  --branches 2 \
  --ticks 80 \
  --seed 42 \
  --mode hybrid \
  --bootstrap-budget 10000 \
  --memory-mb 128 \
  --extra-cmdline "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=25" \
  >"$run_log" 2>&1
run_rc=$?
set -e

if [[ "$run_rc" != 0 && "$run_rc" != 1 && "$run_rc" != 124 ]]; then
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
  >"$export_log" 2>&1

python3 - "$run_dir" "$selected_bug" <<'PY'
import hashlib
import json
import pathlib
import sys

run_dir = pathlib.Path(sys.argv[1]).resolve()
out_path = pathlib.Path(sys.argv[2])
selected = None
for path in sorted(run_dir.glob("bug_*.json")):
    bug = json.loads(path.read_text())
    ref = bug.get("replay_parent_snapshot_ref")
    if not (bug.get("replay_parent_depth", 0) > 0 and ref):
        continue
    if bug.get("assertion_id") != 1806003755:
        continue
    rel = pathlib.PurePosixPath(ref.get("path", ""))
    if rel.is_absolute() or ".." in rel.parts or rel.parts[:1] != ("snapshots",):
        raise SystemExit(f"unconfined snapshot ref path in {path}: {rel}")
    artifact = (run_dir / pathlib.Path(*rel.parts)).resolve()
    if run_dir not in artifact.parents:
        raise SystemExit(f"snapshot path escapes run dir in {path}: {artifact}")
    if not artifact.is_file():
        raise SystemExit(f"missing snapshot artifact in {path}: {artifact}")
    digest = ref.get("digest", "")
    if not digest.startswith("sha256:"):
        raise SystemExit(f"unsupported digest in {path}: {digest}")
    actual = "sha256:" + hashlib.sha256(artifact.read_bytes()).hexdigest()
    if actual != digest:
        raise SystemExit(f"snapshot digest mismatch in {path}: expected {digest}, got {actual}")
    if ref.get("codec") != "simulation-snapshot-bincode-zstd-v1":
        raise SystemExit(f"unexpected snapshot codec in {path}: {ref.get('codec')}")
    selected = path
    break

if selected is None:
    depths = []
    for path in sorted(run_dir.glob("bug_*.json")):
        bug = json.loads(path.read_text())
        depths.append({"file": path.name, "depth": bug.get("replay_parent_depth"), "has_ref": bool(bug.get("replay_parent_snapshot_ref"))})
    raise SystemExit("no snapshot-backed bug found; bugs=" + json.dumps(depths, sort_keys=True))

out_path.write_text(str(selected) + "\n")
print(f"selected {selected.name}")
PY

bug_path="$(cat "$selected_bug")"

timeout "$REPRO_TIMEOUT" "$CHAOSCONTROL_EXPLORE" reproduce \
  --kernel "$KERNEL" \
  --initrd "$INITRD" \
  --bug "$bug_path" \
  --vms 3 \
  --bootstrap-budget 10000 \
  --memory-mb 128 \
  --extra-cmdline "raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after=25" \
  >"$reproduce_log" 2>&1

if ! grep -q "BUG REPRODUCED" "$reproduce_log"; then
  echo "snapshot replay smoke: reproduce did not report BUG REPRODUCED" >&2
  tail -100 "$reproduce_log" >&2 || true
  exit 1
fi

summary="snapshot replay smoke ok: $(basename "$bug_path")"
echo "$summary"

if [[ -n "$OUT" ]]; then
  mkdir -p "$OUT"
  {
    echo "$summary"
    echo "run_rc=$run_rc"
    echo "bug=$bug_path"
  } >"$OUT/snapshot-replay-smoke.txt"
fi
