#!/usr/bin/env python3
"""Materialize a chunked snapshot artifact sidecar back to its raw snapshot file."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path
from typing import Any


class MaterializeError(ValueError):
    """Operator-facing chunk materialization error."""


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise MaterializeError(message)


def load_manifest(manifest_path: Path) -> dict[str, Any]:
    try:
        manifest = json.loads(manifest_path.read_text())
    except FileNotFoundError:
        raise MaterializeError(f"chunk manifest missing: {manifest_path}") from None
    except json.JSONDecodeError as exc:
        raise MaterializeError(f"chunk manifest is not valid JSON: {manifest_path}: {exc}") from exc
    require(isinstance(manifest, dict), f"chunk manifest must be a JSON object: {manifest_path}")
    return manifest


def materialize(manifest_path: Path, *, force: bool = False) -> str:
    manifest = load_manifest(manifest_path)
    require(manifest.get("schema_version") == 1, f"unsupported chunk manifest schema: {manifest_path}")

    original_name = manifest.get("original_path")
    require(isinstance(original_name, str) and original_name.endswith(".snapshot.bin"), f"chunk manifest original_path invalid: {manifest_path}")
    require("/" not in original_name and ".." not in original_name, f"chunk manifest original_path must be a local snapshot filename: {manifest_path}")
    original_path = manifest_path.parent / original_name
    if original_path.exists() and not force:
        raise MaterializeError(f"raw snapshot already exists: {original_path} (use --force)")

    expected_size = manifest.get("original_size")
    require(isinstance(expected_size, int) and expected_size > 0, f"chunk manifest original_size invalid: {manifest_path}")
    expected_sha = manifest.get("original_sha256")
    require(isinstance(expected_sha, str) and len(expected_sha) == 64, f"chunk manifest original_sha256 invalid: {manifest_path}")
    chunks = manifest.get("chunks")
    require(isinstance(chunks, list) and chunks, f"chunk manifest has no chunks: {manifest_path}")

    tmp_path = original_path.with_suffix(original_path.suffix + ".tmp")
    h = hashlib.sha256()
    total = 0
    try:
        with tmp_path.open("wb") as out:
            for idx, entry in enumerate(chunks):
                require(isinstance(entry, dict), f"chunk entry {idx} invalid: {manifest_path}")
                chunk_ref = entry.get("path")
                require(isinstance(chunk_ref, str) and chunk_ref.startswith("snapshots/"), f"chunk {idx} path invalid: {manifest_path}")
                require(".." not in Path(chunk_ref).parts, f"chunk {idx} path escapes snapshot directory: {manifest_path}")
                chunk_path = manifest_path.parent.parent / chunk_ref
                if not chunk_path.exists():
                    raise MaterializeError(f"snapshot chunk missing: {chunk_path}")
                chunk_size = chunk_path.stat().st_size
                expected_chunk_size = entry.get("size")
                if chunk_size != expected_chunk_size:
                    raise MaterializeError(
                        f"snapshot chunk size mismatch: {chunk_path} expected={expected_chunk_size} actual={chunk_size}"
                    )
                chunk_sha = sha256(chunk_path)
                expected_chunk_sha = entry.get("sha256")
                if chunk_sha != expected_chunk_sha:
                    raise MaterializeError(
                        f"snapshot chunk hash mismatch: {chunk_path} expected={expected_chunk_sha} actual={chunk_sha}"
                    )
                with chunk_path.open("rb") as f:
                    for data in iter(lambda: f.read(1024 * 1024), b""):
                        h.update(data)
                        out.write(data)
                total += chunk_size

        if total != expected_size:
            raise MaterializeError(f"aggregate size mismatch: {manifest_path} expected={expected_size} actual={total}")
        actual = h.hexdigest()
        if actual != expected_sha:
            raise MaterializeError(f"aggregate hash mismatch: {manifest_path} expected={expected_sha} actual={actual}")
        tmp_path.replace(original_path)
    except Exception:
        tmp_path.unlink(missing_ok=True)
        raise
    return f"materialized {original_path} sha256:{actual} size={total}"


def write_test_fixture(root: Path) -> Path:
    snapshots = root / "snapshots"
    snapshots.mkdir()
    parts = [b"alpha", b"-beta", b"-gamma"]
    original = b"".join(parts)
    digest = hashlib.sha256(original).hexdigest()
    chunks: list[dict[str, Any]] = []
    for idx, data in enumerate(parts):
        path = snapshots / f"{digest}.snapshot.bin.part{idx:02d}"
        path.write_bytes(data)
        chunks.append(
            {
                "path": f"snapshots/{path.name}",
                "size": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        )
    manifest = {
        "schema_version": 1,
        "original_path": f"{digest}.snapshot.bin",
        "original_size": len(original),
        "original_sha256": digest,
        "chunks": chunks,
    }
    manifest_path = snapshots / f"{digest}.snapshot.bin.chunks.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True))
    return manifest_path


def expect_error(label: str, manifest_path: Path, needle: str) -> None:
    try:
        materialize(manifest_path, force=True)
    except MaterializeError as exc:
        message = str(exc)
        if needle not in message:
            raise AssertionError(f"{label}: expected {needle!r} in {message!r}") from exc
        return
    raise AssertionError(f"{label}: materialization unexpectedly succeeded")


def run_selftest() -> int:
    with tempfile.TemporaryDirectory() as tmp_raw:
        root = Path(tmp_raw)
        manifest_path = write_test_fixture(root)
        message = materialize(manifest_path)
        if "materialized" not in message or "sha256:" not in message:
            raise AssertionError(f"positive materialization message malformed: {message}")

    with tempfile.TemporaryDirectory() as tmp_raw:
        root = Path(tmp_raw)
        manifest_path = write_test_fixture(root)
        manifest = load_manifest(manifest_path)
        missing = root / manifest["chunks"][1]["path"]
        missing.unlink()
        expect_error("missing chunk", manifest_path, "snapshot chunk missing")
        expected_tmp = manifest_path.with_name(manifest["original_path"] + ".tmp")
        if expected_tmp.exists():
            raise AssertionError("missing chunk left a partial .tmp snapshot")

    with tempfile.TemporaryDirectory() as tmp_raw:
        root = Path(tmp_raw)
        manifest_path = write_test_fixture(root)
        manifest = load_manifest(manifest_path)
        manifest["chunks"] = [manifest["chunks"][1], manifest["chunks"][0], manifest["chunks"][2]]
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True))
        expect_error("reordered chunks", manifest_path, "aggregate hash mismatch")

    with tempfile.TemporaryDirectory() as tmp_raw:
        root = Path(tmp_raw)
        manifest_path = write_test_fixture(root)
        manifest = load_manifest(manifest_path)
        corrupt = root / manifest["chunks"][0]["path"]
        corrupt.write_bytes(b"ALPHA")
        expect_error("corrupt chunk", manifest_path, "snapshot chunk hash mismatch")

    print("materialize-snapshot-chunks selftest ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", nargs="?", type=Path, help="<snapshot>.chunks.json sidecar")
    parser.add_argument("--force", action="store_true", help="overwrite existing raw snapshot")
    parser.add_argument("--selftest", action="store_true", help="run deterministic positive and negative tests")
    args = parser.parse_args()

    if args.selftest:
        return run_selftest()
    if args.manifest is None:
        parser.error("manifest is required unless --selftest is used")
    try:
        print(materialize(args.manifest, force=args.force))
        return 0
    except MaterializeError as exc:
        print(f"snapshot chunk materialization failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
