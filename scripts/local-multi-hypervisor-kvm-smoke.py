#!/usr/bin/env python3
"""Run a bounded KVM-backed local multi-hypervisor replay-readiness smoke rail."""

from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

DEFAULT_WORKLOADS = ("raft", "rust-workload")


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)


def require_kvm() -> None:
    kvm = Path("/dev/kvm")
    if not kvm.exists():
        raise SystemExit("local multi-hypervisor KVM smoke requires /dev/kvm")
    if not os.access(kvm, os.R_OK | os.W_OK):
        raise SystemExit("local multi-hypervisor KVM smoke requires read/write access to /dev/kvm")


def parse_workloads(raw: str) -> list[str]:
    workloads = [item.strip() for item in raw.split(",") if item.strip()]
    if len(workloads) < 2:
        raise SystemExit("--workloads must select at least two workloads for multi-hypervisor smoke")
    if len(set(workloads)) != len(workloads):
        raise SystemExit("--workloads must not contain duplicate workloads")
    return workloads


def build_plan(
    *,
    out: Path,
    replay_readiness: str,
    workloads: list[str],
    dogfood_extra: list[str],
) -> dict[str, object]:
    entries = []
    for idx, workload in enumerate(workloads, start=1):
        receipt = out / "run-receipts" / f"{idx:02d}-{workload}-replay-readiness.json"
        dogfood_out = out / "dogfood" / f"{idx:02d}-{workload}"
        command = [
            replay_readiness,
            "--receipt",
            str(receipt),
            "--dogfood",
            workload,
            "--",
            "--output",
            str(dogfood_out),
            *dogfood_extra,
        ]
        entries.append(
            {
                "queue_entry_id": f"kvm-mhq-{idx:04d}",
                "run_id": f"kvm-mh-run-{idx:04d}",
                "workload": workload,
                "command": " ".join(shlex.quote(part) for part in command),
                "receipt_path": str(receipt),
            }
        )
    return {
        "schema_version": 1,
        "campaign_id": "local-kvm-smoke-0001",
        "max_hypervisors": min(len(workloads), 2),
        "state_path": str(out / "campaign-state.json"),
        "artifact_index_path": str(out / "artifact-index.json"),
        "follow_up_policy": {"enabled": False, "reproduce": False, "minimize": False},
        "hypervisors": [
            {
                "hypervisor_worker_id": f"local-kvm-hv-{idx}",
                "node_id": f"local-kvm-node-{idx}",
                "resource_budget": {"vcpus": 2, "memory_mib": 1024},
                "artifact_root": str(out / "hypervisors" / f"local-kvm-hv-{idx}"),
            }
            for idx in range(1, len(workloads) + 1)
        ],
        "queue": {"entries": entries},
        "operator_decisions": [str(out / "operator-decision-receipt.json")],
    }


def write_summary(out: Path, summary: str, receipt_path: Path, plan_path: Path, rc: int, command_output: str) -> None:
    summary_path = out / "summary.txt"
    summary_path.write_text(
        "\n".join(
            [
                "local multi-hypervisor KVM smoke",
                f"status={'passed' if rc == 0 else 'failed'}",
                f"summary={summary.strip() if summary.strip() else '<none>'}",
                f"plan={plan_path}",
                f"receipt={receipt_path}",
                f"queue_state={out / 'campaign-state.json'}",
                "scope=bounded-local-kvm-multi-hypervisor-not-hosted-not-shared-remote-queue-not-cross-machine",
                "raw_log_scraping=false",
                "",
                "runner output:",
                command_output.strip(),
                "",
            ]
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=Path("dogfood-results/local-multi-hypervisor-kvm-smoke-latest"))
    parser.add_argument("--workloads", default=",".join(DEFAULT_WORKLOADS))
    parser.add_argument("--replay-readiness", default=os.environ.get("REPLAY_READINESS", "replay-readiness"))
    parser.add_argument(
        "--scheduler-receipt",
        default=os.environ.get("REPLAY_READINESS_SCHEDULER_RECEIPT", "replay-readiness-scheduler-receipt"),
    )
    parser.add_argument("dogfood_extra", nargs=argparse.REMAINDER, help="extra args passed after each dogfood --output")
    args = parser.parse_args()

    require_kvm()
    out = args.out.resolve()
    if out.exists() and any(out.iterdir()):
        raise SystemExit(f"output directory is not empty: {out}")
    out.mkdir(parents=True, exist_ok=True)
    (out / "run-receipts").mkdir()
    (out / "dogfood").mkdir()

    dogfood_extra = args.dogfood_extra
    if dogfood_extra and dogfood_extra[0] == "--":
        dogfood_extra = dogfood_extra[1:]

    workloads = parse_workloads(args.workloads)
    plan = build_plan(out=out, replay_readiness=args.replay_readiness, workloads=workloads, dogfood_extra=dogfood_extra)
    plan_path = out / "campaign-plan.json"
    receipt_path = out / "campaign-receipt.json"
    plan_path.write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")

    started = datetime.now(timezone.utc).isoformat()
    command = [args.scheduler_receipt, "--run-multi-hypervisor-plan", str(plan_path), "--output", str(receipt_path)]
    completed = run(command)
    summary = completed.stdout.strip().splitlines()[-1] if completed.stdout.strip() else ""
    write_summary(out, summary, receipt_path, plan_path, completed.returncode, completed.stdout)
    (out / "metadata.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "command": "local-multi-hypervisor-kvm-smoke",
                "started_at": started,
                "finished_at": datetime.now(timezone.utc).isoformat(),
                "exit_code": completed.returncode,
                "workloads": workloads,
                "artifacts": {
                    "plan": str(plan_path),
                    "receipt": str(receipt_path),
                    "queue_state": str(out / "campaign-state.json"),
                    "summary": str(out / "summary.txt"),
                },
                "scope": "bounded local KVM multi-hypervisor campaign smoke only; not a hosted service, not a shared remote queue, not cross-machine scheduling, not fleet-scale throughput",
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    sys.stdout.write(completed.stdout)
    if completed.returncode == 0:
        print(f"local multi-hypervisor KVM smoke artifacts: {out}")
    else:
        print(f"local multi-hypervisor KVM smoke failed; artifacts: {out}", file=sys.stderr)
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
