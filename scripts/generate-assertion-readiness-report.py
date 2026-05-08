#!/usr/bin/env python3
"""Generate/check assertion coverage readiness across accepted replay proofs."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "dogfood-results" / "accepted-workload-proofs.json"
REPORT = ROOT / "docs" / "assertion-readiness-status.md"

KIND_LABELS = {
    "always": "always",
    "sometimes": "sometimes",
    "reachable": "reachability",
    "reachability": "reachability",
    "unreachable": "unreachable",
}


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        raise AssertionError(f"missing file: {path.relative_to(ROOT)}") from None
    except json.JSONDecodeError as exc:
        raise AssertionError(f"invalid JSON in {path.relative_to(ROOT)}: {exc}") from exc


def rel(path: Path) -> str:
    return str(path.relative_to(ROOT))


def assertion_rows(proof: dict[str, Any]) -> tuple[str, list[str]]:
    evidence_dir = ROOT / proof["evidence_dir"]
    assertions_path = evidence_dir / "assertions.json"
    assertions = load_json(assertions_path)
    if not isinstance(assertions, list):
        raise AssertionError(f"{rel(assertions_path)}: expected assertion summary list")

    counts = Counter()
    uncategorized = 0
    unhit: list[str] = []
    nonpassing: list[str] = []
    for item in assertions:
        if not isinstance(item, dict):
            raise AssertionError(f"{rel(assertions_path)}: assertion entry is not an object")
        kind = KIND_LABELS.get(str(item.get("kind", "unknown")), str(item.get("kind", "unknown")))
        counts[kind] += 1
        if item.get("category", "uncategorized") == "uncategorized":
            uncategorized += 1
        if int(item.get("hit_count", 0) or 0) == 0:
            unhit.append(str(item.get("message", item.get("id", "<unnamed>"))))
        if item.get("verdict") != "passed":
            nonpassing.append(str(item.get("message", item.get("id", "<unnamed>"))))

    total = sum(counts.values())
    exercised = total - len(unhit)
    return (
        f"| `{proof['workload']}` | `{total}` | `{exercised}` | `{counts['always']}` / `{counts['sometimes']}` / `{counts['reachability']}` / `{counts['unreachable']}` | `{uncategorized}` | `{len(nonpassing)}` | `{proof['evidence_dir']}/assertions.json` |",
        [
            f"{proof['workload']}: {len(unhit)} unhit assertion(s)",
            f"{proof['workload']}: {uncategorized} uncategorized assertion(s)",
            f"{proof['workload']}: {len(nonpassing)} non-passing assertion(s)",
        ],
    )


def render() -> str:
    manifest = load_json(MANIFEST)
    proofs = manifest.get("proofs", [])
    if not proofs:
        raise AssertionError("accepted workload proof manifest has no proofs")

    rows: list[str] = []
    gaps: list[str] = []
    for proof in proofs:
        row, proof_gaps = assertion_rows(proof)
        rows.append(row)
        gaps.extend(proof_gaps)

    lines: list[str] = [
        "# Assertion Readiness Status",
        "",
        "Generated from `dogfood-results/accepted-workload-proofs.json` and each committed `assertions.json`. Do not hand-edit this file; run `python scripts/generate-assertion-readiness-report.py --write`.",
        "",
        "## Summary",
        "",
        "This report is an assertion-density and uncovered-catalog view over accepted replay evidence. It helps decide whether a workload is richly instrumented enough to be a credible Antithesis-alternative rail, but it is not replay proof by itself.",
        "",
        "## Accepted proof assertion coverage",
        "",
        "| Workload | Cataloged | Exercised | always / sometimes / reachability / unreachable | Uncategorized | Non-passing | Evidence |",
        "| --- | ---: | ---: | --- | ---: | ---: | --- |",
        *rows,
        "",
        "## Promotion guidance",
        "",
        "Before promoting a workload beyond a bounded replay proof, review these gaps and either add meaningful assertion categories/coverage or explicitly document why the remaining gaps are acceptable for that workload:",
        "",
    ]
    lines.extend(f"- {gap}" for gap in gaps)
    lines.extend([
        "",
        "## Anti-claim",
        "",
        "A high exercised count only says the committed run observed cataloged SDK assertions. Product parity still requires workload setup ergonomics, replay evidence, minimization/reproduction UX, and operator triage surfaces outside this report.",
        "",
    ])
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--write", action="store_true", help="write docs/assertion-readiness-status.md")
    mode.add_argument("--check", action="store_true", help="fail if the committed assertion report is stale")
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
                print(f"assertion readiness report missing: {rel(REPORT)}", file=sys.stderr)
                return 1
            if current != content:
                print("assertion readiness report stale: run python scripts/generate-assertion-readiness-report.py --write", file=sys.stderr)
                return 1
            print(f"assertion readiness report ok: {rel(REPORT)}")
            return 0
        print(content, end="")
        return 0
    except AssertionError as exc:
        print(f"assertion readiness report failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
