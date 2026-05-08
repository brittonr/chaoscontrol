#!/usr/bin/env python3
"""Validate accepted-verdict dogfood wrapper smoke configuration.

This is a static readiness/CI guard: it compares the committed accepted proof
manifest, the dogfood expectation lockfile, and the Nix-generated dogfood
wrapper configuration. It does not run KVM. Its job is to fail closed when a
wrapper drifts away from the locked live proof expectation, before stale curated
evidence hides the drift.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_REPLAY_CLASS = "snapshot_backed_reproduced"


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


def int_list(value: Any, field: str, errors: list[str]) -> list[int]:
    if not isinstance(value, list) or not value:
        errors.append(f"{field}: expected non-empty integer list")
        return []
    result: list[int] = []
    for item in value:
        if not isinstance(item, int) or isinstance(item, bool):
            errors.append(f"{field}: expected integer list, got {item!r}")
            return []
        result.append(item)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--config",
        type=Path,
        required=True,
        help="Nix-generated accepted-verdict dogfood wrapper config JSON",
    )
    parser.add_argument(
        "--expectations",
        type=Path,
        default=Path("dogfood-results/accepted-dogfood-expectations.json"),
        help="Committed dogfood expectation lockfile",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("dogfood-results/accepted-workload-proofs.json"),
    )
    args = parser.parse_args()

    config = load_json(args.config)
    expectations_root = load_json(args.expectations)
    expectations = expectations_root.get("workloads")
    manifest = load_json(args.manifest)
    errors: list[str] = []

    if not isinstance(expectations, dict):
        errors.append("expectations: missing workloads object")
        expectations = {}

    proofs = manifest.get("proofs", [])
    proof_workloads = {proof.get("workload") for proof in proofs}
    config_workloads = set(config)
    expectation_workloads = set(expectations)

    missing_config = sorted(str(item) for item in proof_workloads - config_workloads)
    if missing_config:
        errors.append(f"missing wrapper config for accepted proof workloads: {', '.join(missing_config)}")

    extra_config = sorted(str(item) for item in config_workloads - proof_workloads)
    if extra_config:
        errors.append(f"wrapper config has no accepted proof manifest entry: {', '.join(extra_config)}")

    missing_expectations = sorted(str(item) for item in config_workloads - expectation_workloads)
    if missing_expectations:
        errors.append(f"wrapper config has no expectation lock entry: {', '.join(missing_expectations)}")

    extra_expectations = sorted(str(item) for item in expectation_workloads - config_workloads)
    if extra_expectations:
        errors.append(f"expectation lock has no wrapper config entry: {', '.join(extra_expectations)}")

    for workload in sorted(config_workloads & expectation_workloads):
        cfg = config[workload]
        exp = expectations[workload]
        if not isinstance(cfg, dict) or not isinstance(exp, dict):
            errors.append(f"{workload}: config and expectation must be objects")
            continue

        expected_assertion = exp.get("assertion_id")
        if cfg.get("assertion_id") != expected_assertion:
            errors.append(
                f"{workload}: wrapper assertion_id {cfg.get('assertion_id')} != expectation {expected_assertion}"
            )

        if cfg.get("expectation") != exp:
            errors.append(f"{workload}: Nix-generated embedded expectation differs from lockfile")

        runner = exp.get("runner") or {}
        if not isinstance(runner, dict):
            errors.append(f"{workload}: expectation runner must be an object")
            runner = {}
        expected_fail_after_values = int_list(
            runner.get("fail_after_values"), f"{workload}: expectation runner.fail_after_values", errors
        )
        cfg_fail_after_values = int_list(
            cfg.get("fail_after_values"), f"{workload}: wrapper fail_after_values", errors
        )
        if expected_fail_after_values and cfg_fail_after_values and cfg_fail_after_values != expected_fail_after_values:
            errors.append(
                f"{workload}: wrapper fail_after_values {cfg_fail_after_values} != expectation {expected_fail_after_values}"
            )

        expected_max_attempts = runner.get("max_attempts")
        if expected_max_attempts is not None and cfg.get("max_attempts") != expected_max_attempts:
            errors.append(
                f"{workload}: wrapper max_attempts {cfg.get('max_attempts')} != expectation {expected_max_attempts}"
            )

        template = str(cfg.get("cmdline_template", ""))
        required_probe = f"{exp.get('probe_key')}=snapshot_replay_probe"
        required_fail_after = f"{exp.get('fail_after_key')}={{fail_after}}"
        if required_probe not in template or required_fail_after not in template:
            errors.append(
                f"{workload}: cmdline_template does not contain locked probe/fail_after keys"
            )

        expected = exp.get("expected") or {}
        if not isinstance(expected, dict):
            errors.append(f"{workload}: expectation expected must be an object")
            expected = {}
        if expected.get("accepted") is not True:
            errors.append(f"{workload}: expectation expected.accepted must be true")
        if expected.get("replay_class") != REQUIRED_REPLAY_CLASS:
            errors.append(
                f"{workload}: expectation replay_class {expected.get('replay_class')} != {REQUIRED_REPLAY_CLASS}"
            )
        expected_values = int_list(
            expected.get("fail_after_values"), f"{workload}: expectation expected.fail_after_values", errors
        )
        if expected_values and expected_fail_after_values and expected_values != expected_fail_after_values:
            errors.append(
                f"{workload}: expected fail_after_values {expected_values} != runner fail_after_values {expected_fail_after_values}"
            )

    for proof in proofs:
        workload = proof.get("workload")
        if not isinstance(workload, str) or workload not in config:
            continue
        cfg = config[workload]
        exp = expectations.get(workload, {}) if isinstance(expectations, dict) else {}
        expected_assertion = proof.get("assertion_id")
        if cfg.get("assertion_id") != expected_assertion:
            errors.append(
                f"{workload}: wrapper assertion_id {cfg.get('assertion_id')} != manifest {expected_assertion}"
            )
        if exp and exp.get("assertion_id") != expected_assertion:
            errors.append(
                f"{workload}: expectation assertion_id {exp.get('assertion_id')} != manifest {expected_assertion}"
            )

        summary_path = args.manifest.parent.parent / proof["evidence_dir"] / proof["summary"]
        if not summary_path.is_file():
            errors.append(f"{workload}: missing accepted summary {summary_path}")
            continue
        summary = load_json(summary_path)
        if summary.get("accepted") is not True:
            errors.append(f"{workload}: summary is not accepted=true")
        verdict = summary.get("verdict") or {}
        if verdict.get("replay_class") != REQUIRED_REPLAY_CLASS:
            errors.append(
                f"{workload}: summary replay_class {verdict.get('replay_class')} != {REQUIRED_REPLAY_CLASS}"
            )
        min_depth = ((exp.get("expected") or {}).get("min_replay_parent_depth") if exp else 1) or 1
        depth = verdict.get("replay_parent_depth")
        if not isinstance(depth, int) or depth < min_depth:
            errors.append(f"{workload}: summary replay_parent_depth {depth} < expected {min_depth}")

        observed_fail_after = summary_fail_after(summary, workload)
        if observed_fail_after is None:
            errors.append(f"{workload}: summary has no snapshot probe fail-after field")

    if errors:
        for error in errors:
            print(f"accepted-dogfood-config: {error}", file=sys.stderr)
        return 1

    print(
        "accepted-dogfood-config: "
        f"{len(proofs)} workloads match expectation lockfile and deterministic wrapper config"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
