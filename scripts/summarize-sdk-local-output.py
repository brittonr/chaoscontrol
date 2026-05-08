#!/usr/bin/env python3
"""Summarize ChaosControl SDK local JSONL output without claiming replay proof."""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path


def summarize(input_path: Path, evidence_class: str) -> dict:
    lifecycle = Counter()
    catalog = {}
    exercised = set()
    sometimes_success = set()
    reachable_hit = set()
    failed = 0
    random_choice_calls = 0
    setup_complete = False
    adoption_tracks = Counter()

    def note_track(track: str | None) -> str | None:
        if track is None:
            return None
        adoption_tracks[track] += 1
        return track

    def details_track(details: object) -> str | None:
        if not isinstance(details, dict):
            return None
        raw = details.get("adoption_track") or details.get("instrumentation_source")
        return str(raw) if raw is not None else None

    for line_no, line in enumerate(input_path.read_text().splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError as exc:
            raise SystemExit(f"invalid JSONL at {input_path}:{line_no}: {exc}") from exc

        assertion = value.get("antithesis_assert")
        if assertion is not None:
            assertion_id = str(assertion.get("id", "unknown"))
            details = assertion.get("details", {})
            track = note_track(details_track(details))
            site = catalog.setdefault(
                assertion_id,
                {
                    "id": assertion_id,
                    "message": assertion.get("message", "<unnamed>"),
                    "assert_type": assertion.get("assert_type", "unknown"),
                    "category": details.get("category", "uncategorized") if isinstance(details, dict) else "uncategorized",
                    "observed": False,
                    "observed_hits": 0,
                    "success_count": 0,
                    "failure_count": 0,
                    "adoption_tracks": [],
                },
            )
            if track is not None and track not in site["adoption_tracks"]:
                site["adoption_tracks"].append(track)
            if not assertion.get("hit", False):
                continue
            exercised.add(assertion_id)
            site["observed"] = True
            site["observed_hits"] += 1
            if assertion.get("condition", False):
                site["success_count"] += 1
            else:
                site["failure_count"] += 1
                failed += 1
            if site["assert_type"] == "sometimes" and assertion.get("condition", False):
                sometimes_success.add(assertion_id)
            if site["assert_type"] == "reachability" and assertion.get("condition", False):
                reachable_hit.add(assertion_id)
            continue

        if "antithesis_setup" in value:
            setup_complete = True
            lifecycle["setup_complete"] += 1
            note_track(details_track(value["antithesis_setup"].get("details", {})))
        elif "chaoscontrol_random_choice" in value:
            random_choice_calls += 1
        elif isinstance(value, dict) and value:
            event_name = next(iter(value.keys()))
            lifecycle[event_name] += 1
            note_track(details_track(value[event_name]))

    sometimes_without_success = [
        site["message"]
        for assertion_id, site in catalog.items()
        if site["assert_type"] == "sometimes" and assertion_id not in sometimes_success
    ]
    reachable_without_hit = [
        site["message"]
        for assertion_id, site in catalog.items()
        if site["assert_type"] == "reachability" and assertion_id not in reachable_hit
    ]
    uncategorized = sum(1 for site in catalog.values() if site["category"] == "uncategorized")

    gaps = []
    if not setup_complete:
        gaps.append("missing setup_complete lifecycle event")
    if uncategorized:
        gaps.append(f"{uncategorized} uncategorized assertion(s)")
    if sometimes_without_success:
        gaps.append(f"{len(sometimes_without_success)} sometimes assertion(s) without observed success")
    if reachable_without_hit:
        gaps.append(f"{len(reachable_without_hit)} reachable assertion(s) without observed hit")

    unobserved_assertions = [
        site["message"] for site in catalog.values() if not site["observed"]
    ]
    assertion_coverage = [
        catalog[assertion_id] for assertion_id in sorted(catalog.keys())
    ]

    return {
        "schema": "chaoscontrol.sdk.local_report.v1",
        "evidence_class": evidence_class,
        "adoption_tracks": dict(sorted(adoption_tracks.items())),
        "instrumentation_sources": dict(sorted(adoption_tracks.items())),
        "replay_evidence": False,
        "replay_boundary": "local SDK JSONL proves instrumentation shape only; VM campaign and replay artifacts must be reviewed separately",
        "setup_complete": setup_complete,
        "lifecycle_events": dict(sorted(lifecycle.items())),
        "cataloged_assertions": len(catalog),
        "registered_assertions": len(catalog),
        "exercised_assertions": len(exercised),
        "observed_assertions": len(exercised),
        "unobserved_assertions": unobserved_assertions,
        "unobserved_assertion_count": len(unobserved_assertions),
        "failed_assertions": failed,
        "sometimes_without_success": sometimes_without_success,
        "reachable_without_hit": reachable_without_hit,
        "uncategorized_assertions": uncategorized,
        "random_choice_calls": random_choice_calls,
        "assertion_coverage": assertion_coverage,
        "gaps": gaps,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--evidence-class", default="instrumentation-dry-run")
    args = parser.parse_args()

    report = summarize(args.input, args.evidence_class)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    main()
