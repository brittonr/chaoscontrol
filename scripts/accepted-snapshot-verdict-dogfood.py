#!/usr/bin/env python3
"""Run bounded snapshot-probe dogfood until an accepted replay verdict exists.

Defaults target the Raft probe, but workload/cmdline/assertion/disk parameters
allow the same filtered-export/verdict rail to exercise non-Raft guests. The
script intentionally keeps raw runtime logs and checkpoints in the output for
local debugging; callers should curate/ignore those before committing.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any

DEFAULT_ASSERTION_ID = 1806003755
DEFAULT_FAIL_AFTER = [25, 30, 35, 20, 40]


def sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text())


def run_command(command: list[str], *, log: Path, timeout: int) -> int:
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as handle:
        try:
            completed = subprocess.run(command, stdout=handle, stderr=subprocess.STDOUT, timeout=timeout)
            return completed.returncode
        except subprocess.TimeoutExpired:
            handle.write(f"\n[accepted-snapshot-verdict-dogfood] timeout after {timeout}s\n")
            return 124


def safe_snapshot_path(run_dir: Path, ref: dict[str, Any]) -> Path:
    rel = PurePosixPath(str(ref.get("path", "")))
    if rel.is_absolute() or ".." in rel.parts or rel.parts[:1] != ("snapshots",):
        raise RuntimeError(f"unconfined snapshot ref path: {rel}")
    path = (run_dir / Path(*rel.parts)).resolve()
    if run_dir.resolve() not in path.parents:
        raise RuntimeError(f"snapshot path escapes run dir: {path}")
    return path


def select_snapshot_bug(run_dir: Path, assertion_id: int) -> Path | None:
    for bug_path in sorted(run_dir.glob("bug_*.json")):
        bug = load_json(bug_path)
        ref = bug.get("replay_parent_snapshot_ref")
        if bug.get("assertion_id") != assertion_id:
            continue
        if not (bug.get("replay_parent_depth", 0) > 0 and ref):
            continue
        artifact = safe_snapshot_path(run_dir, ref)
        if not artifact.is_file():
            raise RuntimeError(f"missing snapshot artifact for {bug_path}: {artifact}")
        digest = ref.get("digest", "")
        if not digest.startswith("sha256:"):
            raise RuntimeError(f"unsupported snapshot digest for {bug_path}: {digest}")
        actual = sha256(artifact)
        if actual != digest:
            raise RuntimeError(f"snapshot digest mismatch for {bug_path}: expected {digest}, got {actual}")
        if ref.get("codec") != "simulation-snapshot-bincode-zstd-v1":
            raise RuntimeError(f"unexpected snapshot codec for {bug_path}: {ref.get('codec')}")
        return bug_path
    return None


def verdict_is_accepted(verdict_path: Path, bug_path: Path, assertion_id: int) -> bool:
    verdict = load_json(verdict_path)
    snapshot = verdict.get("snapshot") or {}
    command = verdict.get("command") or {}
    hashes = verdict.get("artifact_hashes") or []
    return (
        verdict.get("schema_version") == 1
        and verdict.get("replay_class") == "snapshot_backed_reproduced"
        and verdict.get("reproduced") is True
        and verdict.get("assertion_id") == assertion_id
        and verdict.get("replay_parent_depth", 0) > 0
        and snapshot.get("status") == "valid"
        and snapshot.get("digest_verified") is True
        and command.get("exit_status") == 0
        and any(Path(item.get("path", "")).resolve() == bug_path.resolve() for item in hashes)
    )


def copy_tree_contents(src: Path, dst: Path) -> None:
    dst.mkdir(parents=True, exist_ok=True)
    for child in src.iterdir():
        target = dst / child.name
        if target.exists():
            if target.is_dir():
                shutil.rmtree(target)
            else:
                target.unlink()
        if child.is_dir():
            shutil.copytree(child, target)
        else:
            shutil.copy2(child, target)


def summarize_attempt(
    run_dir: Path,
    verdict_path: Path | None,
    *,
    workload: str,
    seed: int,
    fail_after: int,
    run_rc: int,
    export_rc: int | None,
    reproduce_rc: int | None,
) -> dict[str, Any]:
    bugs = []
    for bug_path in sorted(run_dir.glob("bug_*.json")):
        bug = load_json(bug_path)
        bugs.append(
            {
                "file": bug_path.name,
                "assertion_id": bug.get("assertion_id"),
                "replay_parent_depth": bug.get("replay_parent_depth"),
                "has_snapshot_ref": bug.get("replay_parent_snapshot_ref") is not None,
            }
        )
    verdict = load_json(verdict_path) if verdict_path and verdict_path.is_file() else None
    return {
        "workload": workload,
        "seed": seed,
        "snapshot_probe_fail_after": fail_after,
        "run_exit_status": run_rc,
        "export_exit_status": export_rc,
        "reproduce_exit_status": reproduce_rc,
        "bugs": bugs,
        "verdict": None
        if verdict is None
        else {
            "path": str(verdict_path),
            "replay_class": verdict.get("replay_class"),
            "reproduced": verdict.get("reproduced"),
            "replay_parent_depth": verdict.get("replay_parent_depth"),
            "snapshot_status": (verdict.get("snapshot") or {}).get("status"),
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, help="final dogfood output directory")
    parser.add_argument("--kernel", type=Path, default=os.environ.get("KERNEL"))
    parser.add_argument("--initrd", type=Path, default=os.environ.get("INITRD"))
    parser.add_argument("--explore", default=os.environ.get("CHAOSCONTROL_EXPLORE", "chaoscontrol-explore"))
    parser.add_argument("--max-attempts", type=int, default=6)
    parser.add_argument("--start-seed", type=int, default=42)
    parser.add_argument("--run-timeout", type=int, default=240)
    parser.add_argument("--export-timeout", type=int, default=300)
    parser.add_argument("--repro-timeout", type=int, default=300)
    parser.add_argument("--vms", type=int, default=3)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--branches", type=int, default=2)
    parser.add_argument("--ticks", type=int, default=80)
    parser.add_argument("--memory-mb", type=int, default=128)
    parser.add_argument("--disk-image", type=Path)
    parser.add_argument("--workload", default="raft")
    parser.add_argument("--assertion-id", type=int, default=DEFAULT_ASSERTION_ID)
    parser.add_argument("--cmdline-template", default="raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after={fail_after}")
    parser.add_argument("--fail-after-values", default=",".join(str(v) for v in DEFAULT_FAIL_AFTER))
    args = parser.parse_args()

    if args.kernel is None or args.initrd is None:
        parser.error("--kernel/--initrd or KERNEL/INITRD are required")

    output = args.output or Path("dogfood-results") / f"{args.workload}-accepted-verdict-dogfood-{datetime.now(timezone.utc).strftime('%Y%m%d-%H%M%S')}"
    output = output.resolve()
    fail_after_values = [int(value) for value in args.fail_after_values.split(",") if value]
    if not fail_after_values:
        parser.error("--fail-after-values must contain at least one integer")
    scratch = output / "attempts"
    if output.exists() and any(output.iterdir()):
        raise SystemExit(f"output directory is not empty: {output}")
    scratch.mkdir(parents=True, exist_ok=True)

    attempts: list[dict[str, Any]] = []
    for attempt_idx in range(args.max_attempts):
        seed = args.start_seed + attempt_idx
        fail_after = fail_after_values[attempt_idx % len(fail_after_values)]
        run_dir = scratch / f"attempt-{attempt_idx + 1:02d}"
        run_dir.mkdir(parents=True, exist_ok=True)
        extra_cmdline = args.cmdline_template.format(fail_after=fail_after, seed=seed, attempt=attempt_idx + 1)
        run_log = run_dir / "run.log"
        export_log = run_dir / "export-bugs.log"
        reproduce_log = run_dir / "reproduce.log"
        verdict_path: Path | None = None

        run_cmd = [
            args.explore,
            "run",
            "--kernel",
            str(args.kernel),
            "--initrd",
            str(args.initrd),
            "--output",
            str(run_dir),
            "--vms",
            str(args.vms),
            "--rounds",
            str(args.rounds),
            "--branches",
            str(args.branches),
            "--ticks",
            str(args.ticks),
            "--seed",
            str(seed),
            "--mode",
            "hybrid",
            "--bootstrap-budget",
            "10000",
            "--memory-mb",
            str(args.memory_mb),
            "--extra-cmdline",
            extra_cmdline,
        ]
        if args.disk_image is not None:
            run_cmd.extend(["--disk-image", str(args.disk_image)])
        run_rc = run_command(run_cmd, log=run_log, timeout=args.run_timeout)
        export_rc: int | None = None
        reproduce_rc: int | None = None

        if run_rc in (0, 1, 124) and (run_dir / "checkpoint.json").is_file():
            export_cmd = [
                args.explore,
                "export-bugs",
                "--checkpoint",
                str(run_dir / "checkpoint.json"),
                "--output",
                str(run_dir),
                "--assertion-id",
                str(args.assertion_id),
                "--min-replay-parent-depth",
                "1",
                "--max-bugs",
                "1",
            ]
            export_rc = run_command(export_cmd, log=export_log, timeout=args.export_timeout)
            if export_rc == 0 and any(run_dir.glob("bug_*.json")):
                bug_path = select_snapshot_bug(run_dir, args.assertion_id)
                if bug_path is not None:
                    suffix = bug_path.stem.removeprefix("bug_")
                    verdict_path = run_dir / f"replay-verdict-bug{suffix}.json"
                    repro_cmd = [
                        args.explore,
                        "reproduce",
                        "--kernel",
                        str(args.kernel),
                        "--initrd",
                        str(args.initrd),
                        "--bug",
                        str(bug_path),
                        "--vms",
                        str(args.vms),
                        "--bootstrap-budget",
                        "10000",
                        "--memory-mb",
                        str(args.memory_mb),
                        "--extra-cmdline",
                        extra_cmdline,
                        "--verdict-output",
                        str(verdict_path),
                    ]
                    if args.disk_image is not None:
                        repro_cmd.extend(["--disk-image", str(args.disk_image)])
                    reproduce_rc = run_command(repro_cmd, log=reproduce_log, timeout=args.repro_timeout)
                    if reproduce_rc == 0 and verdict_is_accepted(verdict_path, bug_path, args.assertion_id):
                        summary = summarize_attempt(
                            run_dir,
                            verdict_path,
                            workload=args.workload,
                            seed=seed,
                            fail_after=fail_after,
                            run_rc=run_rc,
                            export_rc=export_rc,
                            reproduce_rc=reproduce_rc,
                        )
                        summary["accepted"] = True
                        summary["accepted_bug"] = str(bug_path)
                        summary["accepted_verdict"] = str(verdict_path)
                        copy_tree_contents(run_dir, output)
                        (output / "accepted-snapshot-verdict-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
                        print(f"accepted snapshot-backed verdict: {output / verdict_path.name}")
                        return 0

        attempts.append(
            summarize_attempt(
                run_dir,
                verdict_path,
                workload=args.workload,
                seed=seed,
                fail_after=fail_after,
                run_rc=run_rc,
                export_rc=export_rc,
                reproduce_rc=reproduce_rc,
            )
        )
        (output / "attempts-summary.json").write_text(json.dumps({"accepted": False, "attempts": attempts}, indent=2) + "\n")
        print(f"attempt {attempt_idx + 1}/{args.max_attempts}: no accepted snapshot-backed verdict", file=sys.stderr)

    print(f"no accepted snapshot-backed verdict after {args.max_attempts} attempts; see {output / 'attempts-summary.json'}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
