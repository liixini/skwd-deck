#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import tarfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from package_drawing_model_archive import (
    CHUNK,
    DEFAULT_LOCK,
    DEFAULT_PIN,
    MODEL_VERSION,
    PACK_MANIFEST,
    PACK_PREFIX,
    PACKAGE_NAME,
    ROOT,
    PackageError,
    checked,
    load_lock,
    sha256,
    single_name,
)

RELEASES = "https://github.com/liixini"
LENS_PACKAGE = "skwd-lens-model"
LENS_VERSION = "1.0.0"
LENS_PIN = ROOT / f"packaging/{LENS_PACKAGE}-{LENS_VERSION}.tar.xz.sha256"
LENS_RUNTIME = "semantic/runtime"
DRAWING_HOME = "usr/share/skwd-deck/models/drawing"
DEVICES = ("auto", "cpu", "cuda", "webgpu")
HOME_ENV = "SKWD_DRAWING_HOME"
PACK_ENV = "SKWD_TEST_DRAWING_PACK"
DOWNLOAD_TIMEOUT = 60
DOWNLOAD_ATTEMPTS = 2
PIN_DIGEST = re.compile(r"[0-9a-f]{64}")
PARTIAL = ".partial"


@dataclass(frozen=True)
class Archive:
    package: str
    version: str
    pin: Path
    member: str
    target: Path

    @property
    def file(self) -> str:
        return f"{self.package}-{self.version}.tar.xz"

    def url(self, releases: str) -> str:
        return f"{releases}/{self.package}/releases/download/v{self.version}/{self.file}"


def pinned(path: Path) -> str:
    try:
        digest = path.read_text(encoding="utf-8").split()[0]
    except (OSError, IndexError) as error:
        raise PackageError(f"cannot read pin {path}: {error}") from error
    if not PIN_DIGEST.fullmatch(digest):
        raise PackageError(f"pin {path} is not a lowercase SHA-256 digest")
    return digest


def download(archive: Archive, releases: str, partial: Path, expected: str) -> None:
    print(f"downloading {archive.url(releases)}", file=sys.stderr, flush=True)
    with urllib.request.urlopen(archive.url(releases), timeout=DOWNLOAD_TIMEOUT) as response:
        with partial.open("wb") as output:
            shutil.copyfileobj(response, output, CHUNK)
    actual = sha256(partial)
    if actual != expected:
        raise PackageError(f"{archive.file} digest {actual} does not match the pin {expected}")


def fetch(archive: Archive, releases: str, cache: Path) -> Path:
    expected = pinned(archive.pin)
    path = cache / archive.file
    if path.is_file() and sha256(path) == expected:
        return path
    partial = path.with_name(path.name + PARTIAL)
    try:
        for attempt in range(1, DOWNLOAD_ATTEMPTS + 1):
            try:
                download(archive, releases, partial, expected)
                break
            except (OSError, PackageError) as error:
                if attempt == DOWNLOAD_ATTEMPTS:
                    raise
                print(f"download failed, retrying: {error}", file=sys.stderr, flush=True)
        os.replace(partial, path)
    finally:
        partial.unlink(missing_ok=True)
    return path


def extract(archive: Archive, path: Path) -> None:
    prefix = PurePosixPath(f"{archive.package}-{archive.version}", archive.member)
    archive.target.mkdir(parents=True, exist_ok=True)
    found = 0
    with tarfile.open(path, "r:xz") as source:
        for member in source:
            name = PurePosixPath(member.name)
            if not member.isfile() or name.parent != prefix:
                continue
            destination = archive.target / single_name(name.name, archive.file)
            with source.extractfile(member) as data, destination.open("wb") as output:
                shutil.copyfileobj(data, output, CHUNK)
            found += 1
    if not found:
        raise PackageError(f"{archive.file} has no {prefix} files")


def install(arguments: argparse.Namespace) -> Path:
    lock = load_lock(arguments.lock)
    home = arguments.root / DRAWING_HOME
    drawing = Archive(PACKAGE_NAME, MODEL_VERSION, arguments.pin, PACK_PREFIX, home)
    runtime = arguments.root / lock["runtime"].lstrip("/")
    lens = Archive(LENS_PACKAGE, LENS_VERSION, arguments.lens_pin, LENS_RUNTIME, runtime.parent)
    arguments.cache.mkdir(parents=True, exist_ok=True)
    for archive in (drawing, lens):
        extract(archive, fetch(archive, arguments.releases, arguments.cache))
    for entry in lock["files"]:
        checked(home / entry["path"], entry["size"], entry["sha256"], entry["path"])
    if not runtime.is_file():
        raise PackageError(f"ONNX Runtime is missing from {lens.file}: {runtime}")
    manifest = home / PACK_MANIFEST
    pack = json.loads(manifest.read_text(encoding="utf-8"))
    pack["runtime"] = str(runtime)
    if arguments.device:
        pack["device"] = arguments.device
    manifest.write_text(json.dumps(pack, indent=2) + "\n", encoding="utf-8")
    return home


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Fetch the pinned Drawing and Lens model archives and lay them out installed."
    )
    parser.add_argument("cache", type=Path)
    parser.add_argument("root", type=Path)
    parser.add_argument("--device", choices=DEVICES)
    parser.add_argument("--releases", default=RELEASES)
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--pin", type=Path, default=DEFAULT_PIN)
    parser.add_argument("--lens-pin", type=Path, default=LENS_PIN)
    arguments = parser.parse_args()
    arguments.cache = arguments.cache.expanduser().absolute()
    arguments.root = arguments.root.expanduser().absolute()
    home = install(arguments)
    print(f"{HOME_ENV}={home}")
    print(f"{PACK_ENV}={home / PACK_MANIFEST}")


if __name__ == "__main__":
    try:
        main()
    except PackageError as error:
        raise SystemExit(f"error: {error}") from error
