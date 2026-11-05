#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import tarfile
import tempfile
import urllib.parse
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCK = ROOT / "packaging/drawing-model.lock.json"
DEFAULT_PIN = ROOT / "packaging/skwd-drawing-model-1.0.0.tar.xz.sha256"
PACKAGE_NAME = "skwd-drawing-model"
PINNED_PRODUCT = "drawing-natural-v1"
MODEL_VERSION = "1.0.0"
PACK_PREFIX = "drawing"
PACK_MANIFEST = "pack.json"
PACK_VERSION = 3
PACK_DEVICE = "auto"
LICENSE_PREFIX = "licenses"
LICENSE_FILES = ("Apache-2.0.txt", "MIT.txt")
NOTICE = "NOTICE"
NOTICE_TITLE = f"Skwd Drawing model pack {MODEL_VERSION}"
LINEART_ROLE = "lineart"
WEBGPU_ROLE = "webgpu"
LINEART_GPU_SCOPE = "/model2/"
LINEART_EXPORT_NAMES = ("input", "output")
CHUNK = 1024 * 1024
XZ_ARGUMENTS = ("-6", "--threads=4", "--block-size=64MiB", "--no-adjust", "-c")
DIRECTORY_MODE = 0o755
FILE_MODE = 0o644

WIRE_VARINT = 0
WIRE_FIXED64 = 1
WIRE_LENGTH = 2
WIRE_FIXED32 = 5
WIRE_SIZES = {WIRE_FIXED64: 8, WIRE_FIXED32: 4}
WIRE_BITS = 3
WIRE_MASK = 0b111
VARINT_PAYLOAD = 0x7F
VARINT_SHIFT = 7
MODEL_GRAPH = 7
GRAPH_NODE = 1
GRAPH_INPUT = 11
GRAPH_OUTPUT = 12
NODE_NAME = 3
VALUE_NAME = 1

PACK_ROLES = ("lineart", "depth", "subject", "character", "salient", "faces", "heads")


class PackageError(RuntimeError):
    pass


def single_name(name: Any, label: str) -> str:
    if not isinstance(name, str):
        raise PackageError(f"{label} contains an unsafe path: {name!r}")
    path = PurePosixPath(name)
    if len(path.parts) != 1 or path.name in ("", ".", ".."):
        raise PackageError(f"{label} contains an unsafe path: {name}")
    return name


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(CHUNK):
            digest.update(chunk)
    return digest.hexdigest()


def load_lock(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PackageError(f"cannot read drawing model lock {path}: {error}") from error
    if not isinstance(value, dict) or value.get("format") != 1:
        raise PackageError("drawing model lock must be a format 1 object")
    if value.get("product") != PINNED_PRODUCT:
        raise PackageError(
            f"drawing model lock ships {value.get('product')!r}, not the pinned "
            f"{PINNED_PRODUCT!r}; bump MODEL_VERSION deliberately when the pack changes"
        )
    entries = value.get("files")
    if not isinstance(entries, list) or not entries:
        raise PackageError("drawing model lock must contain files")
    names = [entry.get("path") for entry in entries]
    if names != sorted(names) or len(names) != len(set(names)):
        raise PackageError("drawing model lock paths must be unique and sorted")
    roles = {entry.get("role") for entry in entries}
    if roles != {*PACK_ROLES, WEBGPU_ROLE}:
        raise PackageError(f"drawing model lock roles do not cover the pack: {sorted(roles)}")
    for entry in entries:
        single_name(entry["path"], "drawing model lock")
        for name in entry.get("wheel", {}).get("licenses", {}):
            single_name(name, "drawing model wheel licenses")
        if entry["role"] == LINEART_ROLE:
            export = entry.get("export")
            if not isinstance(export, dict) or not all(
                isinstance(export.get(key), str) for key in LINEART_EXPORT_NAMES
            ):
                raise PackageError("drawing model lock has no lineart export input and output")
    return value


def checked(path: Path, size: int, digest: str, label: str) -> Path:
    if path.is_symlink():
        raise PackageError(f"{label} must not be a symlink: {path}")
    try:
        status = path.stat()
    except OSError as error:
        raise PackageError(f"{label} is missing: {path} ({error})") from error
    if not stat.S_ISREG(status.st_mode):
        raise PackageError(f"{label} is not a regular file: {path}")
    if status.st_size != size:
        raise PackageError(f"size mismatch for {label}: expected {size}, got {status.st_size}")
    actual = sha256(path)
    if actual != digest:
        raise PackageError(f"SHA-256 mismatch for {label}: expected {digest}, got {actual}")
    return path


def source_file(entry: dict[str, Any], sources: Path) -> Path:
    wheel = entry.get("wheel")
    url = entry.get("url")
    name = PurePosixPath(urllib.parse.urlparse(url).path).name if wheel else entry["path"]
    path = sources / name
    if wheel:
        return checked(path, wheel["size"], wheel["sha256"], name)
    return checked(path, entry["size"], entry["sha256"], entry["path"])


def unpack_wheel(entry: dict[str, Any], wheel: Path, workspace: Path) -> dict[str, Path]:
    members = {entry["path"]: entry["wheel"]["member"], **entry["wheel"]["licenses"]}
    unpacked = {}
    with zipfile.ZipFile(wheel) as archive:
        for name, member in members.items():
            destination = workspace / name
            with archive.open(member) as source, destination.open("wb") as output:
                shutil.copyfileobj(source, output, CHUNK)
            unpacked[name] = destination
    checked(unpacked[entry["path"]], entry["size"], entry["sha256"], entry["path"])
    return unpacked


def varint(data: memoryview, offset: int) -> tuple[int, int]:
    value = 0
    shift = 0
    while True:
        byte = data[offset]
        offset += 1
        value |= (byte & VARINT_PAYLOAD) << shift
        if byte <= VARINT_PAYLOAD:
            return value, offset
        shift += VARINT_SHIFT


def fields(data: memoryview):
    offset = 0
    while offset < len(data):
        key, offset = varint(data, offset)
        number, wire = key >> WIRE_BITS, key & WIRE_MASK
        if wire == WIRE_VARINT:
            value, offset = varint(data, offset)
        elif wire == WIRE_LENGTH:
            size, offset = varint(data, offset)
            value = data[offset : offset + size]
            offset += size
        elif wire in WIRE_SIZES:
            value = None
            offset += WIRE_SIZES[wire]
        else:
            raise PackageError(f"lineart graph has an unsupported wire type {wire}")
        yield number, value


def named(message: memoryview, field: int) -> str:
    return next(
        (bytes(value).decode("utf-8") for number, value in fields(message) if number == field),
        "",
    )


def lineart_cpu_nodes(path: Path, export: dict[str, Any]) -> list[str]:
    model = memoryview(path.read_bytes())
    try:
        graph = next((value for number, value in fields(model) if number == MODEL_GRAPH), None)
        if graph is None:
            raise PackageError("lineart model has no graph")
        graph_fields = list(fields(graph))
        inputs = [
            named(value, VALUE_NAME) for number, value in graph_fields if number == GRAPH_INPUT
        ]
        outputs = [
            named(value, VALUE_NAME) for number, value in graph_fields if number == GRAPH_OUTPUT
        ]
        nodes = [named(value, NODE_NAME) for number, value in graph_fields if number == GRAPH_NODE]
    except (IndexError, TypeError, UnicodeDecodeError) as error:
        raise PackageError(f"lineart model is not a readable ONNX graph: {error}") from error
    if inputs != [export["input"]] or outputs != [export["output"]]:
        raise PackageError(f"lineart graph exposes {inputs} -> {outputs}")
    return [node for node in nodes if not node.startswith(LINEART_GPU_SCOPE)]


def pack_manifest(lock: dict[str, Any], files: dict[str, str], cpu_nodes: list[str]) -> bytes:
    pack = {
        "version": PACK_VERSION,
        "runtime": lock["runtime"],
        "device": PACK_DEVICE,
        WEBGPU_ROLE: files[WEBGPU_ROLE],
        LINEART_ROLE: files[LINEART_ROLE],
        "lineart_cpu_nodes": cpu_nodes,
        **{role: files[role] for role in PACK_ROLES if role != LINEART_ROLE},
        "subject_classes": lock["subject_classes"],
        "subject_confidence": lock["subject_confidence"],
    }
    return (json.dumps(pack, indent=2) + "\n").encode("utf-8")


def notice(lock: dict[str, Any]) -> bytes:
    lines = [NOTICE_TITLE, ""]
    lines.extend(
        f"{entry['path']}: {entry['notice']}. License: {entry['license']}."
        for entry in lock["files"]
    )
    return ("\n".join(lines) + "\n").encode("utf-8")


def stage(lock: dict[str, Any], sources: Path, workspace: Path):
    staged = []
    roles = {}
    lineart = None
    for entry in lock["files"]:
        source = source_file(entry, sources)
        roles[entry["role"]] = entry["path"]
        if entry["role"] == WEBGPU_ROLE:
            unpacked = unpack_wheel(entry, source, workspace)
            staged.append((f"{PACK_PREFIX}/{entry['path']}", unpacked.pop(entry["path"])))
            staged.extend((f"{LICENSE_PREFIX}/{name}", path) for name, path in unpacked.items())
            continue
        if entry["role"] == LINEART_ROLE:
            lineart = (source, entry["export"])
        staged.append((f"{PACK_PREFIX}/{entry['path']}", source))
    manifest = workspace / PACK_MANIFEST
    manifest.write_bytes(pack_manifest(lock, roles, lineart_cpu_nodes(*lineart)))
    staged.append((f"{PACK_PREFIX}/{PACK_MANIFEST}", manifest))
    notice_path = workspace / NOTICE
    notice_path.write_bytes(notice(lock))
    staged.append((f"{LICENSE_PREFIX}/{NOTICE}", notice_path))
    for name in LICENSE_FILES:
        source = ROOT / "LICENSES" / name
        if not source.is_file():
            raise PackageError(f"missing Deck license material: {source}")
        staged.append((f"{LICENSE_PREFIX}/{name}", source))
    return sorted(staged, key=lambda item: item[0])


def tar_entry(name: str, epoch: int) -> tarfile.TarInfo:
    info = tarfile.TarInfo(name)
    info.mtime = epoch
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    return info


def write_archive(files: list[tuple[str, Path]], output: Path, prefix: str, epoch: int) -> None:
    directories = sorted({str(PurePosixPath(name).parent) for name, _ in files} - {"."})
    xz = shutil.which("xz")
    if xz is None:
        raise PackageError("xz is required to write the model archive")
    with output.open("wb") as raw:
        process = subprocess.Popen([xz, *XZ_ARGUMENTS], stdin=subprocess.PIPE, stdout=raw)
        try:
            with tarfile.open(
                fileobj=process.stdin, mode="w|", format=tarfile.PAX_FORMAT
            ) as archive:
                for name in ["", *directories]:
                    info = tar_entry(f"{prefix}/{name}".rstrip("/"), epoch)
                    info.type = tarfile.DIRTYPE
                    info.mode = DIRECTORY_MODE
                    archive.addfile(info)
                for name, source in files:
                    info = tar_entry(f"{prefix}/{name}", epoch)
                    info.size = source.stat().st_size
                    info.mode = FILE_MODE
                    with source.open("rb") as handle:
                        archive.addfile(info, handle)
        finally:
            process.stdin.close()
        if process.wait() != 0:
            raise PackageError(f"xz failed with exit status {process.returncode}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Build the redistributable skwd-drawing-model archive from pinned sources."
    )
    parser.add_argument("sources", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--pin", type=Path, default=DEFAULT_PIN)
    parser.add_argument("--unpinned", action="store_true")
    arguments = parser.parse_args()

    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    output_directory = arguments.output.expanduser().absolute()
    output_directory.mkdir(parents=True, exist_ok=True)
    prefix = f"{PACKAGE_NAME}-{MODEL_VERSION}"
    archive_path = output_directory / f"{prefix}.tar.xz"
    if archive_path.exists():
        raise PackageError(f"refusing to overwrite an existing model archive: {archive_path}")

    if not arguments.unpinned and not arguments.pin.is_file():
        raise PackageError(
            f"missing pin {arguments.pin}; pass --unpinned to cut a new pin deliberately"
        )
    lock = load_lock(arguments.lock)
    sources = arguments.sources.expanduser().resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix=".skwd-drawing-model-") as temporary:
        files = stage(lock, sources, Path(temporary))
        write_archive(files, archive_path, prefix, epoch)
    digest = sha256(archive_path)
    if not arguments.unpinned:
        expected = arguments.pin.read_text(encoding="utf-8").split()[0]
        if digest != expected:
            archive_path.unlink()
            raise PackageError(f"model archive digest {digest} does not match the pinned {expected}")

    print(
        json.dumps(
            {
                "archive": str(archive_path),
                "sha256": digest,
                "bytes": archive_path.stat().st_size,
                "package": PACKAGE_NAME,
                "version": MODEL_VERSION,
                "product": lock["product"],
                "files": len(files),
                "pinned": not arguments.unpinned,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except PackageError as error:
        raise SystemExit(f"error: {error}") from error
