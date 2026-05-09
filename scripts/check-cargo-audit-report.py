#!/usr/bin/env python3
"""Validate cargo-audit JSON against ChaosControl's triaged dependency policy."""

from __future__ import annotations

import argparse
import json
import tempfile
from pathlib import Path
from typing import Any


FindingKey = tuple[str, str, str, str]


class AuditPolicyError(Exception):
    """Raised when the audit report violates the committed triage policy."""


def load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text())
    except json.JSONDecodeError as exc:
        raise AuditPolicyError(f"invalid JSON in {path}: {exc}") from exc


def finding_key(category: str, item: dict[str, Any]) -> FindingKey:
    advisory = item.get("advisory") or {}
    package = item.get("package") or {}
    return (
        category,
        str(advisory.get("id") or "unknown"),
        str(package.get("name") or "unknown"),
        str(package.get("version") or "unknown"),
    )


def allowlist_key(entry: dict[str, Any]) -> FindingKey:
    return (
        str(entry.get("category") or ""),
        str(entry.get("id") or ""),
        str(entry.get("package") or ""),
        str(entry.get("version") or ""),
    )


def warning_findings(report: dict[str, Any]) -> dict[FindingKey, dict[str, Any]]:
    findings: dict[FindingKey, dict[str, Any]] = {}
    warnings = report.get("warnings") or {}
    for category, items in warnings.items():
        if not isinstance(items, list):
            continue
        for item in items:
            if isinstance(item, dict):
                findings[finding_key(str(category), item)] = item
    return findings


def validate_allowlist(allowlist: dict[str, Any]) -> dict[FindingKey, dict[str, Any]]:
    if allowlist.get("version") != 1:
        raise AuditPolicyError("allowlist version must be 1")
    entries = allowlist.get("warnings")
    if not isinstance(entries, list):
        raise AuditPolicyError("allowlist must contain a warnings list")

    allowed: dict[FindingKey, dict[str, Any]] = {}
    for index, entry in enumerate(entries, start=1):
        if not isinstance(entry, dict):
            raise AuditPolicyError(f"allowlist entry {index} is not an object")
        missing = [
            field
            for field in ("category", "id", "package", "version", "disposition", "rationale", "follow_up")
            if not entry.get(field)
        ]
        if missing:
            raise AuditPolicyError(f"allowlist entry {index} missing required field(s): {', '.join(missing)}")
        key = allowlist_key(entry)
        if key in allowed:
            raise AuditPolicyError(f"duplicate allowlist entry for {format_key(key)}")
        allowed[key] = entry
    return allowed


def format_key(key: FindingKey) -> str:
    category, advisory_id, package, version = key
    return f"{category}:{advisory_id}:{package}@{version}"


def validate_report(report: dict[str, Any], allowlist: dict[str, Any]) -> str:
    vulnerabilities = report.get("vulnerabilities", {}).get("list", [])
    if vulnerabilities:
        lines = ["dependency audit found vulnerability finding(s):"]
        for item in vulnerabilities:
            advisory = item.get("advisory") or {}
            package = item.get("package") or {}
            lines.append(
                f"- {advisory.get('id', 'unknown')} {package.get('name', 'unknown')}@{package.get('version', 'unknown')}"
            )
        raise AuditPolicyError("\n".join(lines))

    findings = warning_findings(report)
    allowed = validate_allowlist(allowlist)
    untriaged = sorted(set(findings) - set(allowed))
    stale = sorted(set(allowed) - set(findings))

    if untriaged or stale:
        lines = []
        if untriaged:
            lines.append("untriaged cargo-audit warning(s):")
            lines.extend(f"- {format_key(key)}" for key in untriaged)
        if stale:
            lines.append("stale cargo-audit warning allowlist entry/entries:")
            lines.extend(f"- {format_key(key)}" for key in stale)
        raise AuditPolicyError("\n".join(lines))

    counts: dict[str, int] = {}
    for category, _, _, _ in findings:
        counts[category] = counts.get(category, 0) + 1
    return "dependency audit ok: vulnerabilities=0 triaged_warnings=" + json.dumps(counts, sort_keys=True)


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


def base_report(warnings: dict[str, list[dict[str, Any]]] | None = None) -> dict[str, Any]:
    return {"vulnerabilities": {"list": []}, "warnings": warnings or {}}


def warning(category: str, advisory_id: str, package: str, version: str) -> tuple[str, dict[str, Any]]:
    return category, {"advisory": {"id": advisory_id}, "package": {"name": package, "version": version}}


def run_selftest() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        category, item = warning("unmaintained", "RUSTSEC-TEST-0001", "demo", "1.0.0")
        report = base_report({category: [item]})
        allowlist = {
            "version": 1,
            "warnings": [
                {
                    "category": category,
                    "id": "RUSTSEC-TEST-0001",
                    "package": "demo",
                    "version": "1.0.0",
                    "disposition": "accepted-test-risk",
                    "rationale": "selftest fixture",
                    "follow_up": "remove selftest fixture",
                }
            ],
        }
        report_path = root / "report.json"
        allowlist_path = root / "allowlist.json"
        write_json(report_path, report)
        write_json(allowlist_path, allowlist)
        validate_report(load_json(report_path), load_json(allowlist_path))

        unknown_category, unknown_item = warning("unsound", "RUSTSEC-TEST-0002", "other", "2.0.0")
        unknown_report = base_report({category: [item], unknown_category: [unknown_item]})
        try:
            validate_report(unknown_report, allowlist)
        except AuditPolicyError as exc:
            assert "untriaged" in str(exc)
        else:
            raise AssertionError("unknown warning should fail")

        try:
            validate_report(base_report({}), allowlist)
        except AuditPolicyError as exc:
            assert "stale" in str(exc)
        else:
            raise AssertionError("stale allowlist entry should fail")

        vulnerable_report = {
            "vulnerabilities": {
                "list": [
                    {"advisory": {"id": "RUSTSEC-TEST-9999"}, "package": {"name": "bad", "version": "9.9.9"}}
                ]
            },
            "warnings": {},
        }
        try:
            validate_report(vulnerable_report, {"version": 1, "warnings": []})
        except AuditPolicyError as exc:
            assert "vulnerability" in str(exc)
        else:
            raise AssertionError("vulnerability should fail")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, help="cargo-audit JSON report path")
    parser.add_argument(
        "--allowlist",
        type=Path,
        default=Path("audits/cargo-audit-warning-allowlist.json"),
        help="triaged cargo-audit warning allowlist",
    )
    parser.add_argument("--selftest", action="store_true", help="run built-in positive and negative checks")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.selftest:
        run_selftest()
        print("cargo audit policy selftest ok")
        return 0
    if args.report is None:
        raise AuditPolicyError("--report is required unless --selftest is set")
    print(validate_report(load_json(args.report), load_json(args.allowlist)))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AuditPolicyError as exc:
        print(str(exc))
        raise SystemExit(1)
