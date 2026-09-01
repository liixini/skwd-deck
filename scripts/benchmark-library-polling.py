#!/usr/bin/env python3

import argparse
import datetime
import json
import os
import subprocess
import tempfile
from pathlib import Path


PREFIX = "SKWD_POLL_BENCH_JSON="


def create_library(root, files, directories, file_bytes):
    directories = max(1, min(directories, max(1, files)))
    payload = bytes((index % 251 for index in range(file_bytes)))
    for index in range(directories):
        (root / f"group-{index:04d}").mkdir(parents=True)
    for index in range(files):
        folder = root / f"group-{index % directories:04d}"
        (folder / f"wall-{index:07d}.webp").write_bytes(payload)


def filesystem_type(root):
    result = subprocess.run(
        ["stat", "-f", "-c", "%T", str(root)],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def run_benchmark(repo, root, args, evidence_kind):
    step_budget = args.step_budget
    if step_budget is None:
        step_budget = 1 if args.synthetic_entry_delay_us else args.budget
    environment = os.environ.copy()
    environment.update(
        {
            "SKWD_POLL_BENCH_ROOT": str(root),
            "SKWD_POLL_BENCH_BUDGET": str(args.budget),
            "SKWD_POLL_BENCH_STEP_BUDGET": str(step_budget),
            "SKWD_POLL_BENCH_INTERVAL_SECONDS": str(args.interval_seconds),
            "SKWD_POLL_BENCH_ENTRY_DELAY_US": str(args.synthetic_entry_delay_us),
            "SKWD_POLL_BENCH_EVIDENCE": evidence_kind,
        }
    )
    if args.cargo_target_dir:
        environment["CARGO_TARGET_DIR"] = str(args.cargo_target_dir.resolve())
    command = [
        "cargo",
        "test",
        "--release",
        "-p",
        "skwd-walld",
        "polling_fallback_benchmark",
        "--",
        "--ignored",
        "--nocapture",
    ]
    result = subprocess.run(
        command,
        cwd=repo,
        env=environment,
        capture_output=True,
        text=True,
        timeout=args.timeout_seconds,
        check=False,
    )
    combined = result.stdout + "\n" + result.stderr
    if result.returncode != 0:
        raise RuntimeError(
            f"benchmark exited {result.returncode}:\n{combined[-4000:]}"
        )
    payloads = [line[len(PREFIX) :] for line in combined.splitlines() if line.startswith(PREFIX)]
    if len(payloads) != 1:
        raise RuntimeError(f"expected one benchmark payload, found {len(payloads)}")
    return command, json.loads(payloads[0])


def report_for(repo, root, args, evidence_kind):
    command, measurement = run_benchmark(repo, root, args, evidence_kind)
    limitations = [
        "No release budget has been agreed for wakeups, CPU, I/O, memory, or convergence latency.",
        "One process run does not establish distribution tails or cold-cache behaviour.",
        "Logical directory and metadata operations are counted in-process; /proc process I/O does not count stat/getdents calls and may not expose network traffic.",
    ]
    if evidence_kind == "synthetic-latency-model":
        limitations.insert(
            0,
            "Injected per-entry delay models latency in userspace; it is not network or FUSE kernel evidence.",
        )
    else:
        limitations.insert(
            0,
            "The supplied mount is a live observation, but its workload and network conditions still need release approval.",
        )
    return {
        "schema": 1,
        "captured_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "kernel": subprocess.check_output(["uname", "-srmo"], text=True).strip(),
        "filesystem_type": filesystem_type(root),
        "library_source": "generated" if args.root is None else "supplied",
        "generated_files": args.files if args.root is None else None,
        "generated_directories": args.directories if args.root is None else None,
        "evidence_kind": evidence_kind,
        "release_acceptance": False,
        "limitations": limitations,
        "command": command,
        "measurement": measurement,
    }


def main():
    parser = argparse.ArgumentParser(
        description="Measure the bounded library polling fingerprint sweep."
    )
    parser.add_argument("--root", type=Path)
    parser.add_argument("--files", type=int, default=4096)
    parser.add_argument("--directories", type=int, default=64)
    parser.add_argument("--file-bytes", type=int, default=32)
    parser.add_argument("--budget", type=int, default=4096)
    parser.add_argument("--step-budget", type=int)
    parser.add_argument("--interval-seconds", type=int, default=60)
    parser.add_argument("--synthetic-entry-delay-us", type=int, default=250)
    parser.add_argument("--timeout-seconds", type=int, default=900)
    parser.add_argument("--cargo-target-dir", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.files < 1 or args.directories < 1 or args.file_bytes < 0:
        raise SystemExit("generated library sizes must be positive")
    if args.budget < 1 or args.interval_seconds < 1 or args.synthetic_entry_delay_us < 0:
        raise SystemExit("budget and interval must be positive; delay cannot be negative")
    if args.step_budget is not None and args.step_budget < 1:
        raise SystemExit("step budget must be positive")
    if args.root is not None and not args.root.is_dir():
        raise SystemExit(f"root is not a directory: {args.root}")
    repo = Path(__file__).resolve().parent.parent
    evidence_kind = (
        "live-filesystem-observation"
        if args.root is not None and args.synthetic_entry_delay_us == 0
        else "synthetic-latency-model"
    )
    if args.root is not None:
        report = report_for(repo, args.root.resolve(), args, evidence_kind)
    else:
        with tempfile.TemporaryDirectory(prefix="skwd-poll-benchmark-") as temporary:
            root = Path(temporary)
            create_library(root, args.files, args.directories, args.file_bytes)
            report = report_for(repo, root, args, evidence_kind)
    text = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.write_text(text + "\n", encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
