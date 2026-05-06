#!/usr/bin/env python3
"""Validate the Nickel evidence-contract ownership registry.

This checker intentionally validates registry invariants only. Detailed artifact
contracts and fixture validation are follow-up tasks in the active OpenSpec.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "contracts" / "evidence" / "registry.ncl"
ALLOWED_OWNERSHIP = {"nickel-authored", "rust-derived", "excluded"}
REQUIRED_IDS = {
    "run-config",
    "dogfood-receipt",
    "bug-report",
    "assertion-summary",
    "checkpoint-reference",
    "snapshot-reference",
    "replay-verdict",
    "raw-runtime-logs",
    "secrets-and-crypto-internals",
}


def nickel_export_command() -> list[str]:
    if shutil.which("nickel"):
        return ["nickel", "export", str(REGISTRY)]
    if shutil.which("nix"):
        return ["nix", "run", "nixpkgs#nickel", "--", "export", str(REGISTRY)]
    print(
        "error: neither `nickel` nor `nix` is available for registry validation",
        file=sys.stderr,
    )
    raise SystemExit(127)


def load_registry() -> dict:
    proc = subprocess.run(
        nickel_export_command(),
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )

    if proc.returncode != 0:
        print(proc.stderr, file=sys.stderr, end="")
        raise SystemExit(proc.returncode)

    return json.loads(proc.stdout)


def require(condition: bool, message: str, errors: list[str]) -> None:
    if not condition:
        errors.append(message)


def non_empty_strings(value: object) -> bool:
    return isinstance(value, list) and all(isinstance(item, str) and item for item in value)


def main() -> int:
    registry = load_registry()
    errors: list[str] = []

    require(registry.get("schema_version") == "1", "schema_version must be '1'", errors)
    require(isinstance(registry.get("policy"), str) and registry["policy"], "policy must be non-empty", errors)

    families = registry.get("families")
    require(isinstance(families, list) and families, "families must be a non-empty list", errors)
    if not isinstance(families, list):
        families = []

    ids: set[str] = set()
    ownerships: set[str] = set()
    for index, entry in enumerate(families):
        prefix = f"families[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{prefix} must be an object")
            continue

        entry_id = entry.get("id")
        ownership = entry.get("ownership")
        if isinstance(entry_id, str):
            ids.add(entry_id)
        if isinstance(ownership, str):
            ownerships.add(ownership)

        require(isinstance(entry_id, str) and entry_id, f"{prefix}.id must be non-empty", errors)
        require(ownership in ALLOWED_OWNERSHIP, f"{prefix}.ownership must be one of {sorted(ALLOWED_OWNERSHIP)}", errors)
        require(isinstance(entry.get("owner"), str) and entry["owner"], f"{prefix}.owner must be non-empty", errors)
        require(non_empty_strings(entry.get("source_paths")), f"{prefix}.source_paths must be non-empty strings", errors)
        require(isinstance(entry.get("artifact_paths"), list), f"{prefix}.artifact_paths must be a list", errors)
        require(non_empty_strings(entry.get("validation_commands")), f"{prefix}.validation_commands must be non-empty strings", errors)
        require(non_empty_strings(entry.get("fixture_coverage")), f"{prefix}.fixture_coverage must be non-empty strings", errors)
        require(isinstance(entry.get("freshness"), str) and entry["freshness"], f"{prefix}.freshness must be non-empty", errors)
        require(isinstance(entry.get("rationale"), str) and entry["rationale"], f"{prefix}.rationale must be non-empty", errors)

        if ownership == "excluded":
            require(
                not entry.get("artifact_paths"),
                f"{prefix} is excluded and must not declare durable artifact_paths",
                errors,
            )

    missing = REQUIRED_IDS - ids
    require(not missing, f"missing required family ids: {sorted(missing)}", errors)
    require(
        ALLOWED_OWNERSHIP <= ownerships,
        f"registry must include all ownership classes; saw {sorted(ownerships)}",
        errors,
    )

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1

    print(f"contract registry ok: {len(families)} families, ownership={','.join(sorted(ownerships))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
