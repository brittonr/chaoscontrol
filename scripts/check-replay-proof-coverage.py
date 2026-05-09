#!/usr/bin/env python3
"""Validate the aggregate accepted workload replay-proof manifest."""

from __future__ import annotations

SUPPORTED_SNAPSHOT_CODECS = {"simulation-snapshot-cbor-zstd-v2", "simulation-snapshot-bincode-zstd-v1"}
SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS = {1, 2}

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


def snapshot_artifact_sha256(snapshot_path: Path) -> tuple[str, str]:
    """Return logical snapshot SHA-256 and storage mode.

    Accepted evidence may either commit the raw `.snapshot.bin` or a sidecar
    `<snapshot>.chunks.json` plus ordered chunk files. The logical snapshot path
    remains the one embedded in Rust-owned bug/verdict refs.
    """
    if snapshot_path.exists():
        return sha256(snapshot_path), "raw"

    manifest_path = snapshot_path.with_name(snapshot_path.name + ".chunks.json")
    manifest = load_json(manifest_path)
    require(manifest.get("schema_version") == 1, f"chunk manifest schema_version invalid: {rel(manifest_path)}")
    require(manifest.get("original_path") == snapshot_path.name, f"chunk manifest original_path mismatch: {rel(manifest_path)}")
    expected_size = manifest.get("original_size")
    require(isinstance(expected_size, int) and expected_size > 0, f"chunk manifest original_size invalid: {rel(manifest_path)}")
    expected_sha = manifest.get("original_sha256")
    require(isinstance(expected_sha, str) and len(expected_sha) == 64, f"chunk manifest original_sha256 invalid: {rel(manifest_path)}")
    chunks = manifest.get("chunks")
    require(isinstance(chunks, list) and chunks, f"chunk manifest has no chunks: {rel(manifest_path)}")

    aggregate = hashlib.sha256()
    total_size = 0
    for idx, chunk in enumerate(chunks):
        require(isinstance(chunk, dict), f"chunk entry {idx} invalid: {rel(manifest_path)}")
        chunk_path_value = chunk.get("path")
        require(isinstance(chunk_path_value, str) and chunk_path_value.startswith("snapshots/"), f"chunk {idx} path invalid: {rel(manifest_path)}")
        chunk_path = snapshot_path.parent.parent / chunk_path_value
        require(chunk_path.exists(), f"snapshot chunk missing: {rel(chunk_path)}")
        chunk_size = chunk_path.stat().st_size
        require(chunk.get("size") == chunk_size, f"snapshot chunk size mismatch: {rel(chunk_path)}")
        chunk_sha = sha256(chunk_path)
        require(chunk.get("sha256") == chunk_sha, f"snapshot chunk hash mismatch: {rel(chunk_path)}")
        with chunk_path.open("rb") as f:
            for data in iter(lambda: f.read(1024 * 1024), b""):
                aggregate.update(data)
        total_size += chunk_size

    actual = aggregate.hexdigest()
    require(total_size == expected_size, f"chunk manifest aggregate size mismatch: {rel(manifest_path)}")
    require(actual == expected_sha, f"chunk manifest aggregate hash mismatch: {rel(manifest_path)}")
    return actual, "chunks"


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
    require(reference.get("codec") in SUPPORTED_SNAPSHOT_CODECS, f"{workload}: unexpected snapshot codec")
    require(reference.get("path") == proof["snapshot"], f"{workload}: manifest snapshot path disagrees with verdict ref")

    digest = reference.get("digest", "")
    require(digest.startswith("sha256:"), f"{workload}: snapshot digest is not sha256")
    actual, storage = snapshot_artifact_sha256(snapshot_path)
    require(digest == f"sha256:{actual}", f"{workload}: snapshot digest mismatch")

    return f"{workload}: {REQUIRED_CLASS}, assertion={assertion_id}, depth={verdict['replay_parent_depth']}, snapshot=sha256:{actual} ({storage})"


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
