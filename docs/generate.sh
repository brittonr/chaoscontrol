#!/usr/bin/env bash
# Generate ChaosControl documentation from code.
#
# Sources of truth:
#   - //! crate doc comments  → crate guide pages
#   - /// module doc comments  → module index
#   - clap --help              → CLI reference
#   - Cargo.toml workspace     → architecture overview
#   - cargo doc                → API reference (separate)
#
# Usage: ./docs/generate.sh [--out DIR]
#
# Produces: DIR/src/ (mdBook source) ready for `mdbook build`.
# If --cargo-doc is passed, also runs cargo doc into DIR/api/.

set -euo pipefail
unset CDPATH

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/docs/generated}"
CARGO_TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"

rm -rf "$OUT/src"
mkdir -p "$OUT/src/cli" "$OUT/src/crates" "$OUT/src/design"

# ── Helpers ──────────────────────────────────────────────────

# Extract //! doc comments from a file, stripping the //! prefix.
extract_crate_doc() {
    local file="$1"
    sed -n '/^\/\/!/{ s/^\/\/! \?//; p; }' "$file" | sed '/^$/{ N; /^\n$/d; }'
}

# Extract /// doc comments above `pub mod NAME` or `pub fn NAME` etc.
extract_module_doc() {
    local file="$1"
    local module="$2"
    # Get lines of /// comments immediately before the pub mod/fn line.
    awk -v mod="$module" '
        /^\/\/\// { buf = buf $0 "\n"; next }
        /pub.*(mod|fn|struct|enum|trait|type) +'"$module"'/ { if (buf != "") print buf; buf = ""; next }
        { buf = "" }
    ' "$file" | sed 's/^\/\/\/ \?//'
}

# Run a binary and capture its --help.  Falls back to empty if binary
# doesn't exist (guest binaries need musl, won't run on host).
capture_help() {
    local bin="$1"
    local subcmd="${2:-}"
    local binary="$CARGO_TARGET/release/$bin"
    if [ ! -x "$binary" ]; then
        binary="$CARGO_TARGET/debug/$bin"
    fi
    if [ ! -x "$binary" ]; then
        echo "(binary not found — build with cargo build first)"
        return
    fi
    if [ -n "$subcmd" ]; then
        "$binary" "$subcmd" --help 2>/dev/null || true
    else
        "$binary" --help 2>/dev/null || true
    fi
}

# ── Index page ───────────────────────────────────────────────

# Pull from README but stop before the Architecture section
# (the generated site IS the architecture docs).
cat > "$OUT/src/index.md" << 'EOF'
# ChaosControl

A deterministic Virtual Machine Monitor for simulation testing of
distributed systems.  Built with KVM and the rust-vmm crate ecosystem.

ChaosControl controls every source of non-determinism in a VM — time,
entropy, scheduling, I/O — so that executions are perfectly
reproducible.  This lets you do coverage-guided exploration of
distributed system state spaces: fork from snapshots, inject faults,
and verify safety properties across thousands of alternate timelines.

## Quick start

```bash
# Run the Raft exploration (builds everything via Nix)
nix run .#explore-raft

# Or manually:
nix build .#initrd-raft -o result-initrd-raft
nix build .#net-vmlinux -o result-net
cargo build --release
chaoscontrol-explore run \
  --kernel result-net/vmlinux \
  --initrd result-initrd-raft/initrd.gz \
  --vms 3 --rounds 10 --branches 8 --ticks 1000 \
  --seed 42 --mode hybrid
```

## How it works

1. **Boot** guest VMs inside a deterministic hypervisor
2. **Snapshot** the initial state after guest setup
3. **Explore** by forking from the snapshot, injecting faults, running forward
4. **Collect** coverage bitmaps and assertion verdicts from each branch
5. **Repeat** — coverage-guided search prioritizes unexplored edges

The guest SDK (inspired by [Antithesis](https://antithesis.com)) lets
you annotate your system under test with `always`, `sometimes`,
`reachable`, and `unreachable` assertions.  The explorer reports which
assertions pass, fail, or are never exercised.

EOF

# ── Architecture page ────────────────────────────────────────

cat > "$OUT/src/architecture.md" << 'HEADER'
# Architecture

## Workspace

ChaosControl is a Cargo workspace with 12 crates.  Data flows from
bottom to top: the protocol crate defines the wire format, the SDK uses
it from inside the guest, the VMM interprets it on the host, and the
exploration/replay crates orchestrate multiple VMs.

HEADER

# Auto-generate the crate table from Cargo.toml
echo '| Crate | Description |' >> "$OUT/src/architecture.md"
echo '|-------|-------------|' >> "$OUT/src/architecture.md"

for crate_dir in "$ROOT"/crates/*/; do
    name=$(basename "$crate_dir")
    # Get the first //! line as a one-liner description.
    desc=""
    for entry in "$crate_dir/src/lib.rs" "$crate_dir/src/main.rs"; do
        if [ -f "$entry" ]; then
            desc=$(head -1 "$entry" | sed 's/^\/\/! //')
            break
        fi
    done
    echo "| [\`$name\`](crates/${name}.md) | $desc |" >> "$OUT/src/architecture.md"
done

cat >> "$OUT/src/architecture.md" << 'FOOTER'

## Dependency graph

```
                    ┌─────────────────────────┐
                    │   chaoscontrol-explore   │  exploration engine
                    │   chaoscontrol-replay    │  recording + replay
                    └────────────┬────────────┘
                                 │ uses
                    ┌────────────▼────────────┐
                    │    chaoscontrol-vmm      │  deterministic VMM
                    │    chaoscontrol-fault    │  fault injection
                    └────────────┬────────────┘
                                 │ uses
                    ┌────────────▼────────────┐
                    │  chaoscontrol-protocol   │  wire format (no_std)
                    └────────────┬────────────┘
                                 │ used by
              ┌──────────────────┼──────────────────┐
              ▼                  ▼                   ▼
     chaoscontrol-sdk    chaoscontrol-guest   chaoscontrol-raft-guest
      (guest library)     (demo guest)         (Raft consensus)
```

## Determinism model

Every VM exit increments a virtual TSC counter.  Time, entropy, device
state, and scheduling are all derived from this counter and a seed.
Two runs with the same seed produce bit-identical execution traces.

Sources of non-determinism and how they're controlled:

| Source | Mechanism |
|--------|-----------|
| Wall-clock time | Virtual TSC written to `IA32_TSC` MSR before each `vcpu.run()` |
| `RDTSC` / `RDTSCP` | Reads the injected virtual TSC; `RDTSCP` filtered via CPUID |
| `RDRAND` / `RDSEED` | Filtered via CPUID; guest uses SDK `get_random()` instead |
| PIT calibration | Channel 2 frozen; CPUID leaf `0x15` injected (25 MHz × 120 = 3 GHz) |
| HPET | `nohpet` cmdline; MMIO region trapped, returns vTSC-derived values |
| ACPI PM timer | Port `0x408` trapped, returns vTSC-derived 24-bit counter |
| Network | Simulated `NetworkFabric` with deterministic queuing |
| Disk I/O | Copy-on-write block device, no host filesystem interaction |
| Entropy | Seeded ChaCha20 PRNG per VM |
| SMP scheduling | Serialized execution, deterministic round-robin or randomized (seeded) |
| Host interrupts (`SIGALRM`) | Invisible to scheduler; no exit count / vTSC change |

FOOTER

# ── Per-crate pages ──────────────────────────────────────────

for crate_dir in "$ROOT"/crates/*/; do
    name=$(basename "$crate_dir")
    outfile="$OUT/src/crates/${name}.md"

    echo "# \`$name\`" > "$outfile"
    echo "" >> "$outfile"

    # Crate-level doc comment
    for entry in "$crate_dir/src/lib.rs" "$crate_dir/src/main.rs"; do
        if [ -f "$entry" ]; then
            extract_crate_doc "$entry" >> "$outfile"
            echo "" >> "$outfile"
            break
        fi
    done

    # Module listing with doc summaries
    src_dir="$crate_dir/src"
    modules=()
    for rs in "$src_dir"/*.rs; do
        [ -f "$rs" ] || continue
        mod=$(basename "$rs" .rs)
        [ "$mod" = "lib" ] || [ "$mod" = "main" ] && continue
        modules+=("$mod")
    done

    # Check for subdirectory modules (devices/, verified/)
    for subdir in "$src_dir"/*/; do
        [ -d "$subdir" ] || continue
        subname=$(basename "$subdir")
        if [ -f "$subdir/mod.rs" ]; then
            modules+=("$subname")
        fi
    done

    if [ ${#modules[@]} -gt 0 ]; then
        echo "## Modules" >> "$outfile"
        echo "" >> "$outfile"
        echo "| Module | Description |" >> "$outfile"
        echo "|--------|-------------|" >> "$outfile"

        for mod in "${modules[@]}"; do
            # Get first line of module-level //! doc comment
            mod_file=""
            if [ -f "$src_dir/$mod.rs" ]; then
                mod_file="$src_dir/$mod.rs"
            elif [ -f "$src_dir/$mod/mod.rs" ]; then
                mod_file="$src_dir/$mod/mod.rs"
            fi
            mod_desc=""
            if [ -n "$mod_file" ]; then
                mod_desc=$(head -1 "$mod_file" | grep '^//!' | sed 's/^\/\/! //' || true)
            fi
            echo "| \`$mod\` | $mod_desc |" >> "$outfile"
        done
        echo "" >> "$outfile"
    fi

    # Public type summary — count pub structs, enums, traits, functions
    pub_structs=$({ grep -r 'pub struct ' "$src_dir" --include="*.rs" 2>/dev/null || true; } | { grep -v 'pub(crate)' || true; } | wc -l)
    pub_enums=$({ grep -r 'pub enum ' "$src_dir" --include="*.rs" 2>/dev/null || true; } | { grep -v 'pub(crate)' || true; } | wc -l)
    pub_traits=$({ grep -r 'pub trait ' "$src_dir" --include="*.rs" 2>/dev/null || true; } | { grep -v 'pub(crate)' || true; } | wc -l)
    pub_fns=$({ grep -r 'pub fn \|pub async fn \|pub const fn \|pub unsafe fn ' "$src_dir" --include="*.rs" 2>/dev/null || true; } | { grep -v 'pub(crate)' || true; } | wc -l)

    if [ "$((pub_structs + pub_enums + pub_traits + pub_fns))" -gt 0 ]; then
        echo "## Public API summary" >> "$outfile"
        echo "" >> "$outfile"
        [ "$pub_structs" -gt 0 ] && echo "- **$pub_structs** structs" >> "$outfile"
        [ "$pub_enums" -gt 0 ] && echo "- **$pub_enums** enums" >> "$outfile"
        [ "$pub_traits" -gt 0 ] && echo "- **$pub_traits** traits" >> "$outfile"
        [ "$pub_fns" -gt 0 ] && echo "- **$pub_fns** functions" >> "$outfile"
        echo "" >> "$outfile"
        echo "See the [API reference](../api/${name//-/_}/index.html) for full details." >> "$outfile"
        echo "" >> "$outfile"
    fi
done

# ── CLI reference ────────────────────────────────────────────

generate_cli_page() {
    local bin="$1"
    local title="$2"
    local outfile="$3"
    shift 3
    local subcmds=("$@")

    echo "# \`$bin\`" > "$outfile"
    echo "" >> "$outfile"
    echo "$title" >> "$outfile"
    echo "" >> "$outfile"
    echo '```' >> "$outfile"
    capture_help "$bin" >> "$outfile"
    echo '```' >> "$outfile"

    for sub in "${subcmds[@]}"; do
        echo "" >> "$outfile"
        echo "## \`$bin $sub\`" >> "$outfile"
        echo "" >> "$outfile"
        echo '```' >> "$outfile"
        capture_help "$bin" "$sub" >> "$outfile"
        echo '```' >> "$outfile"
    done
}

generate_cli_page "chaoscontrol-explore" \
    "Coverage-guided exploration engine.  Forks VMs from snapshots, injects faults, and searches for assertion violations." \
    "$OUT/src/cli/explore.md" \
    "run" "resume" "minimize" "reproduce"

generate_cli_page "chaoscontrol-replay" \
    "Time-travel debugger and replay tool.  Reproduces recorded sessions, generates bug reports, and provides interactive debugging." \
    "$OUT/src/cli/replay.md" \
    "replay" "triage" "info" "events" "debug" "dlog"

# Capture dlog subcommands too
DLOG_SUBS=()
dlog_binary="$CARGO_TARGET/release/chaoscontrol-replay"
[ ! -x "$dlog_binary" ] && dlog_binary="$CARGO_TARGET/debug/chaoscontrol-replay"
if [ -x "$dlog_binary" ]; then
    for dsub in dump diff stats; do
        echo "" >> "$OUT/src/cli/replay.md"
        echo "## \`chaoscontrol-replay dlog $dsub\`" >> "$OUT/src/cli/replay.md"
        echo "" >> "$OUT/src/cli/replay.md"
        echo '```' >> "$OUT/src/cli/replay.md"
        "$dlog_binary" dlog "$dsub" --help 2>/dev/null >> "$OUT/src/cli/replay.md" || true
        echo '```' >> "$OUT/src/cli/replay.md"
    done
fi

# ── Design docs (existing markdown, included as-is) ─────────

for md in "$ROOT"/docs/*.md; do
    [ -f "$md" ] || continue
    base=$(basename "$md")
    [ "$base" = "generate.sh" ] && continue
    cp "$md" "$OUT/src/design/$base"
done

# Copy glossary
if [ -f "$ROOT/GLOSSARY.md" ]; then
    cp "$ROOT/GLOSSARY.md" "$OUT/src/glossary.md"
fi

# ── Nix integration page ────────────────────────────────────

cat > "$OUT/src/nix.md" << 'EOF'
# Nix integration

ChaosControl uses Nix flakes for reproducible builds.  All guest
binaries, kernels, and initrds are Nix packages.

## Flake outputs

### Packages

| Output | Description |
|--------|-------------|
| `chaoscontrol-vmm` | Host-side VMM + CLI binaries |
| `guest-raft` | Raft consensus guest (musl static binary) |
| `guest-sdk` | Minimal SDK demo guest |
| `guest-net` | Network demo guest |
| `initrd-raft` | Initrd image containing the Raft guest |
| `initrd-sdk` | Initrd image containing the SDK guest |
| `initrd-net` | Initrd image containing the net guest |
| `net-vmlinux` | Linux kernel with virtio-net support |
| `kcov-vmlinux` | Linux kernel with KCOV enabled |
| `kcov-net-vmlinux` | Both virtio-net and KCOV |
| `raft-sim` | Full Raft simulation test (needs KVM) |

### Apps

| Command | Description |
|---------|-------------|
| `nix run .#explore-raft` | Run Raft exploration (builds everything) |
| `nix run .#explore` | Run explorer with manual arguments |
| `nix run .#boot` | Boot a single VM |
| `nix run .#replay` | Replay a recording |

### Checks

```bash
nix flake check   # build + test + clippy + fmt + nixfmt
```

## Downstream usage

Add ChaosControl as a flake input to test your own system:

```nix
{
  inputs.chaoscontrol.url = "github:user/chaoscontrol";

  outputs = { self, chaoscontrol, nixpkgs }: let
    pkgs = nixpkgs.legacyPackages.x86_64-linux;
    cc = chaoscontrol.packages.x86_64-linux;
  in {
    packages.x86_64-linux.my-sim = cc.mkChaosTest {
      name = "my-system-test";
      kernel = cc.mkChaosKernel { virtioNet = true; };
      initrd = cc.mkChaosInitrd {
        guest = self.packages.x86_64-linux.my-guest;
      };
      vms = 5;
      rounds = 20;
      branches = 16;
      ticks = 2000;
      seed = 42;
      mode = "hybrid";
    };
  };
}
```

EOF

# ── SDK quick reference ──────────────────────────────────────

cat > "$OUT/src/sdk.md" << 'SDKEOF'
# SDK quick reference

The `chaoscontrol-sdk` crate is added to your guest program.  It
provides assertions, guided randomness, lifecycle events, and coverage
instrumentation.  The API is modeled after the
[Antithesis SDK](https://antithesis.com/docs/using_antithesis/sdk/).

## Setup

```toml
[dependencies]
chaoscontrol-sdk = { path = "../chaoscontrol-sdk" }
```

```rust
use chaoscontrol_sdk::prelude::*;

fn main() {
    chaoscontrol_init!();

    // ... your system under test ...

    lifecycle::setup_complete();

    loop {
        // ... workload ...
    }
}
```

## Assertions

```rust
// Safety properties — must hold on every hit
cc_assert_always!(leader_count <= 1,
    "election safety: at most one leader per term",
    &json!({"term": term, "leaders": leader_count})
);

// Liveness properties — must hold at least once across the run
cc_assert_sometimes!(commit_index > 0,
    "progress: something gets committed",
    &json!({"commit": commit_index})
);

// Reachability — verify code paths are exercised
cc_assert_reachable!("leader elected",
    &json!({"node": id, "term": term})
);

// Unreachability — verify error paths are never taken
cc_assert_unreachable!("committed entry overwritten",
    &json!({"index": idx})
);

// Compound — always true when hit, unreachable if never hit
cc_assert_always_or_unreachable!(condition, "msg", &details);

// Comparisons
cc_assert_always_le!(a, b, "a ≤ b", &details);
cc_assert_sometimes_gt!(x, 0, "x eventually positive", &details);
```

## Guided randomness

```rust
// Get a random choice from [0, n)
let choice = random::random_choice(3);

// Get random bytes (seeded, deterministic)
let val: u64 = random::get_random();

// Choose from a slice
let node = random::random_choice_from(&nodes);

// Full rand integration
use rand::Rng;
let mut rng = chaoscontrol_sdk::random::ChaosControlRng::new();
let x: f64 = rng.gen();
```

## Lifecycle

```rust
lifecycle::setup_complete();          // signals "ready for faults"
lifecycle::send_event("msg", &json);  // named event for debugging
```

## Guidance

```rust
guidance::maximize("throughput", value);      // explorer prefers higher
guidance::minimize("latency_p99", value);     // explorer prefers lower
```

SDKEOF

# ── SUMMARY.md ───────────────────────────────────────────────

cat > "$OUT/src/SUMMARY.md" << 'HEADER'
# Summary

[Overview](index.md)

# User guide

- [SDK quick reference](sdk.md)
- [Nix integration](nix.md)

# CLI reference

- [chaoscontrol-explore](cli/explore.md)
- [chaoscontrol-replay](cli/replay.md)

# Architecture

- [Overview](architecture.md)
HEADER

# Add crate pages in dependency order
for name in \
    chaoscontrol-protocol \
    chaoscontrol-sdk \
    chaoscontrol-fault \
    chaoscontrol-vmm \
    chaoscontrol-explore \
    chaoscontrol-replay \
    chaoscontrol-trace \
    chaoscontrol-dashboard \
    chaoscontrol-guest \
    chaoscontrol-raft-guest \
    chaoscontrol-guest-net \
    chaoscontrol-net-guest; do
    echo "  - [\`$name\`](crates/${name}.md)" >> "$OUT/src/SUMMARY.md"
done

cat >> "$OUT/src/SUMMARY.md" << 'FOOTER'

# Design notes

FOOTER

# Add design docs
for md in "$OUT"/src/design/*.md; do
    [ -f "$md" ] || continue
    base=$(basename "$md")
    title=$(head -1 "$md" | sed 's/^# //')
    echo "- [$title](design/$base)" >> "$OUT/src/SUMMARY.md"
done

# Glossary at the end
if [ -f "$OUT/src/glossary.md" ]; then
    echo "" >> "$OUT/src/SUMMARY.md"
    echo "---" >> "$OUT/src/SUMMARY.md"
    echo "" >> "$OUT/src/SUMMARY.md"
    echo "[Glossary](glossary.md)" >> "$OUT/src/SUMMARY.md"
fi

# ── book.toml ────────────────────────────────────────────────

cat > "$OUT/book.toml" << 'TOML'
[book]
title = "ChaosControl"
description = "Deterministic VMM for simulation testing"
src = "src"

[output.html]
default-theme = "navy"
preferred-dark-theme = "navy"
git-repository-url = "https://github.com/user/chaoscontrol"
additional-css = []
TOML

echo "Generated mdBook source in $OUT/src/"
echo "Run: mdbook build $OUT"
