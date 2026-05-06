#!/usr/bin/env bash
# Attach Linux perf stat counters to an arbitrary command.
# Usage:
#   scripts/perf-stat.sh cargo run --release --bin chaoscontrol-explore -- run --kernel vmlinux --rounds 5
# Override counters with:
#   PERF_EVENTS="cycles,instructions,L1-dcache-load-misses" scripts/perf-stat.sh ...
set -euo pipefail

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <command> [args...]" >&2
  exit 64
fi

if ! command -v perf >/dev/null 2>&1; then
  echo "error: perf is not available on PATH" >&2
  exit 127
fi

EVENTS=${PERF_EVENTS:-cycles,instructions,cache-references,cache-misses,branch-instructions,branch-misses,context-switches}

"$@" &
child=$!

cleanup() {
  if kill -0 "$child" >/dev/null 2>&1; then
    kill "$child" >/dev/null 2>&1 || true
  fi
}
trap cleanup INT TERM

# Give a fast-failing command one scheduler tick to report failure before attach.
sleep 0.05
if ! kill -0 "$child" >/dev/null 2>&1; then
  wait "$child"
  status=$?
  echo "error: command exited before perf could attach (status $status)" >&2
  exit "$status"
fi

perf stat -e "$EVENTS" -p "$child" &
perf_pid=$!

wait "$child"
status=$?
wait "$perf_pid" || perf_status=$?
trap - INT TERM

if [[ ${perf_status:-0} -ne 0 ]]; then
  echo "warning: perf stat exited with status ${perf_status}" >&2
fi

exit "$status"
