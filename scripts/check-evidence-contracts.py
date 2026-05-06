#!/usr/bin/env python3
"""Validate Nickel evidence-contract fixtures and committed dogfood receipt data."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = ROOT / "contracts" / "evidence"
DOGFOOD = ROOT / "dogfood-results" / "raft-20260506-095025"
STATUSES = {"accepted", "partial", "known-gap", "invalid", "raw-log-only"}


class ContractError(Exception):
    pass


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text())
    except Exception as exc:  # pragma: no cover - diagnostic path
        raise ContractError(f"{path}: invalid JSON: {exc}") from exc


def require(cond: bool, message: str) -> None:
    if not cond:
        raise ContractError(message)


def require_str(value: Any, label: str) -> None:
    require(isinstance(value, str) and len(value) > 0, f"{label}: expected non-empty string")


def require_num(value: Any, label: str) -> None:
    require(isinstance(value, (int, float)) and not isinstance(value, bool), f"{label}: expected number")


def require_pos_int(value: Any, label: str) -> None:
    require(isinstance(value, int) and value > 0 and not isinstance(value, bool), f"{label}: expected positive integer")


def require_status(value: Any, label: str) -> None:
    require(value in STATUSES, f"{label}: expected one of {sorted(STATUSES)}")


def sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def nickel_command() -> list[str] | None:
    if shutil.which("nickel"):
        return ["nickel", "export"]
    if shutil.which("nix"):
        return ["nix", "run", "nixpkgs#nickel", "--", "export"]
    return None


def run_nickel_examples() -> None:
    command = nickel_command()
    require(command is not None, "neither nickel nor nix is available for Nickel export checks")
    examples = [
        "examples/raft-run-config.ncl",
        "examples/raft-dogfood-receipt.ncl",
        "examples/raft-bug-report.ncl",
        "examples/raft-assertion-summary.ncl",
    ]
    for rel in examples:
        subprocess.run(command + [str(CONTRACTS / rel)], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)


def validate_run_config(value: Any) -> None:
    require(isinstance(value, dict), "run-config: expected object")
    for key in ["schema_version", "profile", "mode", "kernel_path", "initrd_path", "raw_log_policy"]:
        require_str(value.get(key), f"run-config.{key}")
    for key in ["num_vms", "branch_factor", "ticks_per_branch", "max_rounds", "max_frontier", "quantum", "bootstrap_budget"]:
        require_pos_int(value.get(key), f"run-config.{key}")
    for key in ["seed", "coverage_gpa"]:
        require_num(value.get(key), f"run-config.{key}")


def validate_artifact_hash(value: Any) -> None:
    require(isinstance(value, dict), "artifact-hash: expected object")
    require_str(value.get("path"), "artifact-hash.path")
    digest = value.get("sha256")
    require(isinstance(digest, str) and digest.startswith("sha256:") and len(digest) == 71, "artifact-hash.sha256: expected sha256:<64 hex>")
    int(digest.removeprefix("sha256:"), 16)


def validate_bug_report(value: Any) -> None:
    require(isinstance(value, dict), "bug-report: expected object")
    for key in ["bug_id", "assertion_id", "tick", "dedup_key"]:
        require_num(value.get(key), f"bug-report.{key}")
    require_str(value.get("assertion_location"), "bug-report.assertion_location")
    schedule = value.get("schedule")
    require(isinstance(schedule, dict), "bug-report.schedule: expected object")
    require(isinstance(schedule.get("faults"), list), "bug-report.schedule.faults: expected list")
    require(len(schedule["faults"]) > 0, "bug-report.schedule.faults: expected at least one fault")


def validate_assertion_summary(value: Any) -> None:
    require(isinstance(value, list) and value, "assertion-summary: expected non-empty array")
    for idx, item in enumerate(value):
        require(isinstance(item, dict), f"assertion-summary[{idx}]: expected object")
        for key in ["id", "hit_count", "true_count", "false_count"]:
            require_num(item.get(key), f"assertion-summary[{idx}].{key}")
        for key in ["message", "kind", "guest", "category"]:
            require_str(item.get(key), f"assertion-summary[{idx}].{key}")
        require(item.get("verdict") in {"passed", "failed", "unexercised"}, f"assertion-summary[{idx}].verdict: invalid")


def validate_checkpoint_reference(value: Any) -> None:
    require(isinstance(value, dict), "checkpoint-reference: expected object")
    for key in ["path", "digest", "kernel_path", "initrd_path"]:
        require_str(value.get(key), f"checkpoint-reference.{key}")
    require_num(value.get("seed"), "checkpoint-reference.seed")


def validate_receipt(value: Any, *, check_files: bool = False) -> None:
    require(isinstance(value, dict), "receipt: expected object")
    for key in ["schema_version", "git_revision", "run_id", "command", "kernel_path", "initrd_path"]:
        require_str(value.get(key), f"receipt.{key}")
    require_status(value.get("status"), "receipt.status")
    require_status(value.get("acceptance_status"), "receipt.acceptance_status")
    require(isinstance(value.get("artifact_hashes"), list) and value["artifact_hashes"], "receipt.artifact_hashes: expected non-empty list")
    for artifact in value["artifact_hashes"]:
        validate_artifact_hash(artifact)
        if check_files and not artifact["path"].endswith(("run.log", "reproduce.log")):
            path = ROOT / artifact["path"]
            require(path.exists(), f"receipt artifact missing: {artifact['path']}")
            require(sha256(path) == artifact["sha256"], f"receipt artifact hash mismatch: {artifact['path']}")
    coverage = value.get("assertion_coverage")
    require(isinstance(coverage, dict), "receipt.assertion_coverage: expected object")
    for key in ["registered", "exercised", "passed", "failed", "unexercised"]:
        require_num(coverage.get(key), f"receipt.assertion_coverage.{key}")
    require(coverage["registered"] == coverage["passed"] + coverage["failed"] + coverage["unexercised"], "receipt.assertion_coverage counts do not add up")
    require(isinstance(value.get("bug_reports"), list), "receipt.bug_reports: expected list")
    for idx, bug in enumerate(value["bug_reports"]):
        require_str(bug.get("path"), f"receipt.bug_reports[{idx}].path")
        require_num(bug.get("assertion_id"), f"receipt.bug_reports[{idx}].assertion_id")
        require_num(bug.get("tick"), f"receipt.bug_reports[{idx}].tick")
        require_status(bug.get("replay_status"), f"receipt.bug_reports[{idx}].replay_status")
        replay = bug.get("replay_attempt")
        require(isinstance(replay, dict), f"receipt.bug_reports[{idx}].replay_attempt: expected object")
        require_str(replay.get("command"), f"receipt.bug_reports[{idx}].replay_attempt.command")
        require_num(replay.get("exit_status"), f"receipt.bug_reports[{idx}].replay_attempt.exit_status")
        require_str(replay.get("message"), f"receipt.bug_reports[{idx}].replay_attempt.message")
    validate_checkpoint_reference(value.get("checkpoint_reference"))
    raw_logs = value.get("raw_logs")
    require(isinstance(raw_logs, list), "receipt.raw_logs: expected list")
    for log in raw_logs:
        require(log.get("policy") == "debug-only-excluded-from-git", "raw log policy must keep logs debug-only/excluded")


def validate_markdown_receipt(data: dict[str, Any]) -> None:
    md = (DOGFOOD / "receipt.md").read_text()
    require(data["run_id"] in str(DOGFOOD), "receipt.md path does not match receipt run_id context")
    require(str(data["assertion_coverage"]["registered"]) in md, "receipt.md missing assertion count")
    for bug in data["bug_reports"]:
        require(str(bug["assertion_id"]) in md, "receipt.md missing bug assertion id")
        require(bug["replay_attempt"]["message"] in md, "receipt.md missing replay outcome")


def expect_invalid(path: Path, validator: Callable[[Any], None]) -> None:
    try:
        validator(load_json(path))
    except (ContractError, ValueError):
        return
    raise ContractError(f"negative fixture unexpectedly passed: {path}")


def main() -> int:
    run_nickel_examples()

    validate_run_config(load_json(DOGFOOD / "run-config.json"))
    validate_bug_report(load_json(DOGFOOD / "bug_0.json"))
    validate_assertion_summary(load_json(DOGFOOD / "assertions.json"))
    receipt = load_json(DOGFOOD / "receipt.json")
    validate_receipt(receipt, check_files=True)
    validate_markdown_receipt(receipt)

    valid = CONTRACTS / "fixtures" / "valid"
    validate_run_config(load_json(valid / "run-config.valid.json"))
    validate_receipt(load_json(valid / "receipt.known-gap.valid.json"))
    validate_bug_report(load_json(valid / "bug-report.valid.json"))
    validate_assertion_summary(load_json(valid / "assertions.valid.json"))

    invalid = CONTRACTS / "fixtures" / "invalid"
    expect_invalid(invalid / "run-config.zero-vms.invalid.json", validate_run_config)
    expect_invalid(invalid / "receipt.missing-hash.invalid.json", validate_receipt)
    expect_invalid(invalid / "receipt.missing-replay-attempt.invalid.json", validate_receipt)
    expect_invalid(invalid / "assertions.bad-verdict.invalid.json", validate_assertion_summary)
    expect_invalid(invalid / "bug-report.missing-schedule.invalid.json", validate_bug_report)
    expect_invalid(invalid / "receipt.missing-deterministic-context.invalid.json", validate_receipt)
    expect_invalid(invalid / "receipt.stale-artifact.invalid.json", lambda value: validate_receipt(value, check_files=True))

    print("evidence contracts ok: nickel examples, dogfood receipt, positive fixtures, negative fixtures")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ContractError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
