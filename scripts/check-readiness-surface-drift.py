#!/usr/bin/env python3
"""Check replay-readiness generated operator surfaces for drift."""

from __future__ import annotations

import argparse
import importlib.util
import json
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FLAKE = ROOT / "flake.nix"
SUMMARY_SCRIPT = ROOT / "scripts" / "summarize-replay-readiness-receipt.py"
DASHBOARD_SCRIPT = ROOT / "scripts" / "render-replay-readiness-dashboard.py"
README_SCRIPT = ROOT / "scripts" / "update-replay-readiness-readme-status.py"

RUN_GATE_RE = re.compile(r"^\s*run_gate\s+(?P<name>\S+)\s+")
RECEIPT_GATE_RE = re.compile(r'^\s*\("(?P<name>[^"]+)",\s+"(?P<command>[^"]+)",')


class SurfaceDriftError(ValueError):
    pass


def load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise SurfaceDriftError(f"failed to load {path.relative_to(ROOT)}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SurfaceDriftError(message)


def bounded(text: str) -> bool:
    lowered = text.lower()
    return "bounded" in lowered and ("not universal" in lowered or "not a claim of universal" in lowered)


def between(text: str, start_marker: str, end_marker: str) -> str:
    start = text.find(start_marker)
    require(start != -1, f"missing start marker: {start_marker}")
    end = text.find(end_marker, start)
    require(end != -1, f"missing end marker: {end_marker}")
    return text[start:end]


def executed_static_gate_names(flake_text: str) -> list[str]:
    block = between(flake_text, "echo \"== replay readiness: static checks ==\"", "echo \"replay readiness checks passed\"")
    names = [match.group("name") for line in block.splitlines() if (match := RUN_GATE_RE.match(line))]
    require(names, "no replay-readiness run_gate entries found")
    require(len(names) == len(set(names)), f"duplicate replay-readiness run_gate entries: {names}")
    return names


def receipt_static_gate_names(flake_text: str) -> list[str]:
    block = between(flake_text, "              gates = [", "              ]\n              receipt = {")
    names = [match.group("name") for line in block.splitlines() if (match := RECEIPT_GATE_RE.match(line))]
    require(names, "no receipt static gate metadata entries found")
    require(len(names) == len(set(names)), f"duplicate receipt static gate metadata entries: {names}")
    return names


def validate_gate_metadata(flake_text: str) -> list[str]:
    executed = executed_static_gate_names(flake_text)
    receipt = receipt_static_gate_names(flake_text)
    missing = [name for name in executed if name not in receipt]
    extra = [name for name in receipt if name not in executed]
    require(not missing, f"executed static gates missing from receipt metadata: {', '.join(missing)}")
    require(not extra, f"receipt static gates without executed run_gate: {', '.join(extra)}")
    return executed


def validate_renderers() -> str:
    summary = load_module("readiness_summary", SUMMARY_SCRIPT)
    dashboard = load_module("readiness_dashboard", DASHBOARD_SCRIPT)
    readme = load_module("readiness_readme", README_SCRIPT)

    with tempfile.TemporaryDirectory() as tmp_raw:
        tmp = Path(tmp_raw)
        receipt = dashboard.sample_receipt(dogfood=True)
        receipt_path = tmp / "receipt.json"
        receipt_path.write_text(json.dumps(receipt, sort_keys=True) + "\n")

        summary_line = summary.summarize(summary.load_receipt(receipt_path))
        require(summary_line.startswith("replay-readiness status="), "summary line has unexpected prefix")
        require("scope=bounded" in summary_line, "summary line lost bounded scope token")

        dashboard_path = tmp / "dashboard.html"
        dashboard.write_dashboard(receipt_path, dashboard_path)
        dashboard_text = dashboard_path.read_text()
        require(summary_line in dashboard_text, "dashboard does not contain summary line")
        require("snapshot_backed_reproduced" in dashboard_text, "dashboard lost dogfood replay class")
        require(bounded(dashboard_text), "dashboard lost bounded anti-overclaim language")

        readme_path = tmp / "README.md"
        readme_path.write_text(
            "# Demo\n\n"
            f"{readme.START_MARKER}\n"
            "old status\n"
            f"{readme.END_MARKER}\n\n"
            "after\n"
        )
        rendered_summary = readme.update_readme(receipt_path, readme_path)
        readme_text = readme_path.read_text()
        require(rendered_summary == summary_line, "README updater returned a different summary line")
        require(summary_line in readme_text, "README snippet does not contain summary line")
        require(bounded(readme_text), "README snippet lost bounded anti-overclaim language")

        missing_marker = tmp / "README-missing.md"
        missing_marker.write_text("# Demo\n")
        try:
            readme.update_readme(receipt_path, missing_marker)
        except readme.ReadmeStatusError:
            pass
        else:
            raise SurfaceDriftError("README updater accepted missing status markers")
    return summary_line


def validate(flake_path: Path = FLAKE) -> list[str]:
    try:
        flake_text = flake_path.read_text()
    except OSError as exc:
        raise SurfaceDriftError(str(exc)) from exc
    gate_names = validate_gate_metadata(flake_text)
    summary_line = validate_renderers()
    return [
        f"static_gates={','.join(gate_names)}",
        f"summary={summary_line}",
    ]


def run_selftest() -> int:
    flake_text = FLAKE.read_text()
    validate_gate_metadata(flake_text)
    validate_renderers()

    missing_receipt_gate = flake_text.replace(
        '                  ("readiness-promotion", "python scripts/check-readiness-promotion-gate.py", os.environ["READINESS_PROMOTION_STATUS"]),\n',
        "",
    )
    try:
        validate_gate_metadata(missing_receipt_gate)
    except SurfaceDriftError as exc:
        require("missing from receipt metadata" in str(exc), f"unexpected missing-gate error: {exc}")
    else:
        raise SurfaceDriftError("missing receipt gate fixture unexpectedly passed")

    extra_receipt_gate = flake_text.replace(
        "              ]\n              receipt = {",
        '                  ("phantom-gate", "python scripts/phantom.py", os.environ["CONTRACT_REGISTRY_STATUS"]),\n              ]\n              receipt = {',
    )
    try:
        validate_gate_metadata(extra_receipt_gate)
    except SurfaceDriftError as exc:
        require("without executed run_gate" in str(exc), f"unexpected extra-gate error: {exc}")
    else:
        raise SurfaceDriftError("extra receipt gate fixture unexpectedly passed")

    print("readiness surface drift selftest ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--flake", type=Path, default=FLAKE)
    parser.add_argument("--selftest", action="store_true", help="run deterministic positive and negative fixtures")
    args = parser.parse_args()

    try:
        if args.selftest:
            return run_selftest()
        lines = validate(args.flake)
        print("readiness surface drift ok:")
        for line in lines:
            print(f"  {line}")
        return 0
    except SurfaceDriftError as exc:
        print(f"readiness surface drift failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
