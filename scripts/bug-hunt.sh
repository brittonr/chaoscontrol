#!/usr/bin/env bash
# Run ChaosControl exploration against each Raft bug variant.
# Reports which bugs are found, how many rounds/branches to find them.
#
# Usage: nix develop --command bash -c "scripts/bug-hunt.sh"
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

KERNEL="result-dev/vmlinux"
INITRD="guest/initrd-raft.gz"
OUTPUT_BASE="bug-hunt-results"

# Exploration parameters — tuned for speed while still finding bugs
ROUNDS=10
BRANCHES=8
TICKS=2000
VMS=1
SEED=42

# Bug variants to test
BUGS=(
    "none"           # Control: should find 0 bugs
    "fig8_commit"    # Raft §5.4.2 — commits old-term entries
    "skip_truncate"  # Missing log truncation on conflict
    "accept_stale_term"  # Accepts AppendEntries from lower terms
    "leader_no_stepdown" # Leader ignores higher-term AE
    "double_vote"    # Votes for multiple candidates in same term
    "premature_commit"   # Commits before log consistency check
)

# Validate prerequisites
if [ ! -f "$KERNEL" ]; then
    echo "ERROR: Kernel not found at $KERNEL"
    echo "  Run: nix build .#dev-kernel -o result-dev"
    exit 1
fi

if [ ! -f "$INITRD" ]; then
    echo "ERROR: Initrd not found at $INITRD"
    echo "  Run: scripts/build-raft-guest.sh"
    exit 1
fi

# Build explorer in release mode
echo "==> Building chaoscontrol-explore (release)..."
cargo build --release --bin chaoscontrol-explore 2>&1 | tail -1
EXPLORE="${CARGO_TARGET_DIR:-target}/release/chaoscontrol-explore"

echo ""
echo "═══════════════════════════════════════════════════════════════════════"
echo "  ChaosControl Bug Hunt"
echo "═══════════════════════════════════════════════════════════════════════"
echo ""
echo "  Kernel:     $KERNEL"
echo "  Initrd:     $INITRD"
echo "  VMs:        $VMS"
echo "  Rounds:     $ROUNDS"
echo "  Branches:   $BRANCHES"
echo "  Ticks:      $TICKS"
echo "  Seed:       $SEED"
echo ""

mkdir -p "$OUTPUT_BASE"

# Results table
declare -A RESULTS
declare -A TIMES

for bug in "${BUGS[@]}"; do
    echo "───────────────────────────────────────────────────────────────────────"
    echo "  Testing: raft_bug=$bug"
    echo "───────────────────────────────────────────────────────────────────────"
    echo ""

    OUTDIR="$OUTPUT_BASE/$bug"
    mkdir -p "$OUTDIR"

    EXTRA=""
    if [ "$bug" != "none" ]; then
        EXTRA="raft_bug=$bug"
    fi

    START_TIME=$(date +%s)

    # Run exploration, capture exit code
    set +e
    RUST_LOG=info "$EXPLORE" run \
        --kernel "$KERNEL" \
        --initrd "$INITRD" \
        --vms "$VMS" \
        --seed "$SEED" \
        --rounds "$ROUNDS" \
        --branches "$BRANCHES" \
        --ticks "$TICKS" \
        --bootstrap-budget 5000 \
        --output "$OUTDIR" \
        ${EXTRA:+--extra-cmdline "$EXTRA"} \
        > "$OUTDIR/stdout.txt" 2> "$OUTDIR/stderr.txt"
    EXIT_CODE=$?
    set -e

    END_TIME=$(date +%s)
    ELAPSED=$((END_TIME - START_TIME))

    # Count bugs found (exit code 1 = bugs found)
    if [ $EXIT_CODE -eq 1 ]; then
        BUG_COUNT=$(grep -c "BUG #" "$OUTDIR/stdout.txt" 2>/dev/null || echo "?")
        RESULTS[$bug]="FOUND ($BUG_COUNT bugs)"
    elif [ $EXIT_CODE -eq 0 ]; then
        RESULTS[$bug]="NOT FOUND"
    else
        RESULTS[$bug]="ERROR (exit=$EXIT_CODE)"
    fi
    TIMES[$bug]="${ELAPSED}s"

    echo "  Result: ${RESULTS[$bug]} in ${TIMES[$bug]}"
    echo ""

    # Show brief summary from report
    if [ -f "$OUTDIR/stdout.txt" ]; then
        grep -E "Bugs found|Total edges|Rounds completed|Assertion" "$OUTDIR/stdout.txt" | head -5 || true
    fi
    echo ""
done

# Summary table
echo ""
echo "═══════════════════════════════════════════════════════════════════════"
echo "  Bug Hunt Results"
echo "═══════════════════════════════════════════════════════════════════════"
echo ""
printf "  %-22s %-20s %s\n" "BUG VARIANT" "RESULT" "TIME"
printf "  %-22s %-20s %s\n" "──────────────────────" "────────────────────" "─────"
for bug in "${BUGS[@]}"; do
    printf "  %-22s %-20s %s\n" "$bug" "${RESULTS[$bug]}" "${TIMES[$bug]}"
done
echo ""

# Count how many bugs were found
FOUND=0
TOTAL=${#BUGS[@]}
for bug in "${BUGS[@]}"; do
    if [[ "${RESULTS[$bug]}" == FOUND* ]]; then
        ((FOUND++))
    fi
done

# none should NOT be found (it's the control)
echo "  Found $FOUND/${TOTAL} variants (including control)"
echo ""
echo "  Full results in: $OUTPUT_BASE/"
