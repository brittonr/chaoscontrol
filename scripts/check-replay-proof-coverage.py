#!/usr/bin/env python3
"""Validate the aggregate accepted workload replay-proof manifest."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "dogfood-results" / "accepted-workload-proofs.json"
REQUIRED_CLASS = "snapshot_backed_reproduced"


def load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        raise AssertionError(f"missing file: {path.relative_to(ROOT)}") from None
    except json.JSONDecodeError as exc:
        raise AssertionError(f"invalid JSON in {path.relative_to(ROOT)}: {exc}") from exc


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def rel(path: Path) -> str:
    return str(path.relative_to(ROOT))


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def validate_proof(proof: dict[str, Any]) -> str:
    workload = proof.get("workload")
    evidence_dir = ROOT / proof["evidence_dir"]
    summary_path = evidence_dir / proof["summary"]
    bug_path = evidence_dir / proof["bug"]
    verdict_path = evidence_dir / proof["verdict"]
    snapshot_path = evidence_dir / proof["snapshot"]

    summary = load_json(summary_path)
    bug = load_json(bug_path)
    verdict = load_json(verdict_path)

    require(summary.get("accepted") is True, f"{workload}: summary is not accepted")
    require(summary.get("export_exit_status") == 0, f"{workload}: export-bugs did not exit 0")
    require(summary.get("reproduce_exit_status") == 0, f"{workload}: reproduce did not exit 0")

    assertion_id = proof["assertion_id"]
    require(bug.get("assertion_id") == assertion_id, f"{workload}: bug assertion mismatch")
    require(bug.get("replay_parent_depth", 0) > 0, f"{workload}: bug lacks replay parent depth")
    require(bool(bug.get("replay_parent_snapshot_ref")), f"{workload}: bug lacks snapshot ref")

    require(verdict.get("assertion_id") == assertion_id, f"{workload}: verdict assertion mismatch")
    require(verdict.get("replay_class") == REQUIRED_CLASS, f"{workload}: verdict class is {verdict.get('replay_class')!r}")
    require(verdict.get("reproduced") is True, f"{workload}: verdict did not reproduce")
    require(verdict.get("replay_parent_depth", 0) > 0, f"{workload}: verdict lacks replay parent depth")
    require(verdict.get("command", {}).get("exit_status") == 0, f"{workload}: verdict command did not exit 0")

    snapshot = verdict.get("snapshot") or {}
    reference = snapshot.get("reference") or {}
    require(snapshot.get("status") == "valid", f"{workload}: snapshot status is not valid")
    require(snapshot.get("present") is True, f"{workload}: snapshot not present")
    require(snapshot.get("digest_verified") is True, f"{workload}: snapshot digest not verified")
    require(reference.get("codec") == "simulation-snapshot-bincode-zstd-v1", f"{workload}: unexpected snapshot codec")
    require(reference.get("path") == proof["snapshot"], f"{workload}: manifest snapshot path disagrees with verdict ref")
    require(snapshot_path.exists(), f"{workload}: snapshot artifact missing: {rel(snapshot_path)}")

    digest = reference.get("digest", "")
    require(digest.startswith("sha256:"), f"{workload}: snapshot digest is not sha256")
    actual = sha256(snapshot_path)
    require(digest == f"sha256:{actual}", f"{workload}: snapshot digest mismatch")

    return f"{workload}: {REQUIRED_CLASS}, assertion={assertion_id}, depth={verdict['replay_parent_depth']}, snapshot=sha256:{actual}"


def main() -> int:
    manifest = load_json(MANIFEST)
    proofs = manifest.get("proofs", [])
    require(manifest.get("schema_version") == 1, "manifest schema_version must be 1")
    require(manifest.get("required_replay_class") == REQUIRED_CLASS, "manifest required_replay_class mismatch")
    require(len(proofs) >= 2, "manifest must contain at least two independent workload proofs")

    workloads: set[str] = set()
    lines: list[str] = []
    for proof in proofs:
        workload = proof.get("workload")
        require(isinstance(workload, str) and workload, "proof workload must be non-empty")
        require(workload not in workloads, f"duplicate workload proof: {workload}")
        workloads.add(workload)
        lines.append(validate_proof(proof))

    require({"raft", "redb"}.issubset(workloads), "manifest must include raft and redb proofs")
    print("replay proof coverage ok:")
    for line in lines:
        print(f"  {line}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as exc:
        print(f"replay proof coverage check failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
