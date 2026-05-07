#!/usr/bin/env python3
"""Generate/check the operator-facing replay readiness status report."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "dogfood-results" / "accepted-workload-proofs.json"
REPORT = ROOT / "docs" / "replay-readiness-status.md"
SUPPORTED_STATUS = "supported-bounded"
EXPERIMENTAL = [
    {
        "surface": "Fresh workload authoring",
        "status": "experimental",
        "reason": "New workloads need their own bounded probe, accepted verdict, manifest entry, and committed snapshot artifact before promotion.",
    },
    {
        "surface": "Schedule-only replay",
        "status": "gap-evidence-only",
        "reason": "Depth-zero replay results classify replay gaps; they do not prove snapshot-backed replay coverage.",
    },
    {
        "surface": "Arbitrary guest/device determinism",
        "status": "unproven",
        "reason": "Current evidence covers named bounded workload rails only, not universal hypervisor/device/timing behavior.",
    },
    {
        "surface": "Full Antithesis-style product replacement",
        "status": "not-supported",
        "reason": "No hosted service, broad workload catalog, fleet-scale scheduler, UI, or formal determinism theorem is claimed by this evidence.",
    },
]


def load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        raise AssertionError(f"missing file: {path.relative_to(ROOT)}") from None
    except json.JSONDecodeError as exc:
        raise AssertionError(f"invalid JSON in {path.relative_to(ROOT)}: {exc}") from exc


def rel(path: Path) -> str:
    return str(path.relative_to(ROOT))


def proof_row(proof: dict[str, Any]) -> str:
    evidence_dir = ROOT / proof["evidence_dir"]
    verdict = load_json(evidence_dir / proof["verdict"])
    summary = load_json(evidence_dir / proof["summary"])
    return (
        f"| `{proof['workload']}` | `{SUPPORTED_STATUS}` | `{proof['assertion_id']}` | "
        f"`{verdict['replay_class']}` | `{verdict['replay_parent_depth']}` | "
        f"`{summary['export_exit_status']}` / `{summary['reproduce_exit_status']}` | "
        f"`{proof['evidence_dir']}/` |"
    )


def render() -> str:
    manifest = load_json(MANIFEST)
    proofs = manifest.get("proofs", [])
    if not proofs:
        raise AssertionError("accepted workload proof manifest has no proofs")

    workloads = ", ".join(f"`{p['workload']}`" for p in proofs)
    lines: list[str] = [
        "# Replay Readiness Status",
        "",
        "Generated from `dogfood-results/accepted-workload-proofs.json`. Do not hand-edit this file; run `python scripts/generate-replay-readiness-report.py --write`.",
        "",
        "## Summary",
        "",
        f"ChaosControl currently supports bounded snapshot-backed replay proof claims for: {workloads}.",
        "",
        "This status is evidence-backed but narrow: it is not a mathematical determinism proof, not a universal hypervisor/device/timing proof, and not a full Antithesis-style product replacement claim.",
        "",
        "## Supported bounded replay surfaces",
        "",
        "| Workload | Status | Assertion ID | Accepted verdict | Replay parent depth | export/reproduce exit | Evidence |",
        "| --- | --- | ---: | --- | ---: | --- | --- |",
    ]
    lines.extend(proof_row(p) for p in proofs)
    lines.extend([
        "",
        "Supported here means the committed evidence contains an accepted summary, exported bug artifact, Rust-owned replay verdict, `replay_parent_depth > 0`, and a present digest-matching `.snapshot.bin` artifact validated by `scripts/check-replay-proof-coverage.py`.",
        "",
        "## Experimental or unproven surfaces",
        "",
        "| Surface | Status | Why it is not promoted |",
        "| --- | --- | --- |",
    ])
    for item in EXPERIMENTAL:
        lines.append(f"| {item['surface']} | `{item['status']}` | {item['reason']} |")
    lines.extend([
        "",
        "## Promotion rule",
        "",
        "A new surface can move into `supported-bounded` only after it has committed evidence in the accepted workload manifest and all of these checks pass:",
        "",
        "```bash",
        "python scripts/check-replay-proof-coverage.py",
        "python scripts/generate-replay-readiness-report.py --check",
        "nix build .#checks.x86_64-linux.evidence-contracts --no-link -L",
        "```",
        "",
    ])
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--write", action="store_true", help="write docs/replay-readiness-status.md")
    mode.add_argument("--check", action="store_true", help="fail if the committed report is stale")
    args = parser.parse_args()

    try:
        content = render()
        if args.write:
            REPORT.write_text(content)
            print(f"wrote {rel(REPORT)}")
            return 0
        if args.check:
            try:
                current = REPORT.read_text()
            except FileNotFoundError:
                print(f"readiness report missing: {rel(REPORT)}", file=sys.stderr)
                return 1
            if current != content:
                print(f"readiness report stale: run python scripts/generate-replay-readiness-report.py --write", file=sys.stderr)
                return 1
            print(f"replay readiness report ok: {rel(REPORT)}")
            return 0
        print(content, end="")
        return 0
    except AssertionError as exc:
        print(f"replay readiness report failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
