#!/usr/bin/env python3

import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
import urllib.request
import uuid


ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "scripts" / "theme_provider_matrix"
PINS_PATH = MATRIX / "pins.json"
GUEST_PATH = MATRIX / "guest.py"
DEFAULT_CACHE = Path.home() / ".cache" / "skwd-deck" / "theme-provider-vm"
DEFAULT_RESULTS = ROOT / "test-results" / "theme-provider-matrix"
REMOTE_ROOT = "/home/skwd/theme-provider-matrix"


class MatrixError(RuntimeError):
    pass


def run(command, **kwargs):
    print("+", " ".join(str(part) for part in command), flush=True)
    return subprocess.run([str(part) for part in command], check=True, **kwargs)


def require_tool(name):
    path = shutil.which(name)
    if not path:
        raise MatrixError(f"required tool is missing: {name}")
    return path


def direct_dir(path):
    path = path.expanduser()
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.is_symlink() or not path.is_dir():
        raise MatrixError(f"cache path must be a direct directory: {path}")
    return path.resolve()


def direct_file(path, description):
    if path.is_symlink() or not path.is_file():
        raise MatrixError(f"{description} must be a direct regular file: {path}")
    return path


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def prepared_recipe():
    return {
        "schema": 2,
        "base_sha256": pins()["arch_image"]["sha256"],
        "pins_sha256": sha256(PINS_PATH),
        "guest_sha256": sha256(GUEST_PATH),
    }


def prepared_is_current(prepared, metadata):
    direct_file(prepared, "prepared image")
    direct_file(metadata, "prepared metadata")
    try:
        actual = json.loads(metadata.read_text())
    except (json.JSONDecodeError, UnicodeDecodeError) as err:
        raise MatrixError(f"prepared metadata is invalid: {metadata}: {err}") from err
    return all(actual.get(key) == value for key, value in prepared_recipe().items())


@contextlib.contextmanager
def cache_lock(cache):
    lock_path = cache / "matrix.lock"
    descriptor = os.open(lock_path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        yield
    finally:
        os.close(descriptor)


def pins():
    return json.loads(PINS_PATH.read_text())


def download_base(cache):
    image = pins()["arch_image"]
    base = cache / "arch-cloud.qcow2"
    if base.exists():
        direct_file(base, "base image")
        actual = sha256(base)
        if actual != image["sha256"]:
            raise MatrixError(f"base image checksum mismatch: {actual}")
        return base
    candidate = cache / f".arch-cloud.{uuid.uuid4().hex}.download"
    print(f"downloading {image['url']}", flush=True)
    try:
        with urllib.request.urlopen(image["url"], timeout=60) as response:
            with candidate.open("xb") as output:
                shutil.copyfileobj(response, output, 1024 * 1024)
        actual = sha256(candidate)
        if actual != image["sha256"]:
            raise MatrixError(f"downloaded image checksum mismatch: {actual}")
        os.chmod(candidate, 0o600)
        os.replace(candidate, base)
    finally:
        if candidate.exists() and not candidate.is_symlink() and candidate.is_file():
            candidate.unlink()
    return base


def ensure_key(cache):
    key = cache / "vmkey"
    public = cache / "vmkey.pub"
    if key.exists() or public.exists():
        direct_file(key, "SSH private key")
        direct_file(public, "SSH public key")
        return key
    run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", "skwd-theme-vm", "-f", key])
    os.chmod(key, 0o600)
    return key


def make_seed(cache, public_key, instance):
    seed_dir = cache / f"seed-{instance}"
    seed_dir.mkdir(mode=0o700)
    user_data = seed_dir / "user-data"
    meta_data = seed_dir / "meta-data"
    seed = cache / f"seed-{instance}.iso"
    user_data.write_text(
        "#cloud-config\n"
        "users:\n"
        "  - name: skwd\n"
        "    groups: wheel,video,render,seat\n"
        "    sudo: ALL=(ALL) NOPASSWD:ALL\n"
        "    shell: /bin/bash\n"
        "    ssh_authorized_keys:\n"
        f"      - {public_key.read_text().strip()}\n"
        "ssh_pwauth: false\n"
        "growpart:\n"
        "  mode: auto\n"
        "  devices: ['/']\n"
        "resize_rootfs: true\n"
    )
    meta_data.write_text(f"instance-id: {instance}\nlocal-hostname: skwd-theme-vm\n")
    run(["genisoimage", "-quiet", "-output", seed, "-volid", "CIDATA", "-joliet", "-rock", user_data, meta_data])
    return seed, [user_data, meta_data, seed_dir]


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def ssh_base(key, port):
    return [
        "ssh", "-i", key, "-p", str(port),
        "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null", "-o", "LogLevel=ERROR",
        "skwd@127.0.0.1",
    ]


def wait_ssh(key, port, process, timeout=180):
    deadline = time.monotonic() + timeout
    command = ssh_base(key, port) + ["true"]
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise MatrixError(f"QEMU exited before SSH became ready: {process.returncode}")
        result = subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if result.returncode == 0:
            return
        time.sleep(2)
    raise MatrixError("timed out waiting for guest SSH")


def launch_qemu(image, seed, serial, port):
    command = [
        "qemu-system-x86_64", "-enable-kvm", "-cpu", "host",
        "-smp", "4", "-m", "8192", "-display", "none",
        "-vga", "none", "-device", "virtio-vga",
        "-serial", f"file:{serial}", "-no-reboot",
        "-drive", f"if=virtio,format=qcow2,file={image}",
        "-netdev", f"user,id=net0,hostfwd=tcp:127.0.0.1:{port}-:22",
        "-device", "virtio-net-pci,netdev=net0",
    ]
    if seed:
        command += ["-drive", f"if=virtio,format=raw,readonly=on,file={seed}"]
    print("+", " ".join(str(part) for part in command), flush=True)
    return subprocess.Popen(command)


def stop_guest(key, port, process):
    if process.poll() is None:
        subprocess.run(
            ssh_base(key, port) + ["sudo", "systemctl", "poweroff"],
            timeout=15,
            check=False,
        )
    try:
        process.wait(timeout=45)
    except subprocess.TimeoutExpired:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


def scp_to(key, port, sources, destination):
    command = [
        "scp", "-q", "-i", key, "-P", str(port),
        "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
        *sources, f"skwd@127.0.0.1:{destination}",
    ]
    run(command)


def boot_and(key, image, seed, run_dir, action):
    port = free_port()
    serial = run_dir / "serial.log"
    process = launch_qemu(image, seed, serial, port)
    try:
        wait_ssh(key, port, process)
        action(port)
    finally:
        stop_guest(key, port, process)


def prepare(cache, force):
    prepared = cache / "prepared.qcow2"
    metadata = cache / "prepared.json"
    if prepared.exists() and not force:
        if prepared_is_current(prepared, metadata):
            print(f"prepared image is current: {prepared}")
            return prepared
        print("prepared image recipe changed; rebuilding", flush=True)
    base = download_base(cache)
    key = ensure_key(cache)
    candidate = cache / f".prepared.{uuid.uuid4().hex}.qcow2"
    run_dir = cache / f"prepare-{uuid.uuid4().hex}"
    run_dir.mkdir(mode=0o700)
    run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", base, candidate])
    run(["qemu-img", "resize", "-q", candidate, "40G"])
    seed, seed_parts = make_seed(cache, Path(f"{key}.pub"), f"skwd-theme-{uuid.uuid4().hex}")

    def provision(port):
        run(ssh_base(key, port) + ["cloud-init", "status", "--wait"])
        run(ssh_base(key, port) + ["sudo", "pacman", "-Sy", "--noconfirm", "--needed", "python", "git"])
        run(ssh_base(key, port) + ["mkdir", "-p", REMOTE_ROOT])
        scp_to(key, port, [GUEST_PATH, PINS_PATH], f"{REMOTE_ROOT}/")
        run(ssh_base(key, port) + ["python3", f"{REMOTE_ROOT}/guest.py", "provision", "--pins", f"{REMOTE_ROOT}/pins.json"])

    try:
        boot_and(key, candidate, seed, run_dir, provision)
        check = subprocess.run(["qemu-img", "check", "-q", candidate])
        if check.returncode != 0:
            raise MatrixError("prepared image failed qemu-img check")
        os.replace(candidate, prepared)
        metadata_candidate = cache / f".prepared.{uuid.uuid4().hex}.json"
        metadata_value = prepared_recipe()
        metadata_value["created"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        metadata_candidate.write_text(json.dumps(metadata_value, indent=2) + "\n")
        os.replace(metadata_candidate, metadata)
    finally:
        for path in [seed, *seed_parts]:
            if path.exists() and not path.is_symlink():
                if path.is_file():
                    path.unlink()
                elif path.is_dir():
                    path.rmdir()
        if candidate.exists() and not candidate.is_symlink() and candidate.is_file():
            candidate.unlink()
    return prepared


def build_contract():
    run(["cargo", "build", "--locked", "--release", "-p", "skwd-e2e", "--bin", "theme-provider-contract"], cwd=ROOT)
    return direct_file(ROOT / "target" / "release" / "theme-provider-contract", "contract driver")


def execute_matrix(cache, results_root, keep_overlay):
    prepared = cache / "prepared.qcow2"
    metadata = cache / "prepared.json"
    if not prepared_is_current(prepared, metadata):
        raise MatrixError("prepared image recipe is stale; run the all or prepare command")
    key = ensure_key(cache)
    contract = build_contract()
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    results = results_root / stamp
    results.mkdir(parents=True, mode=0o700)
    overlay = cache / f"run-{stamp}-{uuid.uuid4().hex}.qcow2"
    run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", prepared, overlay])

    def matrix(port):
        run(ssh_base(key, port) + ["mkdir", "-p", REMOTE_ROOT])
        scp_to(key, port, [GUEST_PATH, PINS_PATH, contract], f"{REMOTE_ROOT}/")
        command = ssh_base(key, port) + [
            "python3", f"{REMOTE_ROOT}/guest.py", "run",
            "--pins", f"{REMOTE_ROOT}/pins.json",
            "--contract", f"{REMOTE_ROOT}/theme-provider-contract",
            "--results", f"{REMOTE_ROOT}/results",
        ]
        run(command)
        archive = results / "guest-results.tar"
        remote = f"skwd@127.0.0.1:{REMOTE_ROOT}/results.tar"
        run(ssh_base(key, port) + ["tar", "-C", REMOTE_ROOT, "-cf", f"{REMOTE_ROOT}/results.tar", "results"])
        run([
            "scp", "-q", "-i", key, "-P", str(port),
            "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
            remote, archive,
        ])
        run(["bsdtar", "-xf", archive, "-C", results, "--strip-components", "1"])

    success = False
    try:
        boot_and(key, overlay, None, results, matrix)
        success = True
    finally:
        if success and not keep_overlay:
            direct_file(overlay, "run overlay").unlink()
        elif overlay.exists():
            print(f"retained overlay: {overlay}")
    print(f"results: {results}")
    return results


def doctor(cache):
    for tool in ["qemu-system-x86_64", "qemu-img", "genisoimage", "ssh", "scp", "ssh-keygen", "bsdtar", "cargo"]:
        print(f"{tool}: {require_tool(tool)}")
    if not Path("/dev/kvm").exists():
        raise MatrixError("/dev/kvm is unavailable")
    print(f"cache: {cache}")
    print(f"pins: {sha256(PINS_PATH)}")
    if (cache / "prepared.qcow2").exists():
        print(f"prepared: {cache / 'prepared.qcow2'}")


def parse_args():
    parser = argparse.ArgumentParser(description="Run the Deck desktop-theme provider matrix in local QEMU")
    parser.add_argument("command", choices=["doctor", "prepare", "run", "all"])
    parser.add_argument("--cache", type=Path, default=DEFAULT_CACHE)
    parser.add_argument("--results", type=Path, default=DEFAULT_RESULTS)
    parser.add_argument("--force-prepare", action="store_true")
    parser.add_argument("--keep-overlay", action="store_true")
    return parser.parse_args()


def main():
    args = parse_args()
    try:
        cache = direct_dir(args.cache)
        results = direct_dir(args.results)
        with cache_lock(cache):
            if args.command == "doctor":
                doctor(cache)
            elif args.command == "prepare":
                prepare(cache, args.force_prepare)
            elif args.command == "run":
                execute_matrix(cache, results, args.keep_overlay)
            else:
                prepare(cache, args.force_prepare)
                execute_matrix(cache, results, args.keep_overlay)
    except (MatrixError, OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as err:
        print(f"theme-provider-vm: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
