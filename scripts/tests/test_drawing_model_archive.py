import hashlib
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/package_drawing_model_archive.py"
ROLES = ("character", "depth", "faces", "heads", "lineart", "salient", "subject")
WHEEL_URL = "https://example.invalid/plugin.whl"
NODES = ("/model0/Conv", "/model2/model2.0/Conv", "/model4/Sigmoid")


def varint(value):
    encoded = bytearray()
    while value > 0x7F:
        encoded.append(value & 0x7F | 0x80)
        value >>= 7
    encoded.append(value)
    return bytes(encoded)


def field(number, payload):
    return varint(number << 3 | 2) + varint(len(payload)) + payload


def named(number, name):
    return field(number, field(1, name.encode()))


def lineart_model():
    nodes = b"".join(field(1, field(3, node.encode())) for node in NODES)
    graph = nodes + named(11, "image") + named(12, "lines")
    return varint(1 << 3) + varint(8) + field(7, graph)


def entry(role, payload):
    return {
        "path": f"{role}.onnx",
        "role": role,
        "size": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
        "license": "MIT",
        "notice": f"{role} fixture",
    }


def fixture(root):
    sources = root / "sources"
    sources.mkdir()
    files = []
    for role in ROLES:
        payload = lineart_model() if role == "lineart" else role.encode()
        (sources / f"{role}.onnx").write_bytes(payload)
        files.append(entry(role, payload))
    files[ROLES.index("lineart")]["export"] = {"input": "image", "output": "lines"}
    plugin = b"\x7fELF plugin"
    wheel = sources / "plugin.whl"
    with zipfile.ZipFile(wheel, "w") as archive:
        archive.writestr("ep/plugin.so", plugin)
        archive.writestr("ep/LICENSE", "MIT")
    webgpu = {
        **entry("webgpu", plugin),
        "path": "plugin.so",
        "url": WHEEL_URL,
        "wheel": {
            "size": wheel.stat().st_size,
            "sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
            "member": "ep/plugin.so",
            "licenses": {"plugin-LICENSE.txt": "ep/LICENSE"},
        },
    }
    files.append(webgpu)
    lock = {
        "format": 1,
        "product": "drawing-natural-v1",
        "runtime": "/usr/share/skwd-lens/models/semantic/runtime/libonnxruntime.so.1.27.0",
        "subject_classes": [0],
        "subject_confidence": 0.3,
        "files": sorted(files, key=lambda item: item["path"]),
    }
    lock_path = root / "lock.json"
    lock_path.write_text(json.dumps(lock), encoding="utf-8")
    return sources, lock_path


def build(root, sources, lock, pin="--unpinned"):
    return subprocess.run(
        [sys.executable, str(SCRIPT), str(sources), str(root / "out"),
         "--lock", str(lock), *pin.split()],
        capture_output=True,
        text=True,
    )


def edit_lock(lock, change):
    value = json.loads(lock.read_text(encoding="utf-8"))
    change({entry["role"]: entry for entry in value["files"]})
    lock.write_text(json.dumps(value), encoding="utf-8")


class DrawingModelArchiveTests(unittest.TestCase):
    def test_builds_pack(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sources, lock = fixture(root)
            result = build(root, sources, lock)
            self.assertEqual(result.returncode, 0, result.stderr)
            archive = root / "out/skwd-drawing-model-1.0.0.tar.xz"
            with tarfile.open(archive) as handle:
                names = set(handle.getnames())
                pack = json.load(handle.extractfile("skwd-drawing-model-1.0.0/drawing/pack.json"))
            self.assertIn("skwd-drawing-model-1.0.0/drawing/plugin.so", names)
            self.assertIn("skwd-drawing-model-1.0.0/licenses/plugin-LICENSE.txt", names)
            self.assertIn("skwd-drawing-model-1.0.0/licenses/NOTICE", names)
            self.assertEqual(pack["lineart_cpu_nodes"], ["/model0/Conv", "/model4/Sigmoid"])
            self.assertEqual((pack["device"], pack["webgpu"]), ("auto", "plugin.so"))

    def test_rejects_changed_model(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sources, lock = fixture(root)
            (sources / "depth.onnx").write_bytes(b"changed")
            result = build(root, sources, lock)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("depth.onnx", result.stderr)

    def test_requires_pin(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sources, lock = fixture(root)
            result = build(root, sources, lock, f"--pin {root / 'missing.sha256'}")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing pin", result.stderr)
            self.assertFalse((root / "out/skwd-drawing-model-1.0.0.tar.xz").exists())

    def test_rejects_bad_lock(self):
        changes = {
            "unsafe path": lambda roles: roles["webgpu"]["wheel"]["licenses"].update(
                {"../LICENSE": "ep/LICENSE"}
            ),
            "lineart export": lambda roles: roles["lineart"].pop("export"),
        }
        for message, change in changes.items():
            with self.subTest(message), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                sources, lock = fixture(root)
                edit_lock(lock, change)
                result = build(root, sources, lock)
                self.assertIn("error: drawing model", result.stderr)
                self.assertIn(message, result.stderr)

    def test_rejects_corrupt_lineart(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sources, lock = fixture(root)
            corrupt = field(7, b"\x0a\xff")
            (sources / "lineart.onnx").write_bytes(corrupt)

            def retarget(roles):
                roles["lineart"]["size"] = len(corrupt)
                roles["lineart"]["sha256"] = hashlib.sha256(corrupt).hexdigest()

            edit_lock(lock, retarget)
            result = build(root, sources, lock)
            self.assertIn("error: lineart model is not a readable ONNX graph", result.stderr)


if __name__ == "__main__":
    unittest.main()
