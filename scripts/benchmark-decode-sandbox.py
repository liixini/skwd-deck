#!/usr/bin/env python3

import argparse
import json
import math
import os
import select
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def timed(command, env, timeout):
    started = time.perf_counter()
    result = subprocess.run(
        command,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    elapsed = time.perf_counter() - started
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", "replace")[-2000:]
        raise RuntimeError(f"{command[0]} exited {result.returncode}: {detail}")
    return elapsed


def isolated(root, wallpaper):
    config = root / "config" / "skwd-wall-v2"
    video = root / "video"
    workshop = root / "workshop"
    for path in (config, wallpaper, video, workshop, root / "runtime"):
        path.mkdir(parents=True, exist_ok=True)
    (config / "config.json").write_text(
        json.dumps(
            {
                "paths": {
                    "wallpaper": str(wallpaper),
                    "videoWallpaper": str(video),
                    "steamWorkshop": str(workshop),
                },
                "performance": {"maxThumbJobs": 3},
            }
        ),
        encoding="utf-8",
    )
    env = os.environ.copy()
    env.update(
        {
            "HOME": str(root),
            "XDG_CACHE_HOME": str(root / "cache"),
            "XDG_CONFIG_HOME": str(root / "config"),
            "XDG_DATA_HOME": str(root / "data"),
            "XDG_RUNTIME_DIR": str(root / "runtime"),
            "SKWD_WALL_V2_CONFIG": str(config),
            "SKWD_SCAN_THREADS": "3",
            "SKWD_LIVE_PREVIEW_DECODER": "software",
        }
    )
    return env


def tone(binary, image):
    return timed([str(binary), "--tone", str(image)], os.environ.copy(), 30)


def scan(binary, image):
    with tempfile.TemporaryDirectory(prefix="skwd-sandbox-scan-") as temporary:
        root = Path(temporary)
        wallpaper = root / "wallpaper"
        wallpaper.mkdir()
        for index in range(12):
            shutil.copyfile(image, wallpaper / f"sample-{index}.webp")
        env = isolated(root, wallpaper)
        return timed([str(binary)], env, 120)


def preview(binary, video):
    with tempfile.TemporaryDirectory(prefix="skwd-sandbox-preview-") as temporary:
        root = Path(temporary)
        wallpaper = root / "wallpaper"
        env = isolated(root, wallpaper)
        return timed(
            [str(binary), "--preview", "video:sample.mp4", str(video)], env, 120
        )


def first_frame(binary, video):
    with tempfile.TemporaryDirectory(prefix="skwd-sandbox-frame-") as temporary:
        root = Path(temporary)
        env = isolated(root, root / "wallpaper")
        started = time.perf_counter()
        child = subprocess.Popen(
            [str(binary), "--stream", str(video)],
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        target = 640 * 360 * 4
        received = 0
        deadline = started + 30
        try:
            while received < target:
                remaining = deadline - time.perf_counter()
                if remaining <= 0:
                    raise TimeoutError("first preview frame exceeded 30 seconds")
                ready, _, _ = select.select([child.stdout], [], [], remaining)
                if not ready:
                    raise TimeoutError("first preview frame exceeded 30 seconds")
                chunk = os.read(child.stdout.fileno(), target - received)
                if not chunk:
                    detail = child.stderr.read().decode("utf-8", "replace")[-2000:]
                    raise RuntimeError(f"preview stream closed after {received} bytes: {detail}")
                received += len(chunk)
            return time.perf_counter() - started
        finally:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()


def summary(samples):
    ordered = sorted(samples)
    p95 = ordered[max(0, math.ceil(len(ordered) * 0.95) - 1)]
    return {
        "n": len(samples),
        "median_ms": round(statistics.median(samples) * 1000, 3),
        "mean_ms": round(statistics.mean(samples) * 1000, 3),
        "p95_ms": round(p95 * 1000, 3),
    }


def paired(baseline, candidate, operation, count):
    samples = {"baseline": [], "candidate": []}
    operation(baseline)
    operation(candidate)
    for index in range(count):
        order = (("baseline", baseline), ("candidate", candidate))
        if index % 2:
            order = tuple(reversed(order))
        for label, binary in order:
            samples[label].append(operation(binary))
    return {label: summary(values) for label, values in samples.items()}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--video", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    for path in (args.baseline, args.candidate, args.image, args.video):
        if not path.is_file():
            raise SystemExit(f"missing file: {path}")

    baseline = args.baseline.resolve()
    candidate = args.candidate.resolve()
    image = args.image.resolve()
    video = args.video.resolve()
    report = {
        "kernel": subprocess.check_output(["uname", "-srmo"], text=True).strip(),
        "image": subprocess.check_output(["file", "-b", str(image)], text=True).strip(),
        "video": subprocess.check_output(
            ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name,width,height,r_frame_rate", "-of", "json", str(video)],
            text=True,
        ).strip(),
        "tone_process": paired(baseline, candidate, lambda binary: tone(binary, image), 20),
        "full_scan_12_images": paired(
            baseline, candidate, lambda binary: scan(binary, image), 3
        ),
        "animated_preview": paired(
            baseline, candidate, lambda binary: preview(binary, video), 3
        ),
        "software_first_frame": paired(
            baseline, candidate, lambda binary: first_frame(binary, video), 5
        ),
    }
    text = json.dumps(report, indent=2, sort_keys=True)
    if args.output:
        args.output.write_text(text + "\n", encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
