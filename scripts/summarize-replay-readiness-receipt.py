#!/usr/bin/env python3
"""Print one CI/dashboard summary line from a replay-readiness receipt."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


class ReceiptError(ValueError):
    pass


def require_dict(value: Any, field: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ReceiptError(f"{field}: expected object")
    return value


def require_str(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value:
        raise ReceiptError(f"{field}: expected non-empty string")
    return value


def load_receipt(path: Path) -> dict[str, Any]:
    try:
        return require_dict(json.loads(path.read_text()), "receipt")
    except json.JSONDecodeError as exc:
        raise ReceiptError(f"invalid JSON: {exc}") from exc
    except OSError as exc:
        raise ReceiptError(str(exc)) from exc


def summarize(receipt: dict[str, Any]) -> str:
    command = require_str(receipt.get("command"), "receipt.command")
    if command != "replay-readiness":
        raise ReceiptError(f"receipt.command: expected replay-readiness, got {command!r}")

    status = require_str(receipt.get("status"), "receipt.status")
    if status not in {"passed", "failed"}:
        raise ReceiptError(f"receipt.status: unsupported value {status!r}")

    gates = receipt.get("static_gates")
    if not isinstance(gates, list) or not gates:
        raise ReceiptError("receipt.static_gates: expected non-empty list")
    passed_gates = 0
    failed_gates: list[str] = []
    for idx, gate in enumerate(gates):
        gate_obj = require_dict(gate, f"receipt.static_gates[{idx}]")
        name = require_str(gate_obj.get("name"), f"receipt.static_gates[{idx}].name")
        gate_status = require_str(gate_obj.get("status"), f"receipt.static_gates[{idx}].status")
        if gate_status == "pass":
            passed_gates += 1
        elif gate_status == "fail":
            failed_gates.append(name)
        elif gate_status not in {"pending", "running"}:
            raise ReceiptError(f"receipt.static_gates[{idx}].status: unsupported value {gate_status!r}")

    dogfood = require_dict(receipt.get("dogfood"), "receipt.dogfood")
    selected = dogfood.get("selected_workload")
    if selected is not None and not isinstance(selected, str):
        raise ReceiptError("receipt.dogfood.selected_workload: expected string or null")
    dogfood_status = require_str(dogfood.get("status"), "receipt.dogfood.status")
    if dogfood_status not in {"skipped", "pass", "fail", "running"}:
        raise ReceiptError(f"receipt.dogfood.status: unsupported value {dogfood_status!r}")

    failed_phase = receipt.get("failed_phase")
    if failed_phase is not None and not isinstance(failed_phase, str):
        raise ReceiptError("receipt.failed_phase: expected string or null")

    exit_code = receipt.get("exit_code")
    if not isinstance(exit_code, int):
        raise ReceiptError("receipt.exit_code: expected integer")

    scope = require_str(receipt.get("scope"), "receipt.scope")
    scope_token = "bounded" if "bounded" in scope and "not universal" in scope else "check-scope"
    dogfood_label = f"{selected}:{dogfood_status}" if selected else dogfood_status
    failed_label = failed_phase or "none"
    failed_gates_label = ",".join(failed_gates) if failed_gates else "none"

    return (
        f"replay-readiness status={status} exit={exit_code} "
        f"static_gates={passed_gates}/{len(gates)} failed_gates={failed_gates_label} "
        f"dogfood={dogfood_label} failed_phase={failed_label} scope={scope_token}"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", type=Path, help="path to replay-readiness receipt JSON")
    args = parser.parse_args()

    try:
        print(summarize(load_receipt(args.receipt)))
        return 0
    except ReceiptError as exc:
        print(f"replay-readiness summary failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
