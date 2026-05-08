#!/usr/bin/env python3
"""Update the README replay-readiness status block from a receipt."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SUMMARY_SCRIPT = ROOT / "scripts" / "summarize-replay-readiness-receipt.py"
DASHBOARD_SCRIPT = ROOT / "scripts" / "render-replay-readiness-dashboard.py"
START_MARKER = "<!-- replay-readiness-status:start -->"
END_MARKER = "<!-- replay-readiness-status:end -->"


class ReadmeStatusError(ValueError):
    pass


def load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SUMMARY = load_module("replay_readiness_summary", SUMMARY_SCRIPT)
DASHBOARD = load_module("replay_readiness_dashboard", DASHBOARD_SCRIPT)
ReceiptError = SUMMARY.ReceiptError


def render_status_block(summary_line: str) -> str:
    return "\n".join(
        [
            START_MARKER,
            "> **Replay readiness:** `" + summary_line + "`",
            ">",
            "> This is a bounded committed-evidence signal for ChaosControl's Antithesis-alternative rail: static contracts, accepted proof manifests, and optional selected dogfood evidence. It is not a claim of universal determinism or hosted-product parity.",
            END_MARKER,
        ]
    )


def replace_marker_block(readme_text: str, replacement: str) -> str:
    start = readme_text.find(START_MARKER)
    end = readme_text.find(END_MARKER)
    if start == -1 or end == -1 or end < start:
        raise ReadmeStatusError("README status markers missing or out of order")
    end += len(END_MARKER)
    return readme_text[:start] + replacement + readme_text[end:]


def update_readme(receipt_path: Path, readme_path: Path) -> str:
    receipt = SUMMARY.load_receipt(receipt_path)
    summary_line = SUMMARY.summarize(receipt)
    try:
        existing = readme_path.read_text()
    except OSError as exc:
        raise ReadmeStatusError(str(exc)) from exc
    updated = replace_marker_block(existing, render_status_block(summary_line))
    if updated != existing:
        readme_path.write_text(updated)
    return summary_line


def run_selftest() -> int:
    with tempfile.TemporaryDirectory() as tmp_raw:
        tmp = Path(tmp_raw)
        receipt = DASHBOARD.sample_receipt(dogfood=True)
        receipt_path = tmp / "receipt.json"
        receipt_path.write_text(json.dumps(receipt))
        readme = tmp / "README.md"
        readme.write_text(
            "# Demo\n\n"
            f"{START_MARKER}\n"
            "old status\n"
            f"{END_MARKER}\n\n"
            "after\n"
        )
        summary_line = update_readme(receipt_path, readme)
        rendered = readme.read_text()
        assert "# Demo" in rendered
        assert "after" in rendered
        assert f"`{summary_line}`" in rendered
        assert "snapshot_backed_reproduced" in rendered
        assert "bounded committed-evidence signal" in rendered
        assert rendered.count(START_MARKER) == 1
        assert rendered.count(END_MARKER) == 1

        missing = tmp / "MISSING.md"
        missing.write_text("# Demo\n")
        try:
            update_readme(receipt_path, missing)
        except ReadmeStatusError:
            pass
        else:
            raise AssertionError("missing marker README unexpectedly updated")

        malformed = tmp / "malformed.json"
        malformed.write_text(json.dumps({"command": "other"}))
        try:
            update_readme(malformed, readme)
        except ReceiptError:
            pass
        else:
            raise AssertionError("malformed receipt unexpectedly updated README")
    print("replay-readiness-readme-status selftest ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", nargs="?", type=Path, help="path to replay-readiness receipt JSON")
    parser.add_argument("--readme", type=Path, default=ROOT / "README.md", help="README path to update")
    parser.add_argument("--selftest", action="store_true", help="run deterministic updater self-tests")
    args = parser.parse_args()

    if args.selftest:
        return run_selftest()
    if args.receipt is None:
        parser.error("receipt is required unless --selftest is used")
    try:
        print(update_readme(args.receipt, args.readme))
        return 0
    except (ReceiptError, ReadmeStatusError) as exc:
        print(f"replay-readiness README status update failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
