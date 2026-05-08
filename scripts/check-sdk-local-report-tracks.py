#!/usr/bin/env python3
"""Fixture checks for SDK local report adoption-track summaries."""

from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SUMMARY = ROOT / "scripts" / "summarize-sdk-local-output.py"

spec = importlib.util.spec_from_file_location("sdk_local_summary", SUMMARY)
assert spec is not None and spec.loader is not None
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def summarize_jsonl(content: str) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "sdk.jsonl"
        path.write_text(content)
        return module.summarize(path, "instrumentation-dry-run")


def assert_tracks(name: str, content: str, expected: dict[str, int]) -> None:
    report = summarize_jsonl(content)
    actual = report["adoption_tracks"]
    if actual != expected:
        raise AssertionError(f"{name}: expected {expected}, got {actual}")
    if report["instrumentation_sources"] != expected:
        raise AssertionError(f"{name}: instrumentation_sources drifted")
    if report["replay_evidence"] is not False:
        raise AssertionError(f"{name}: local report claimed replay evidence")


def main() -> None:
    harness = """{\"antithesis_setup\":{\"status\":\"complete\",\"details\":{\"adoption_track\":\"external-harness\"}}}
{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"must_hit\":false,\"id\":\"1\",\"message\":\"driver invariant\",\"display_type\":\"always\",\"details\":{\"category\":\"driver\",\"adoption_track\":\"external-harness\"}}}
"""
    in_process = """{\"antithesis_assert\":{\"assert_type\":\"always\",\"condition\":true,\"hit\":true,\"must_hit\":false,\"id\":\"2\",\"message\":\"internal invariant\",\"display_type\":\"always\",\"details\":{\"category\":\"service-invariant\",\"instrumentation_source\":\"in-process-service\"}}}
"""
    mixed = harness + in_process

    assert_tracks("harness-only", harness, {"external-harness": 2})
    assert_tracks("in-process-only", in_process, {"in-process-service": 1})
    assert_tracks("mixed", mixed, {"external-harness": 2, "in-process-service": 1})
    print("sdk-local-report-tracks: ok")


if __name__ == "__main__":
    main()
