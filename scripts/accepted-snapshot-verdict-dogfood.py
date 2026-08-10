#!/usr/bin/env python3
"""Run bounded snapshot-probe dogfood until an accepted replay verdict exists.

Defaults target the Raft probe, but workload/cmdline/assertion/disk parameters
allow the same filtered-export/verdict rail to exercise non-Raft guests. The
script intentionally keeps raw runtime logs and checkpoints in the output for
local debugging; callers should curate/ignore those before committing.
"""

from __future__ import annotations

CURRENT_REPLAY_VERDICT_SCHEMA_VERSION = 2
CURRENT_SNAPSHOT_CODEC = "simulation-snapshot-cbor-zstd-v2"
CURRENT_SNAPSHOT_SCHEMA_VERSION = 2
SUCCESS_EXIT_STATUS = 0
DEFAULT_MAX_ATTEMPTS = 1
DEFAULT_START_SEED = 42
DEFAULT_RUN_TIMEOUT_SECONDS = 240
DEFAULT_EXPORT_TIMEOUT_SECONDS = 300
DEFAULT_REPLAY_TIMEOUT_SECONDS = 300
DEFAULT_BOOTSTRAP_TICKS = 10000
DEFAULT_MEMORY_MIB = 128
SNAPSHOT_CHUNK_BYTES = 20 * 1024 * 1024
MAX_EXPORTED_BUGS = 8

import argparse
import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any

DEFAULT_ASSERTION_ID = 1806003755
DEFAULT_FAIL_AFTER = [1]


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


def assertion_matches_profile(identity: dict[str, Any], profile: dict[str, Any]) -> bool:
    descriptor = identity.get("descriptor") or {}
    logical_key = descriptor.get("logical_key") or {}
    return (
        descriptor.get("namespace") == profile.get("namespace")
        and logical_key.get("type") == "stable"
        and logical_key.get("key") == profile.get("logical_key")
        and descriptor.get("compatibility_id") == profile.get("compatibility_id")
        and descriptor.get("guest") == profile.get("guest")
        and descriptor.get("category") == profile.get("category")
        and descriptor.get("message") == profile.get("message")
        and isinstance(identity.get("fingerprint"), str)
        and len(identity["fingerprint"]) == 64
        and isinstance(identity.get("catalog_token"), str)
        and len(identity["catalog_token"]) == 64
    )


def select_snapshot_bug(
    run_dir: Path,
    assertion_id: int,
    assertion_profile: dict[str, Any],
) -> Path | None:
    for bug_path in sorted(run_dir.glob("bug_*.json")):
        bug = load_json(bug_path)
        ref = bug.get("replay_parent_snapshot_ref")
        identity = bug.get("assertion_identity") or {}
        schedule = bug.get("schedule") or {}
        faults = schedule.get("faults") or []
        if bug.get("assertion_id") != assertion_id:
            continue
        if not assertion_matches_profile(identity, assertion_profile):
            continue
        if not (bug.get("replay_parent_depth", 0) > 0 and ref):
            continue
        if not faults:
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
        if ref.get("codec") != CURRENT_SNAPSHOT_CODEC:
            raise RuntimeError(f"unexpected snapshot codec for {bug_path}: {ref.get('codec')}")
        if ref.get("schema_version") != CURRENT_SNAPSHOT_SCHEMA_VERSION:
            raise RuntimeError(
                f"unexpected snapshot schema for {bug_path}: {ref.get('schema_version')}"
            )
        return bug_path
    return None


def verdict_is_accepted(verdict_path: Path, bug_path: Path, assertion_id: int) -> bool:
    verdict = load_json(verdict_path)
    bug = load_json(bug_path)
    snapshot = verdict.get("snapshot") or {}
    reference = snapshot.get("reference") or {}
    command = verdict.get("command") or {}
    hashes = verdict.get("artifact_hashes") or []
    return (
        verdict.get("schema_version") == CURRENT_REPLAY_VERDICT_SCHEMA_VERSION
        and verdict.get("replay_class") == "snapshot_backed_reproduced"
        and verdict.get("reproduced") is True
        and verdict.get("assertion_id") == assertion_id
        and verdict.get("assertion_identity") == bug.get("assertion_identity")
        and verdict.get("replay_parent_depth", 0) > 0
        and snapshot.get("status") == "valid"
        and snapshot.get("digest_verified") is True
        and reference.get("codec") == CURRENT_SNAPSHOT_CODEC
        and reference.get("schema_version") == CURRENT_SNAPSHOT_SCHEMA_VERSION
        and command.get("exit_status") == SUCCESS_EXIT_STATUS
        and any(Path(item.get("path", "")).resolve() == bug_path.resolve() for item in hashes)
    )


def copy_accepted_artifacts(
    run_dir: Path,
    output: Path,
    bug_path: Path,
    verdict_path: Path,
) -> Path:
    output.mkdir(parents=True, exist_ok=True)
    for source in [run_dir / "assertions.json", bug_path, verdict_path]:
        shutil.copy2(source, output / source.name)
    bug = load_json(bug_path)
    snapshot = safe_snapshot_path(run_dir, bug["replay_parent_snapshot_ref"])
    snapshots = output / "snapshots"
    snapshots.mkdir(parents=True, exist_ok=True)
    target = snapshots / snapshot.name
    shutil.copy2(snapshot, target)
    return target


def chunk_snapshot(snapshot: Path) -> None:
    size = snapshot.stat().st_size
    if size <= SNAPSHOT_CHUNK_BYTES:
        return
    chunks = []
    with snapshot.open("rb") as handle:
        index = 0
        while True:
            data = handle.read(SNAPSHOT_CHUNK_BYTES)
            if not data:
                break
            part = snapshot.with_name(f"{snapshot.name}.part{index:02d}")
            part.write_bytes(data)
            chunks.append(
                {
                    "path": f"snapshots/{part.name}",
                    "size": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                }
            )
            index += 1
    manifest = {
        "schema_version": 1,
        "original_path": snapshot.name,
        "original_size": size,
        "original_sha256": sha256(snapshot).removeprefix("sha256:"),
        "chunks": chunks,
    }
    snapshot.with_name(f"{snapshot.name}.chunks.json").write_text(
        json.dumps(manifest, indent=2) + "\n"
    )
    snapshot.unlink()


def rewrite_command_option(command: str, option: str, value: str) -> str:
    parts = shlex.split(command)
    positions = [index for index, part in enumerate(parts) if part == option]
    if len(positions) != 1 or positions[0] + 1 >= len(parts):
        raise RuntimeError(f"command must contain one value for {option}")
    parts[positions[0] + 1] = value
    return shlex.join(parts)


def rewrite_public_paths(
    output: Path,
    bug_name: str,
    verdict_name: str,
    evidence_prefix: str,
    workload: str,
) -> None:
    bug_path = f"{evidence_prefix}/{bug_name}"
    bug = load_json(output / bug_name)
    snapshot_ref = bug["replay_parent_snapshot_ref"]
    snapshot_path = f"{evidence_prefix}/{snapshot_ref['path']}"
    verdict_file = output / verdict_name
    verdict = load_json(verdict_file)
    verdict["bug_path"] = bug_path
    command = verdict.get("command") or {}
    command_text = command.get("command")
    if not isinstance(command_text, str):
        raise RuntimeError("verdict command.command must be a string")
    command_text = rewrite_command_option(command_text, "--bug", bug_path)
    command_text = rewrite_command_option(
        command_text,
        "--verdict-output",
        f"target/fresh-v2-replay/{workload}-verdict.json",
    )
    command["command"] = command_text
    verdict["command"] = command
    verdict["artifact_hashes"] = [
        {"path": bug_path, "sha256": sha256(output / bug_name)},
        {"path": snapshot_path, "sha256": snapshot_ref["digest"]},
    ]
    verdict_file.write_text(json.dumps(verdict, indent=2) + "\n")


def refresh_existing_output(output: Path, evidence_prefix: str, workload: str) -> None:
    summary = load_json(output / "accepted-snapshot-verdict-summary.json")
    bug_name = Path(summary["accepted_bug"]).name
    verdict_name = Path(summary["accepted_verdict"]).name
    public_verdict_path = f"{evidence_prefix}/{verdict_name}"
    summary["verdict"]["path"] = public_verdict_path
    summary["accepted_verdict"] = public_verdict_path
    (output / "accepted-snapshot-verdict-summary.json").write_text(
        json.dumps(summary, indent=2) + "\n"
    )
    rewrite_public_paths(output, bug_name, verdict_name, evidence_prefix, workload)

    receipt_path = output / "proof-receipt.json"
    receipt = load_json(receipt_path)
    verdict_path = f"{evidence_prefix}/{verdict_name}"
    updated = False
    for artifact in receipt.get("artifacts", []):
        if artifact.get("path") == verdict_path:
            artifact["sha256"] = sha256(output / verdict_name)
            updated = True
    if not updated:
        raise RuntimeError(f"receipt lacks verdict artifact: {verdict_path}")
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")


def write_proof_receipt(
    output: Path,
    cohort: dict[str, Any],
    workload_profile: dict[str, Any],
    bug_name: str,
    verdict_name: str,
    evidence_prefix: str,
    runtime_artifacts: list[tuple[str, Path]],
) -> None:
    receipt_artifacts = []
    for name in ["assertions.json", bug_name, verdict_name]:
        receipt_artifacts.append(
            {"path": f"{evidence_prefix}/{name}", "sha256": sha256(output / name)}
        )
    bug = load_json(output / bug_name)
    receipt_artifacts.append(
        {
            "path": f"{evidence_prefix}/{bug['replay_parent_snapshot_ref']['path']}",
            "sha256": bug["replay_parent_snapshot_ref"]["digest"],
        }
    )
    receipt = {
        "schema_version": 1,
        "status": "accepted",
        "scope": "bounded snapshot-backed replay proof for the recorded workload and cohort",
        "cohort_id": cohort["cohort_id"],
        "source_revision": cohort["source_revision"],
        "workload": workload_profile["workload"],
        "assertion": workload_profile["assertion"],
        "bounds": workload_profile["bounds"],
        "execution": cohort["execution"],
        "kvm_observation": {"readable": True, "writable": True},
        "runtime_artifacts": [
            {"role": role, "path": str(path), "sha256": sha256(path)}
            for role, path in runtime_artifacts
        ],
        "snapshot_policy": cohort["snapshot_policy"],
        "replay_policy": cohort["replay_policy"],
        "artifacts": receipt_artifacts,
        "non_claims": cohort["non_claims"],
    }
    (output / "proof-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


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
                "has_assertion_identity": bug.get("assertion_identity") is not None,
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
    parser.add_argument("--cohort", type=Path)
    parser.add_argument("--evidence-prefix", required=True)
    parser.add_argument("--refresh-output", type=Path)
    parser.add_argument("--max-attempts", type=int, default=DEFAULT_MAX_ATTEMPTS)
    parser.add_argument("--start-seed", type=int, default=DEFAULT_START_SEED)
    parser.add_argument("--run-timeout", type=int, default=DEFAULT_RUN_TIMEOUT_SECONDS)
    parser.add_argument("--export-timeout", type=int, default=DEFAULT_EXPORT_TIMEOUT_SECONDS)
    parser.add_argument("--repro-timeout", type=int, default=DEFAULT_REPLAY_TIMEOUT_SECONDS)
    parser.add_argument("--vms", type=int, default=3)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--branches", type=int, default=2)
    parser.add_argument("--ticks", type=int, default=80)
    parser.add_argument("--memory-mb", type=int, default=DEFAULT_MEMORY_MIB)
    parser.add_argument("--disk-image", type=Path)
    parser.add_argument("--workload", default="raft")
    parser.add_argument("--assertion-id", type=int, default=DEFAULT_ASSERTION_ID)
    parser.add_argument("--cmdline-template", default="raft_bug=snapshot_replay_probe raft_snapshot_probe_fail_after={fail_after}")
    parser.add_argument("--fail-after-values", default=",".join(str(v) for v in DEFAULT_FAIL_AFTER))
    args = parser.parse_args()

    if args.refresh_output is not None:
        refresh_existing_output(
            args.refresh_output.resolve(), args.evidence_prefix, args.workload
        )
        return SUCCESS_EXIT_STATUS
    if args.cohort is None:
        parser.error("--cohort is required")
    if args.kernel is None or args.initrd is None:
        parser.error("--kernel/--initrd or KERNEL/INITRD are required")
    if not os.access("/dev/kvm", os.R_OK | os.W_OK):
        parser.error("/dev/kvm must be readable and writable")

    cohort = load_json(args.cohort)
    workload_profiles = {
        item["workload"]: item for item in cohort.get("workloads", [])
    }
    workload_profile = workload_profiles.get(args.workload)
    if workload_profile is None:
        parser.error(f"workload {args.workload!r} is absent from the cohort")
    if workload_profile["assertion"]["compatibility_id"] != args.assertion_id:
        parser.error("--assertion-id differs from the admitted cohort")
    if workload_profile["cmdline_template"] != args.cmdline_template:
        parser.error("--cmdline-template differs from the admitted cohort")

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
            str(DEFAULT_BOOTSTRAP_TICKS),
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
                str(MAX_EXPORTED_BUGS),
            ]
            export_rc = run_command(export_cmd, log=export_log, timeout=args.export_timeout)
            if export_rc == 0 and any(run_dir.glob("bug_*.json")):
                bug_path = select_snapshot_bug(
                    run_dir,
                    args.assertion_id,
                    workload_profile["assertion"],
                )
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
                        "--seed",
                        str(seed),
                        "--bootstrap-budget",
                        str(DEFAULT_BOOTSTRAP_TICKS),
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
                        summary["accepted_bug"] = f"{args.evidence_prefix}/{bug_path.name}"
                        summary["accepted_verdict"] = (
                            f"{args.evidence_prefix}/{verdict_path.name}"
                        )
                        summary["verdict"]["path"] = summary["accepted_verdict"]
                        snapshot = copy_accepted_artifacts(
                            run_dir,
                            output,
                            bug_path,
                            verdict_path,
                        )
                        rewrite_public_paths(
                            output,
                            bug_path.name,
                            verdict_path.name,
                            args.evidence_prefix,
                            args.workload,
                        )
                        chunk_snapshot(snapshot)
                        runtime_artifacts = [
                            ("host-binary", Path(args.explore)),
                            ("guest-kernel", args.kernel),
                            ("guest-initrd", args.initrd),
                        ]
                        if args.disk_image is not None:
                            runtime_artifacts.append(("guest-disk", args.disk_image))
                        write_proof_receipt(
                            output,
                            cohort,
                            workload_profile,
                            bug_path.name,
                            verdict_path.name,
                            args.evidence_prefix,
                            runtime_artifacts,
                        )
                        summary["receipt"] = f"{args.evidence_prefix}/proof-receipt.json"
                        (output / "accepted-snapshot-verdict-summary.json").write_text(
                            json.dumps(summary, indent=2) + "\n"
                        )
                        print(f"accepted snapshot-backed verdict: {output / verdict_path.name}")
                        return SUCCESS_EXIT_STATUS

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
