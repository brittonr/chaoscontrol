#!/usr/bin/env python3
"""Materialize contract-backed dogfood run-config.json and receipt.json.

This is a deterministic post-processor for explorer output directories. It keeps
raw runtime logs out of the acceptance boundary and records reported bug replay
attempts as explicit receipt statuses.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


def sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def load(path: Path) -> Any:
    return json.loads(path.read_text())


def materialize(output: Path, *, git_revision: str, replay_status: str, replay_message: str, replay_exit_status: int, replay_command: str | None) -> None:
    checkpoint = load(output / "checkpoint.json")
    config = checkpoint["config"]
    run_config = {
        "schema_version": "1",
        "profile": output.name,
        "mode": "hybrid" if config.get("schedule_diversity") is False else "fault-schedule",
        "num_vms": config["num_vms"],
        "kernel_path": config["kernel_path"],
        "initrd_path": config.get("initrd_path") or "none",
        "seed": config["seed"],
        "branch_factor": config["branch_factor"],
        "ticks_per_branch": config["ticks_per_branch"],
        "max_rounds": config["max_rounds"],
        "max_frontier": config["max_frontier"],
        "quantum": config["quantum"],
        "coverage_gpa": config["coverage_gpa"],
        "bootstrap_budget": config["bootstrap_budget"],
        "raw_log_policy": "debug-only-excluded-from-git",
    }
    (output / "run-config.json").write_text(json.dumps(run_config, indent=2) + "\n")

    artifacts = ["report.txt", "checkpoint.json", "assertions.json", "receipt.md", "run-config.json"]
    bugs = sorted(output.glob("bug_*.json"))
    artifacts.extend(path.name for path in bugs)
    assertion_details = load(output / "assertions.json")
    coverage = {
        "registered": len(assertion_details),
        "exercised": sum(1 for item in assertion_details if item.get("verdict") != "unexercised"),
        "passed": sum(1 for item in assertion_details if item.get("verdict") == "passed"),
        "failed": sum(1 for item in assertion_details if item.get("verdict") == "failed"),
        "unexercised": sum(1 for item in assertion_details if item.get("verdict") == "unexercised"),
        "summary_path": str(output / "assertions.json"),
    }

    bug_reports = []
    for bug_path in bugs:
        bug = load(bug_path)
        command = replay_command or (
            f"nix run .#explore -- reproduce --kernel {config['kernel_path']} "
            f"--initrd {config.get('initrd_path') or 'none'} --bug {bug_path} "
            f"--vms {config['num_vms']} --ticks {config['ticks_per_branch'] * 5}"
        )
        replay_parent_depth = bug.get("replay_parent_depth", 0)
        replay_context = "parent-snapshot-required" if replay_parent_depth > 0 else "schedule-only-replay"
        if replay_status == "known-gap":
            replay_context = f"{replay_context}-insufficient"
        item = {
            "path": str(bug_path),
            "assertion_id": bug["assertion_id"],
            "tick": bug["tick"],
            "replay_parent_depth": replay_parent_depth,
            "replay_context": replay_context,
            "replay_status": replay_status,
            "replay_attempt": {
                "command": command,
                "exit_status": replay_exit_status,
                "message": replay_message,
            },
        }
        if bug.get("replay_parent_snapshot_ref") is not None:
            item["replay_parent_snapshot_ref"] = bug["replay_parent_snapshot_ref"]
        bug_reports.append(item)

    receipt = {
        "schema_version": "1",
        "status": replay_status,
        "acceptance_status": replay_status,
        "git_revision": git_revision,
        "run_id": output.name,
        "command": f"nix run .#explore-raft -- --output {output}",
        "config": {"path": str(output / "run-config.json"), "digest": sha256(output / "run-config.json")},
        "kernel_path": config["kernel_path"],
        "initrd_path": config.get("initrd_path") or "none",
        "artifact_hashes": [{"path": str(output / name), "sha256": sha256(output / name)} for name in artifacts],
        "assertion_coverage": coverage,
        "bug_reports": bug_reports,
        "checkpoint_reference": {
            "path": str(output / "checkpoint.json"),
            "digest": sha256(output / "checkpoint.json"),
            "kernel_path": config["kernel_path"],
            "initrd_path": config.get("initrd_path") or "none",
            "seed": config["seed"],
        },
        "raw_logs": [
            {"path": str(output / "run.log"), "policy": "debug-only-excluded-from-git"},
            {"path": str(output / "reproduce.log"), "policy": "debug-only-excluded-from-git"},
        ],
    }
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--git-revision", required=True)
    parser.add_argument("--replay-status", default="known-gap", choices=["accepted", "partial", "known-gap", "invalid", "raw-log-only"])
    parser.add_argument("--replay-message", default="Bug NOT reproduced — assertion 1205943209 did not fail")
    parser.add_argument("--replay-exit-status", type=int, default=1)
    parser.add_argument("--replay-command")
    args = parser.parse_args()
    materialize(args.output, git_revision=args.git_revision, replay_status=args.replay_status, replay_message=args.replay_message, replay_exit_status=args.replay_exit_status, replay_command=args.replay_command)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
