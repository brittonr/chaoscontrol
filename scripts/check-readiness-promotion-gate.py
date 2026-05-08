#!/usr/bin/env python3
"""Fail-closed promotion gate for replay-readiness supported workload claims."""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "dogfood-results" / "accepted-workload-proofs.json"
REPORT = ROOT / "docs" / "replay-readiness-status.md"
REQUIRED_EXPERIMENTAL_SURFACES = {
    "Fresh workload authoring": "experimental",
    "Schedule-only replay": "gap-evidence-only",
    "Arbitrary guest/device determinism": "unproven",
    "Full Antithesis-style product replacement": "not-supported",
}
REQUIRED_ANTI_CLAIM_FRAGMENTS = (
    "does not prove global deterministic hypervisor correctness",
    "proves only the named workload",
)
SUPPORTED_ROW_RE = re.compile(r"^\| `(?P<workload>[^`]+)` \| `supported-bounded` \| `(?P<assertion>\d+)` \|")
EXPERIMENTAL_ROW_RE = re.compile(r"^\| (?P<surface>[^|`][^|]*?) \| `(?P<status>[^`]+)` \|")


class PromotionGateError(ValueError):
    pass


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text())
    except FileNotFoundError:
        raise PromotionGateError(f"missing file: {path.relative_to(ROOT)}") from None
    except json.JSONDecodeError as exc:
        raise PromotionGateError(f"invalid JSON in {path.relative_to(ROOT)}: {exc}") from exc
    if not isinstance(value, dict):
        raise PromotionGateError(f"{path.relative_to(ROOT)}: expected JSON object")
    return value


def load_report(path: Path) -> str:
    try:
        return path.read_text()
    except FileNotFoundError:
        raise PromotionGateError(f"missing file: {path.relative_to(ROOT)}") from None


def require(condition: bool, message: str) -> None:
    if not condition:
        raise PromotionGateError(message)


def manifest_proofs(manifest: dict[str, Any]) -> dict[str, int]:
    require(manifest.get("schema_version") == 1, "manifest schema_version must be 1")
    require(
        manifest.get("scope") == "bounded accepted snapshot-backed replay workload proofs",
        "manifest scope must remain bounded accepted snapshot-backed replay workload proofs",
    )
    require(
        manifest.get("required_replay_class") == "snapshot_backed_reproduced",
        "manifest required_replay_class must remain snapshot_backed_reproduced",
    )

    anti_claims = manifest.get("anti_claims")
    require(isinstance(anti_claims, list), "manifest anti_claims must be a list")
    anti_claim_text = "\n".join(str(item) for item in anti_claims)
    for fragment in REQUIRED_ANTI_CLAIM_FRAGMENTS:
        require(fragment in anti_claim_text, f"manifest anti_claims missing fragment: {fragment}")

    proofs = manifest.get("proofs")
    require(isinstance(proofs, list) and proofs, "manifest proofs must be a non-empty list")
    workloads: dict[str, int] = {}
    assertion_ids: set[int] = set()
    for index, proof in enumerate(proofs):
        require(isinstance(proof, dict), f"proof[{index}] must be an object")
        workload = proof.get("workload")
        require(isinstance(workload, str) and workload, f"proof[{index}].workload must be non-empty")
        require(workload not in workloads, f"duplicate workload proof: {workload}")
        assertion_id = proof.get("assertion_id")
        require(isinstance(assertion_id, int) and not isinstance(assertion_id, bool), f"{workload}: assertion_id must be an integer")
        require(assertion_id not in assertion_ids, f"duplicate assertion_id: {assertion_id}")
        assertion_ids.add(assertion_id)
        for field in ("evidence_dir", "summary", "bug", "verdict", "snapshot", "notes"):
            require(isinstance(proof.get(field), str) and proof[field], f"{workload}: proof.{field} must be non-empty")
        workloads[workload] = assertion_id
    return workloads


def report_supported_workloads(report: str) -> dict[str, int]:
    rows: dict[str, int] = {}
    for line in report.splitlines():
        match = SUPPORTED_ROW_RE.match(line)
        if not match:
            continue
        workload = match.group("workload")
        assertion_id = int(match.group("assertion"))
        require(workload not in rows, f"duplicate supported readiness row: {workload}")
        rows[workload] = assertion_id
    require(rows, "readiness report has no supported-bounded workload rows")
    return rows


def report_experimental_surfaces(report: str) -> dict[str, str]:
    surfaces: dict[str, str] = {}
    in_experimental = False
    for line in report.splitlines():
        if line == "## Experimental or unproven surfaces":
            in_experimental = True
            continue
        if in_experimental and line.startswith("## "):
            break
        if not in_experimental:
            continue
        match = EXPERIMENTAL_ROW_RE.match(line)
        if match:
            surfaces[match.group("surface").strip()] = match.group("status")
    return surfaces


def validate(manifest: dict[str, Any], report: str) -> list[str]:
    proofs = manifest_proofs(manifest)
    supported = report_supported_workloads(report)

    missing_from_report = sorted(set(proofs) - set(supported))
    unsupported_in_report = sorted(set(supported) - set(proofs))
    require(not missing_from_report, f"accepted manifest proofs missing from readiness report: {', '.join(missing_from_report)}")
    require(not unsupported_in_report, f"readiness report promotes workloads missing from manifest: {', '.join(unsupported_in_report)}")

    for workload, assertion_id in proofs.items():
        require(
            supported[workload] == assertion_id,
            f"{workload}: readiness report assertion {supported[workload]} does not match manifest {assertion_id}",
        )

    surfaces = report_experimental_surfaces(report)
    for surface, expected_status in REQUIRED_EXPERIMENTAL_SURFACES.items():
        actual_status = surfaces.get(surface)
        require(actual_status == expected_status, f"experimental surface {surface!r} status {actual_status!r}, expected {expected_status!r}")

    return [f"{workload}: assertion={proofs[workload]}" for workload in sorted(proofs)]


def run_selftest() -> int:
    manifest = load_json(MANIFEST)
    report = load_report(REPORT)
    validate(manifest, report)

    def expect_failure(name: str, candidate_manifest: dict[str, Any], candidate_report: str, needle: str) -> None:
        try:
            validate(candidate_manifest, candidate_report)
        except PromotionGateError as exc:
            if needle not in str(exc):
                raise AssertionError(f"{name}: expected {needle!r}, got {exc}") from exc
            return
        raise AssertionError(f"{name}: unexpectedly passed")

    missing_claim = json.loads(json.dumps(manifest))
    missing_claim["anti_claims"] = ["bounded only"]
    expect_failure("missing anti-claim", missing_claim, report, "anti_claims missing fragment")

    duplicate_assertion = json.loads(json.dumps(manifest))
    duplicate_assertion["proofs"][1]["assertion_id"] = duplicate_assertion["proofs"][0]["assertion_id"]
    expect_failure("duplicate assertion", duplicate_assertion, report, "duplicate assertion_id")

    missing_fresh_surface = report.replace("| Fresh workload authoring | `experimental` |", "| Fresh workload authoring | `supported-bounded` |")
    expect_failure("fresh workload overclaim", manifest, missing_fresh_surface, "Fresh workload authoring")

    report_only = report.replace("| `raft` | `supported-bounded` | `1806003755` |", "| `new-service` | `supported-bounded` | `12345` |\n| `raft` | `supported-bounded` | `1806003755` |", 1)
    expect_failure("report-only promotion", manifest, report_only, "missing from manifest")

    with tempfile.TemporaryDirectory() as _:
        # Keep tempfile imported/available for future fixture expansion while proving the selftest has no repo writes.
        pass

    print("readiness promotion gate selftest ok")
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
        lines = validate(load_json(args.manifest), load_report(args.report))
        print("readiness promotion gate ok:")
        for line in lines:
            print(f"  {line}")
        return 0
    except (PromotionGateError, AssertionError) as exc:
        print(f"readiness promotion gate failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
