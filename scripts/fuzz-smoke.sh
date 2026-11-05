#!/usr/bin/env bash
# Bounded libFuzzer smoke over guest-controlled protocol frames and virtio planners.
# r[impl chaoscontrol.fuzzing.smoke_rail]
set -euo pipefail

seconds="${FUZZ_SECONDS:-60}"
if [ -n "${FUZZ_ROOT:-}" ]; then
    root="$FUZZ_ROOT"
elif [ -d "$PWD/crates/chaoscontrol-protocol/fuzz" ]; then
    # Invoked as a nix app from the repository root.
    root="$PWD"
else
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi

# libFuzzer needs coverage instrumentation and the address sanitizer; both require
# a nightly toolchain (the nix app supplies rust-bin.nightly, otherwise put one on
# PATH and export RUSTC/cargo from it).
export RUSTFLAGS="${FUZZ_RUSTFLAGS:--Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=4 -Cllvm-args=-sanitizer-coverage-inline-8bit-counters -Cllvm-args=-sanitizer-coverage-pc-table -Cllvm-args=-sanitizer-coverage-trace-compares --cfg fuzzing -Clink-dead-code -Zsanitizer=address -Ccodegen-units=1}"

run_crate() {
    local crate="$1"
    shift
    local fuzz="$root/crates/$crate/fuzz"
    if [ ! -d "$fuzz" ]; then
        echo "missing fuzz crate: $fuzz" >&2
        exit 2
    fi
    (
        cd "$fuzz"
        CARGO_TARGET_DIR="$fuzz/target" cargo build --release --bins --target x86_64-unknown-linux-gnu
        for target in "$@"; do
            echo "fuzz smoke: $crate::$target for ${seconds}s"
            "./target/x86_64-unknown-linux-gnu/release/$target" \
                -max_total_time="$seconds" \
                -timeout=5 \
                -rss_limit_mb=1024 \
                -malloc_limit_mb=512
        done
    )
}

run_crate chaoscontrol-protocol payload catalog_frames
run_crate chaoscontrol-vmm virtio_chain virtio_request
echo "fuzz smoke completed; no crashers"
