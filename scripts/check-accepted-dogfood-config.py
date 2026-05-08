#!/usr/bin/env python3
"""Validate accepted-verdict dogfood wrapper smoke configuration.

This is a static readiness/CI guard: it compares the committed accepted proof
manifest and summaries against the Nix-generated dogfood wrapper configuration.
It does not run KVM. Its job is to fail closed when a wrapper drifts away from
known accepted proof parameters, before stale curated evidence hides the drift.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def load_json(path: Path) -> Any:
    return json.loads(path.read_text())


def workload_key(workload: str) -> str:
    return workload.replace("-", "_")


def summary_fail_after(summary: dict[str, Any], workload: str) -> int | None:
    generic = summary.get("snapshot_probe_fail_after")
    if generic is not None:
        return int(generic)
    specific = summary.get(f"{workload_key(workload)}_snapshot_probe_fail_after")
    if specific is not None:
        return int(specific)
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--config",
        type=Path,
        required=True,
        help="Nix-generated accepted-verdict dogfood wrapper config JSON",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("dogfood-results/accepted-workload-proofs.json"),
    )
    args = parser.parse_args()

    config = load_json(args.config)
    manifest = load_json(args.manifest)
    errors: list[str] = []

    proofs = manifest.get("proofs", [])
    proof_workloads = {proof.get("workload") for proof in proofs}
    config_workloads = set(config)
    missing_config = sorted(str(item) for item in proof_workloads - config_workloads)
    if missing_config:
        errors.append(f"missing wrapper config for accepted proof workloads: {', '.join(missing_config)}")

    extra_config = sorted(str(item) for item in config_workloads - proof_workloads)
    if extra_config:
        errors.append(f"wrapper config has no accepted proof manifest entry: {', '.join(extra_config)}")

    for proof in proofs:
        workload = proof.get("workload")
        if not isinstance(workload, str) or workload not in config:
            continue
        cfg = config[workload]
        expected_assertion = proof.get("assertion_id")
        if cfg.get("assertion_id") != expected_assertion:
            errors.append(
                f"{workload}: wrapper assertion_id {cfg.get('assertion_id')} != manifest {expected_assertion}"
            )

        fail_after_values = cfg.get("fail_after_values") or []
        if not fail_after_values:
            errors.append(f"{workload}: wrapper fail_after_values is empty")
            continue

        template = str(cfg.get("cmdline_template", ""))
        required_probe = f"{workload_key(workload)}_bug=snapshot_replay_probe"
        required_fail_after = f"{workload_key(workload)}_snapshot_probe_fail_after={{fail_after}}"
        if required_probe not in template or required_fail_after not in template:
            errors.append(
                f"{workload}: cmdline_template does not contain required probe/fail_after keys"
            )

        summary_path = args.manifest.parent.parent / proof["evidence_dir"] / proof["summary"]
        if not summary_path.is_file():
            errors.append(f"{workload}: missing accepted summary {summary_path}")
            continue
        summary = load_json(summary_path)
        if summary.get("accepted") is not True:
            errors.append(f"{workload}: summary is not accepted=true")

        observed_fail_after = summary_fail_after(summary, workload)
        if observed_fail_after is None:
            errors.append(f"{workload}: summary has no snapshot probe fail-after field")
            continue
        first_fail_after = int(fail_after_values[0])
        if observed_fail_after != first_fail_after:
            errors.append(
                f"{workload}: wrapper first fail_after {first_fail_after} != accepted summary {observed_fail_after}"
            )

    if errors:
        for error in errors:
            print(f"accepted-dogfood-config: {error}", file=sys.stderr)
        return 1

    print(f"accepted-dogfood-config: {len(proofs)} workloads match deterministic wrapper smoke config")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
