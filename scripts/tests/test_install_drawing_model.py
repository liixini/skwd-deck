import hashlib
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
import urllib.error
import urllib.request
from unittest import mock
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/install_drawing_model.py"
sys.path.insert(0, str(SCRIPT.parent))
import install_drawing_model
ROLES = ("character", "depth", "faces", "heads", "lineart", "salient", "subject", "webgpu")
RUNTIME = "/usr/lib/skwd-test/runtime/libonnxruntime.so"
DRAWING = "skwd-drawing-model-1.0.0"
LENS = "skwd-lens-model-1.0.0"
HOME = "usr/share/skwd-deck/models/drawing"


def write_archive(path, members):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(path, "w:xz") as archive:
        for name, payload in members.items():
            info = tarfile.TarInfo(name)
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))
    return hashlib.sha256(path.read_bytes()).hexdigest()


def release(root, package, members):
    name = f"{package}.tar.xz"
    project = package.rsplit("-", 1)[0]
    digest = write_archive(root / "releases" / project / "releases/download/v1.0.0" / name, members)
    pin = root / f"{package}.sha256"
    pin.write_text(f"{digest}  {name}\n")
    return pin


def fixture(root):
    models = {f"{role}.onnx": role.encode() for role in ROLES}
    files = [
        {
            "path": name,
            "role": name.removesuffix(".onnx"),
            "size": len(payload),
            "sha256": hashlib.sha256(payload).hexdigest(),
        }
        for name, payload in models.items()
    ]
    files[ROLES.index("lineart")]["export"] = {"input": "image", "output": "lines"}
    lock = root / "lock.json"
    lock.write_text(json.dumps({
        "format": 1,
        "product": "drawing-natural-v1",
        "runtime": RUNTIME,
        "files": files,
    }))
    pack = {"version": 3, "runtime": RUNTIME, "device": "auto"}
    drawing = {f"{DRAWING}/drawing/{name}": payload for name, payload in models.items()}
    drawing[f"{DRAWING}/drawing/pack.json"] = json.dumps(pack).encode()
    drawing[f"{DRAWING}/licenses/NOTICE"] = b"notice"
    lens = {
        f"{LENS}/semantic/runtime/libonnxruntime.so": b"\x7fELF runtime",
        f"{LENS}/semantic/text.onnx": b"text",
    }
    return lock, release(root, DRAWING, drawing), release(root, LENS, lens)


def run(root, *extra, releases=None):
    lock, pin, lens_pin = fixture(root) if releases is None else releases
    return subprocess.run(
        [sys.executable, str(SCRIPT), str(root / "cache"), str(root / "install"),
         "--releases", (root / "releases").as_uri(), "--lock", str(lock),
         "--pin", str(pin), "--lens-pin", str(lens_pin), *extra],
        text=True, capture_output=True, check=False,
    )


class InstallDrawingModelTests(unittest.TestCase):
    def test_lays_out_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = run(root, "--device", "cpu")
            self.assertEqual(result.returncode, 0, result.stderr)
            home = root / "install" / HOME
            runtime = root / "install" / RUNTIME.lstrip("/")
            self.assertEqual(result.stdout.splitlines(), [
                f"SKWD_DRAWING_HOME={home}",
                f"SKWD_TEST_DRAWING_PACK={home / 'pack.json'}",
            ])
            pack = json.loads((home / "pack.json").read_text())
            self.assertEqual((pack["runtime"], pack["device"]), (str(runtime), "cpu"))
            self.assertEqual(runtime.read_bytes(), b"\x7fELF runtime")
            self.assertEqual(len(list(home.iterdir())), len(ROLES) + 1)
            self.assertFalse((runtime.parent.parent / "text.onnx").exists())

    def test_reuses_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pins = fixture(root)
            self.assertEqual(run(root, releases=pins).returncode, 0)
            for archive in (root / "releases").rglob("*.tar.xz"):
                archive.unlink()
            result = run(root, releases=pins)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn("downloading", result.stderr)

    def test_rejects_unpinned_archive(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            lock, pin, lens_pin = fixture(root)
            pin.write_text(f"{'0' * 64}  {DRAWING}.tar.xz\n")
            result = run(root, releases=(lock, pin, lens_pin))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("does not match the pin", result.stderr)
            self.assertEqual(list((root / "cache").iterdir()), [])
            self.assertFalse((root / "install").exists())

    def test_rejects_model_drift(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            lock, pin, lens_pin = fixture(root)
            value = json.loads(lock.read_text())
            value["files"][0]["sha256"] = "0" * 64
            lock.write_text(json.dumps(value))
            result = run(root, releases=(lock, pin, lens_pin))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SHA-256 mismatch for character.onnx", result.stderr)

    def test_rejects_malformed_pin(self):
        for text in ("A" * 64, "0" * 63, "0" * 65, "not-a-digest", ""):
            with self.subTest(text=text), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                lock, pin, lens_pin = fixture(root)
                pin.write_text(f"{text}  {DRAWING}.tar.xz\n")
                result = run(root, releases=(lock, pin, lens_pin))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("error:", result.stderr)
                self.assertNotIn("downloading", result.stderr)
                self.assertFalse((root / "install").exists())

    def test_retries_failed_download_once(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, pin, _ = fixture(root)
            archive = install_drawing_model.Archive(
                "skwd-drawing-model", "1.0.0", pin, "drawing", root / "target"
            )
            releases = (root / "releases").as_uri()
            real = urllib.request.urlopen
            for failures, succeeds in ((1, True), (install_drawing_model.DOWNLOAD_ATTEMPTS, False)):
                cache = root / f"cache{failures}"
                cache.mkdir()
                calls = []

                def flaky(url, timeout):
                    calls.append(url)
                    if len(calls) <= failures:
                        raise urllib.error.URLError("reset")
                    return real(url, timeout=timeout)

                with mock.patch.object(install_drawing_model.urllib.request, "urlopen", flaky):
                    if succeeds:
                        path = install_drawing_model.fetch(archive, releases, cache)
                        self.assertTrue(path.is_file())
                    else:
                        with self.assertRaises(urllib.error.URLError):
                            install_drawing_model.fetch(archive, releases, cache)
                self.assertEqual(
                    len(calls), failures + 1 if succeeds else install_drawing_model.DOWNLOAD_ATTEMPTS
                )
                self.assertEqual(list(cache.glob("*.partial")), [])


if __name__ == "__main__":
    unittest.main()
