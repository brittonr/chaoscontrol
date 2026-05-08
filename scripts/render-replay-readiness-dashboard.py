#!/usr/bin/env python3
"""Render a static HTML dashboard from a replay-readiness receipt."""

from __future__ import annotations

import argparse
import html
import importlib.util
import json
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SUMMARY_SCRIPT = ROOT / "scripts" / "summarize-replay-readiness-receipt.py"


def load_summary_module() -> Any:
    spec = importlib.util.spec_from_file_location("replay_readiness_summary", SUMMARY_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {SUMMARY_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SUMMARY = load_summary_module()
ReceiptError = SUMMARY.ReceiptError


def esc(value: Any) -> str:
    if value is None:
        return "none"
    if isinstance(value, bool):
        return "true" if value else "false"
    return html.escape(str(value), quote=True)


def token_class(value: str) -> str:
    return "ok" if value in {"passed", "pass", "matched", "skipped"} else "bad" if value in {"failed", "fail"} or value.startswith("mismatched") else "warn"


def require_receipt(path: Path) -> tuple[dict[str, Any], str]:
    receipt = SUMMARY.load_receipt(path)
    summary = SUMMARY.summarize(receipt)
    return receipt, summary


def render_gate_rows(gates: list[Any]) -> str:
    rows: list[str] = []
    for gate in gates:
        gate_obj = SUMMARY.require_dict(gate, "receipt.static_gates[]")
        name = SUMMARY.require_str(gate_obj.get("name"), "gate.name")
        command = SUMMARY.require_str(gate_obj.get("command"), "gate.command")
        status = SUMMARY.require_str(gate_obj.get("status"), "gate.status")
        rows.append(
            "<tr>"
            f"<td>{esc(name)}</td>"
            f"<td><span class=\"pill {token_class(status)}\">{esc(status)}</span></td>"
            f"<td><code>{esc(command)}</code></td>"
            "</tr>"
        )
    return "\n".join(rows)


def render_dashboard(receipt: dict[str, Any], summary_line: str) -> str:
    status = SUMMARY.require_str(receipt.get("status"), "receipt.status")
    exit_code = receipt.get("exit_code")
    started_at = receipt.get("started_at")
    finished_at = receipt.get("finished_at")
    failed_phase = receipt.get("failed_phase") or "none"
    scope = SUMMARY.require_str(receipt.get("scope"), "receipt.scope")
    gates = receipt.get("static_gates")
    if not isinstance(gates, list) or not gates:
        raise ReceiptError("receipt.static_gates: expected non-empty list")
    passed = sum(1 for gate in gates if isinstance(gate, dict) and gate.get("status") == "pass")
    dogfood = SUMMARY.require_dict(receipt.get("dogfood"), "receipt.dogfood")
    selected = dogfood.get("selected_workload") or "none"
    dogfood_status = SUMMARY.require_str(dogfood.get("status"), "receipt.dogfood.status")
    expectation_status = dogfood.get("expectation_status") or "not-applicable"
    evidence_curation = dogfood.get("evidence_curation") or "not-recorded"
    output = dogfood.get("output") or "none"
    dogfood_summary = dogfood.get("summary") if isinstance(dogfood.get("summary"), dict) else {}
    verdict = dogfood_summary.get("verdict") if isinstance(dogfood_summary.get("verdict"), dict) else {}
    raw_json = json.dumps(receipt, indent=2, sort_keys=True)

    return f"""<!doctype html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>ChaosControl replay readiness</title>
<style>
:root {{ color-scheme: light dark; --ok:#138a36; --bad:#b42318; --warn:#b7791f; --muted:#667085; --border:#98a2b3; }}
body {{ font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif; margin: 2rem; line-height: 1.45; }}
header {{ border-bottom: 1px solid var(--border); margin-bottom: 1.5rem; padding-bottom: 1rem; }}
.grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr)); gap: 1rem; }}
.card {{ border: 1px solid var(--border); border-radius: 0.75rem; padding: 1rem; }}
.card h2 {{ font-size: 0.95rem; margin: 0 0 0.4rem; color: var(--muted); }}
.value {{ font-size: 1.35rem; font-weight: 700; }}
.pill {{ border-radius: 999px; color: white; display: inline-block; font-weight: 700; padding: 0.15rem 0.55rem; }}
.ok {{ background: var(--ok); }} .bad {{ background: var(--bad); }} .warn {{ background: var(--warn); }}
table {{ border-collapse: collapse; margin-top: 1rem; width: 100%; }}
th, td {{ border-bottom: 1px solid var(--border); padding: 0.55rem; text-align: left; vertical-align: top; }}
code, pre {{ background: rgba(127,127,127,.14); border-radius: .35rem; padding: .1rem .25rem; }}
pre {{ overflow-x: auto; padding: 1rem; }}
.scope {{ border-left: .35rem solid var(--warn); padding-left: .8rem; }}
</style>
</head>
<body>
<header>
<h1>ChaosControl replay readiness</h1>
<p><code>{esc(summary_line)}</code></p>
</header>
<section class=\"grid\" aria-label=\"Replay readiness summary\">
<div class=\"card\"><h2>Status</h2><div class=\"value\"><span class=\"pill {token_class(status)}\">{esc(status)}</span></div></div>
<div class=\"card\"><h2>Exit code</h2><div class=\"value\">{esc(exit_code)}</div></div>
<div class=\"card\"><h2>Static gates</h2><div class=\"value\">{passed}/{len(gates)}</div></div>
<div class=\"card\"><h2>Failed phase</h2><div class=\"value\">{esc(failed_phase)}</div></div>
</section>
<section>
<h2>Dogfood proof rail</h2>
<div class=\"grid\">
<div class=\"card\"><h2>Workload</h2><div class=\"value\">{esc(selected)}</div></div>
<div class=\"card\"><h2>Dogfood status</h2><div class=\"value\"><span class=\"pill {token_class(dogfood_status)}\">{esc(dogfood_status)}</span></div></div>
<div class=\"card\"><h2>Expectation</h2><div class=\"value\"><span class=\"pill {token_class(str(expectation_status))}\">{esc(expectation_status)}</span></div></div>
<div class=\"card\"><h2>Evidence curation</h2><div class=\"value\">{esc(evidence_curation)}</div></div>
<div class=\"card\"><h2>Accepted</h2><div class=\"value\">{esc(dogfood_summary.get('accepted'))}</div></div>
<div class=\"card\"><h2>Replay class</h2><div class=\"value\">{esc(verdict.get('replay_class'))}</div></div>
<div class=\"card\"><h2>Replay-parent depth</h2><div class=\"value\">{esc(verdict.get('replay_parent_depth'))}</div></div>
<div class=\"card\"><h2>Seed / fail-after</h2><div class=\"value\">{esc(dogfood_summary.get('seed'))} / {esc(dogfood_summary.get('snapshot_probe_fail_after'))}</div></div>
</div>
<p>Dogfood output: <code>{esc(output)}</code></p>
</section>
<section>
<h2>Static gates</h2>
<table><thead><tr><th>Gate</th><th>Status</th><th>Command</th></tr></thead><tbody>
{render_gate_rows(gates)}
</tbody></table>
</section>
<section>
<h2>Scope</h2>
<p class=\"scope\">{esc(scope)}</p>
<p>Started: <time>{esc(started_at)}</time>; finished: <time>{esc(finished_at)}</time>.</p>
</section>
<section>
<h2>Raw receipt</h2>
<pre>{esc(raw_json)}</pre>
</section>
</body>
</html>
"""


def write_dashboard(receipt_path: Path, output_path: Path) -> None:
    receipt, summary_line = require_receipt(receipt_path)
    html_text = render_dashboard(receipt, summary_line)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(html_text)


def sample_receipt(*, dogfood: bool = False, status: str = "passed") -> dict[str, Any]:
    dogfood_obj: dict[str, Any] = {
        "selected_workload": None,
        "status": "skipped",
        "output": None,
        "summary": None,
        "expectation": None,
        "expectation_status": "not-applicable",
        "evidence_curation": "explicit-follow-up",
    }
    if dogfood:
        dogfood_obj = {
            "selected_workload": "rust-workload",
            "status": "pass",
            "output": "/tmp/proof&artifact",
            "summary": {
                "accepted": True,
                "seed": 42,
                "snapshot_probe_fail_after": 25,
                "verdict": {
                    "replay_class": "snapshot_backed_reproduced",
                    "replay_parent_depth": 2,
                },
            },
            "expectation": {"expected": {"accepted": True}},
            "expectation_status": "matched",
            "evidence_curation": "explicit-follow-up",
        }
    gates = [
        {"name": "contract-registry", "command": "python scripts/check-contract-registry.py", "status": "pass"},
        {"name": "evidence-contracts", "command": "python scripts/check-evidence-contracts.py", "status": "pass" if status == "passed" else "fail"},
    ]
    return {
        "schema_version": 1,
        "command": "replay-readiness",
        "status": status,
        "exit_code": 0 if status == "passed" else 1,
        "failed_phase": None if status == "passed" else "evidence-contracts",
        "started_at": "2026-05-08T00:00:00Z",
        "finished_at": "2026-05-08T00:00:01Z",
        "static_gates": gates,
        "dogfood": dogfood_obj,
        "scope": "bounded committed replay/evidence readiness; not universal determinism or hosted-product parity",
    }


def run_selftest() -> int:
    with tempfile.TemporaryDirectory() as tmp_raw:
        tmp = Path(tmp_raw)
        for name, receipt in {
            "checks": sample_receipt(),
            "dogfood": sample_receipt(dogfood=True),
            "failed": sample_receipt(status="failed"),
        }.items():
            receipt_path = tmp / f"{name}.json"
            output_path = tmp / f"{name}.html"
            receipt_path.write_text(json.dumps(receipt))
            write_dashboard(receipt_path, output_path)
            rendered = output_path.read_text()
            assert "ChaosControl replay readiness" in rendered
            assert "bounded committed replay/evidence readiness" in rendered
            assert "replay-readiness status=" in rendered
            if name == "dogfood":
                assert "snapshot_backed_reproduced" in rendered
                assert "/tmp/proof&amp;artifact" in rendered
        malformed = tmp / "malformed.json"
        malformed.write_text(json.dumps({"command": "other"}))
        try:
            write_dashboard(malformed, tmp / "bad.html")
        except ReceiptError:
            pass
        else:
            raise AssertionError("malformed receipt unexpectedly rendered")
    print("replay-readiness-dashboard selftest ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", nargs="?", type=Path, help="path to replay-readiness receipt JSON")
    parser.add_argument("--output", "-o", type=Path, help="path to write dashboard HTML")
    parser.add_argument("--selftest", action="store_true", help="run deterministic renderer self-tests")
    args = parser.parse_args()

    if args.selftest:
        return run_selftest()
    if args.receipt is None or args.output is None:
        parser.error("receipt and --output are required unless --selftest is used")
    try:
        write_dashboard(args.receipt, args.output)
        return 0
    except ReceiptError as exc:
        print(f"replay-readiness dashboard failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
