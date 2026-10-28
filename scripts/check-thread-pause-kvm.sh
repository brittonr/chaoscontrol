#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
LINKER=$(command -v x86_64-unknown-linux-musl-ld)
RUSTFLAGS="-C relocation-model=static -C link-arg=-Wl,--allow-multiple-definition -C link-arg=-nostartfiles -C link-arg=-static -C link-arg=-no-pie -C link-arg=-Wl,-T$ROOT/crates/chaoscontrol-thread-guest/linker.ld" \
  cargo build --manifest-path "$ROOT/crates/chaoscontrol-thread-guest/Cargo.toml" --release --target x86_64-unknown-linux-musl
GUEST="$ROOT/.cargo-target/x86_64-unknown-linux-musl/release/chaoscontrol-thread-guest"
if [ ! -x "$GUEST" ]; then
  GUEST=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"] + "/x86_64-unknown-linux-musl/release/chaoscontrol-thread-guest")')
fi
CHAOSCONTROL_THREAD_GUEST="$GUEST" cargo test -p chaoscontrol-vmm --test thread_pause_kvm -- --ignored --nocapture
