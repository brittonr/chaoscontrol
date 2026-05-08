#!/usr/bin/env python3
"""Fail-closed promotion gate for assertion-readiness workload claims."""

from __future__ import annotations

import argparse
import copy
import json
import re
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
REQUIRED_SUMMARY_FRAGMENTS = (
    "assertion-density and uncovered-catalog view over accepted replay evidence",
    "not replay proof by itself",
)
REQUIRED_ANTI_CLAIM_FRAGMENTS = (
    "A high exercised count only says the committed run observed cataloged SDK assertions",
    "Product parity still requires workload setup ergonomics, replay evidence, minimization/reproduction UX, and operator triage surfaces",
)
FORBIDDEN_OVERCLAIM_FRAGMENTS = (
    "product parity is established",
    "full antithesis-style product replacement",
    "assertion density proves replay",
    "assertion coverage proves replay",
)
ROW_RE = re.compile(
    r"^\| `(?P<workload>[^`]+)` \| `(?P<cataloged>\d+)` \| `(?P<exercised>\d+)` "
    r"\| `(?P<always>\d+)` / `(?P<sometimes>\d+)` / `(?P<reachability>\d+)` / `(?P<unreachable>\d+)` "
    r"\| `(?P<uncategorized>\d+)` \| `(?P<nonpassing>\d+)` \| `(?P<evidence>[^`]+)` \|$"
)
GAP_RE = re.compile(r"^- (?P<workload>[^:]+): (?P<count>\d+) (?P<class>unhit|uncategorized|non-passing) assertion\(s\)$")


class AssertionReadinessPromotionError(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionReadinessPromotionError(message)


def rel(path: Path) -> str:
    return str(path.relative_to(ROOT))


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        raise AssertionReadinessPromotionError(f"missing file: {rel(path)}") from None
    except json.JSONDecodeError as exc:
        raise AssertionReadinessPromotionError(f"invalid JSON in {rel(path)}: {exc}") from exc


def load_report(path: Path) -> str:
    try:
        return path.read_text()
    except FileNotFoundError:
        raise AssertionReadinessPromotionError(f"missing file: {rel(path)}") from None


def manifest_proofs(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    require(manifest.get("schema_version") == 1, "manifest schema_version must be 1")
    proofs = manifest.get("proofs")
    require(isinstance(proofs, list) and proofs, "manifest proofs must be a non-empty list")
    seen_workloads: set[str] = set()
    for index, proof in enumerate(proofs):
        require(isinstance(proof, dict), f"proof[{index}] must be an object")
        workload = proof.get("workload")
        require(isinstance(workload, str) and workload, f"proof[{index}].workload must be non-empty")
        require(workload not in seen_workloads, f"duplicate workload proof: {workload}")
        seen_workloads.add(workload)
        evidence_dir = proof.get("evidence_dir")
        require(isinstance(evidence_dir, str) and evidence_dir, f"{workload}: proof.evidence_dir must be non-empty")
    return proofs


def expected_workload_summary(proof: dict[str, Any]) -> dict[str, Any]:
    workload = proof["workload"]
    evidence_dir = proof["evidence_dir"]
    assertions_path = ROOT / evidence_dir / "assertions.json"
    assertions = load_json(assertions_path)
    require(isinstance(assertions, list), f"{rel(assertions_path)}: expected assertion summary list")

    counts: Counter[str] = Counter()
    uncategorized = 0
    unhit = 0
    nonpassing = 0
    for item_index, item in enumerate(assertions):
        require(isinstance(item, dict), f"{rel(assertions_path)}[{item_index}]: assertion entry is not an object")
        raw_kind = str(item.get("kind", "unknown"))
        counts[KIND_LABELS.get(raw_kind, raw_kind)] += 1
        if item.get("category", "uncategorized") == "uncategorized":
            uncategorized += 1
        if int(item.get("hit_count", 0) or 0) == 0:
            unhit += 1
        if item.get("verdict") != "passed":
            nonpassing += 1

    cataloged = sum(counts.values())
    return {
        "workload": workload,
        "cataloged": cataloged,
        "exercised": cataloged - unhit,
        "always": counts["always"],
        "sometimes": counts["sometimes"],
        "reachability": counts["reachability"],
        "unreachable": counts["unreachable"],
        "uncategorized": uncategorized,
        "nonpassing": nonpassing,
        "evidence": f"{evidence_dir}/assertions.json",
        "gaps": {
            "unhit": unhit,
            "uncategorized": uncategorized,
            "non-passing": nonpassing,
        },
    }


def report_rows(report: str) -> dict[str, dict[str, Any]]:
    rows: dict[str, dict[str, Any]] = {}
    for line in report.splitlines():
        match = ROW_RE.match(line)
        if not match:
            continue
        workload = match.group("workload")
        require(workload not in rows, f"duplicate assertion-readiness row: {workload}")
        rows[workload] = {
            "workload": workload,
            "cataloged": int(match.group("cataloged")),
            "exercised": int(match.group("exercised")),
            "always": int(match.group("always")),
            "sometimes": int(match.group("sometimes")),
            "reachability": int(match.group("reachability")),
            "unreachable": int(match.group("unreachable")),
            "uncategorized": int(match.group("uncategorized")),
            "nonpassing": int(match.group("nonpassing")),
            "evidence": match.group("evidence"),
        }
    require(rows, "assertion-readiness report has no accepted proof coverage rows")
    return rows


def report_gaps(report: str) -> dict[tuple[str, str], int]:
    gaps: dict[tuple[str, str], int] = {}
    for line in report.splitlines():
        match = GAP_RE.match(line)
        if not match:
            continue
        key = (match.group("workload"), match.group("class"))
        require(key not in gaps, f"duplicate assertion-readiness gap line: {key[0]} {key[1]}")
        gaps[key] = int(match.group("count"))
    require(gaps, "assertion-readiness report has no promotion guidance gap lines")
    return gaps


def validate(manifest: dict[str, Any], report: str) -> list[str]:
    for fragment in REQUIRED_SUMMARY_FRAGMENTS + REQUIRED_ANTI_CLAIM_FRAGMENTS:
        require(fragment in report, f"assertion-readiness report missing anti-claim fragment: {fragment}")
    lowered_report = report.lower()
    for fragment in FORBIDDEN_OVERCLAIM_FRAGMENTS:
        require(fragment not in lowered_report, f"assertion-readiness report contains overclaim fragment: {fragment}")

    expected = {summary["workload"]: summary for summary in (expected_workload_summary(proof) for proof in manifest_proofs(manifest))}
    rows = report_rows(report)
    gaps = report_gaps(report)

    missing = sorted(set(expected) - set(rows))
    extra = sorted(set(rows) - set(expected))
    require(not missing, f"accepted manifest proofs missing from assertion-readiness report: {', '.join(missing)}")
    require(not extra, f"assertion-readiness report lists workloads missing from manifest: {', '.join(extra)}")

    numeric_fields = ("cataloged", "exercised", "always", "sometimes", "reachability", "unreachable", "uncategorized", "nonpassing")
    for workload, summary in expected.items():
        row = rows[workload]
        for field in numeric_fields:
            require(row[field] == summary[field], f"{workload}: report {field}={row[field]} does not match assertion artifacts {summary[field]}")
        require(row["evidence"] == summary["evidence"], f"{workload}: report evidence {row['evidence']} does not match {summary['evidence']}")
        for gap_class, count in summary["gaps"].items():
            actual = gaps.get((workload, gap_class))
            require(actual == count, f"{workload}: promotion guidance {gap_class} gap {actual!r}, expected {count}")

    return [
        f"{workload}: cataloged={summary['cataloged']} exercised={summary['exercised']} unhit={summary['gaps']['unhit']} uncategorized={summary['gaps']['uncategorized']} nonpassing={summary['gaps']['non-passing']}"
        for workload, summary in sorted(expected.items())
    ]


def run_selftest() -> int:
    manifest = load_json(MANIFEST)
    require(isinstance(manifest, dict), "manifest root must be an object")
    report = load_report(REPORT)
    validate(manifest, report)

    def expect_failure(name: str, candidate_manifest: dict[str, Any], candidate_report: str, needle: str) -> None:
        try:
            validate(candidate_manifest, candidate_report)
        except AssertionReadinessPromotionError as exc:
            if needle not in str(exc):
                raise AssertionError(f"{name}: expected {needle!r}, got {exc}") from exc
            return
        raise AssertionError(f"{name}: unexpectedly passed")

    missing_anticlaim = report.replace("but it is not replay proof by itself", "and is promotion-ready", 1)
    expect_failure("missing anti-claim", copy.deepcopy(manifest), missing_anticlaim, "missing anti-claim")

    hidden_gap = report.replace("- raft: 43 uncategorized assertion(s)\n", "", 1)
    expect_failure("hidden uncategorized gap", copy.deepcopy(manifest), hidden_gap, "raft: promotion guidance uncategorized")

    weakened_count = report.replace("| `redb` | `27` | `18` | `17` / `2` / `8` / `0` | `27` | `10` |", "| `redb` | `27` | `18` | `17` / `2` / `8` / `0` | `0` | `10` |", 1)
    expect_failure("weakened report count", copy.deepcopy(manifest), weakened_count, "redb: report uncategorized=0")

    report_only = report.replace("| `raft` | `43` |", "| `new-service` | `43` |", 1)
    expect_failure("report-only workload", copy.deepcopy(manifest), report_only, "missing from assertion-readiness report")

    overclaim = report + "\nFull Antithesis-style product replacement is now ready.\n"
    expect_failure("explicit overclaim", copy.deepcopy(manifest), overclaim, "overclaim fragment")

    print("assertion readiness promotion gate selftest ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument("--report", type=Path, default=REPORT)
    parser.add_argument("--selftest", action="store_true", help="run deterministic positive and negative fixtures")
    args = parser.parse_args()

    try:
        if args.selftest:
            return run_selftest()
        manifest = load_json(args.manifest)
        require(isinstance(manifest, dict), "manifest root must be an object")
        lines = validate(manifest, load_report(args.report))
        print("assertion readiness promotion gate ok:")
        for line in lines:
            print(f"  {line}")
        return 0
    except (AssertionReadinessPromotionError, AssertionError) as exc:
        print(f"assertion readiness promotion gate failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
