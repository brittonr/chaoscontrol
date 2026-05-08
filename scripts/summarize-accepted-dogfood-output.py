#!/usr/bin/env python3
"""Summarize an accepted-verdict dogfood output directory for operator receipts."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


class SummaryError(ValueError):
    pass


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text())
    except json.JSONDecodeError as exc:
        raise SummaryError(f"{path}: invalid JSON: {exc}") from exc
    except OSError as exc:
        raise SummaryError(f"{path}: {exc}") from exc
    if not isinstance(value, dict):
        raise SummaryError(f"{path}: expected JSON object")
    return value


def optional_int(value: Any, field: str) -> int | None:
    if value is None:
        return None
    if isinstance(value, int) and not isinstance(value, bool):
        return value
    raise SummaryError(f"{field}: expected integer or null")


def require_str(value: Any, field: str) -> str:
    if isinstance(value, str) and value and not any(ch.isspace() for ch in value):
        return value
    raise SummaryError(f"{field}: expected non-empty whitespace-free string")


def compact_attempt(attempt: dict[str, Any]) -> dict[str, Any]:
    verdict = attempt.get("verdict")
    bugs = attempt.get("bugs")
    bug_count = len(bugs) if isinstance(bugs, list) else None
    if bugs is not None and bug_count is None:
        raise SummaryError("attempt.bugs: expected list or null")
    verdict_summary = None
    if verdict is not None:
        if not isinstance(verdict, dict):
            raise SummaryError("attempt.verdict: expected object or null")
        reproduced = verdict.get("reproduced")
        if reproduced is not None and not isinstance(reproduced, bool):
            raise SummaryError("attempt.verdict.reproduced: expected boolean or null")
        snapshot_status = verdict.get("snapshot_status")
        if snapshot_status is not None:
            snapshot_status = require_str(snapshot_status, "attempt.verdict.snapshot_status")
        verdict_summary = {
            "replay_class": require_str(verdict.get("replay_class"), "attempt.verdict.replay_class"),
            "reproduced": reproduced,
            "replay_parent_depth": optional_int(verdict.get("replay_parent_depth"), "attempt.verdict.replay_parent_depth"),
            "snapshot_status": snapshot_status,
        }
    return {
        "workload": require_str(attempt.get("workload"), "attempt.workload"),
        "seed": optional_int(attempt.get("seed"), "attempt.seed"),
        "snapshot_probe_fail_after": optional_int(attempt.get("snapshot_probe_fail_after"), "attempt.snapshot_probe_fail_after"),
        "run_exit_status": optional_int(attempt.get("run_exit_status"), "attempt.run_exit_status"),
        "export_exit_status": optional_int(attempt.get("export_exit_status"), "attempt.export_exit_status"),
        "reproduce_exit_status": optional_int(attempt.get("reproduce_exit_status"), "attempt.reproduce_exit_status"),
        "bug_count": bug_count,
        "verdict": verdict_summary,
    }


def summarize_output(output: Path) -> dict[str, Any]:
    output = output.resolve()
    accepted_summary = output / "accepted-snapshot-verdict-summary.json"
    attempts_summary = output / "attempts-summary.json"
    if accepted_summary.is_file():
        summary = load_json(accepted_summary)
        if summary.get("accepted") is not True:
            raise SummaryError(f"{accepted_summary}: accepted must be true")
        result = compact_attempt(summary)
        result.update(
            {
                "accepted": summary.get("accepted") is True,
                "output": str(output),
                "accepted_bug": Path(str(summary.get("accepted_bug", ""))).name or None,
                "accepted_verdict": Path(str(summary.get("accepted_verdict", ""))).name or None,
            }
        )
        return result
    if attempts_summary.is_file():
        summary = load_json(attempts_summary)
        attempts = summary.get("attempts")
        if not isinstance(attempts, list):
            raise SummaryError(f"{attempts_summary}: attempts must be a list")
        if not attempts:
            raise SummaryError(f"{attempts_summary}: attempts must not be empty")
        last = attempts[-1]
        if not isinstance(last, dict):
            raise SummaryError(f"{attempts_summary}: last attempt must be an object")
        result = compact_attempt(last)
        result.update(
            {
                "accepted": False,
                "output": str(output),
                "attempts": len(attempts),
            }
        )
        return result
    raise SummaryError(f"{output}: missing accepted-snapshot-verdict-summary.json or attempts-summary.json")


def format_line(summary: dict[str, Any]) -> str:
    verdict = summary.get("verdict") if isinstance(summary.get("verdict"), dict) else {}
    accepted = "true" if summary.get("accepted") is True else "false"
    parts = [
        "dogfood-summary",
        f"workload={summary.get('workload') or 'unknown'}",
        f"accepted={accepted}",
        f"seed={summary.get('seed') if summary.get('seed') is not None else 'unknown'}",
        f"fail_after={summary.get('snapshot_probe_fail_after') if summary.get('snapshot_probe_fail_after') is not None else 'unknown'}",
        f"run={summary.get('run_exit_status') if summary.get('run_exit_status') is not None else 'unknown'}",
        f"export={summary.get('export_exit_status') if summary.get('export_exit_status') is not None else 'unknown'}",
        f"reproduce={summary.get('reproduce_exit_status') if summary.get('reproduce_exit_status') is not None else 'unknown'}",
        f"class={verdict.get('replay_class') or 'none'}",
        f"depth={verdict.get('replay_parent_depth') if verdict.get('replay_parent_depth') is not None else 'none'}",
        f"output={summary.get('output') or 'unknown'}",
    ]
    if summary.get("attempts") is not None:
        parts.insert(4, f"attempts={summary['attempts']}")
    return " ".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="accepted-verdict dogfood output directory")
    parser.add_argument("--json", action="store_true", help="print compact JSON instead of one-line text")
    args = parser.parse_args()

    try:
        summary = summarize_output(args.output)
    except SummaryError as exc:
        print(f"dogfood summary failed: {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(json.dumps(summary, indent=2, sort_keys=True))
    else:
        print(format_line(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
