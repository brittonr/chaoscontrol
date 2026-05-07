#!/usr/bin/env python3
"""Materialize a chunked snapshot artifact sidecar back to its raw snapshot file."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path, help="<snapshot>.chunks.json sidecar")
    parser.add_argument("--force", action="store_true", help="overwrite existing raw snapshot")
    args = parser.parse_args()

    manifest_path = args.manifest
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("schema_version") != 1:
        raise SystemExit(f"unsupported chunk manifest schema: {manifest_path}")
    original_path = manifest_path.parent / manifest["original_path"]
    if original_path.exists() and not args.force:
        raise SystemExit(f"raw snapshot already exists: {original_path} (use --force)")

    tmp_path = original_path.with_suffix(original_path.suffix + ".tmp")
    h = hashlib.sha256()
    total = 0
    with tmp_path.open("wb") as out:
        for entry in manifest["chunks"]:
            chunk_path = manifest_path.parent.parent / entry["path"]
            chunk_size = chunk_path.stat().st_size
            if chunk_size != entry["size"]:
                raise SystemExit(f"chunk size mismatch: {chunk_path}")
            chunk_sha = sha256(chunk_path)
            if chunk_sha != entry["sha256"]:
                raise SystemExit(f"chunk hash mismatch: {chunk_path}")
            with chunk_path.open("rb") as f:
                for data in iter(lambda: f.read(1024 * 1024), b""):
                    h.update(data)
                    out.write(data)
            total += chunk_size

    if total != manifest["original_size"]:
        tmp_path.unlink(missing_ok=True)
        raise SystemExit("aggregate size mismatch")
    actual = h.hexdigest()
    if actual != manifest["original_sha256"]:
        tmp_path.unlink(missing_ok=True)
        raise SystemExit("aggregate hash mismatch")
    tmp_path.replace(original_path)
    print(f"materialized {original_path} sha256:{actual} size={total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
