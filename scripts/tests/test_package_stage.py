import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/package-stage.sh"
LICENSE = ROOT / "LICENSES/ffmpeg-sys-the-third-WTFPL.txt"
BINARIES = (
    "skwd-walld",
    "skwd-wall-scan",
    "skwd-wall-effects",
    "skwd-steam",
    "skwd-helm",
)


def release_binaries(directory: Path, names=BINARIES) -> None:
    directory.mkdir(parents=True)
    for name in names:
        path = directory / name
        path.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        path.chmod(0o755)


class PackageStageTests(unittest.TestCase):
    def test_stage_keeps_the_ffmpeg_license_name_and_exact_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binaries = root / "release"
            destination = root / "stage"
            release_binaries(binaries)
            environment = {**os.environ, "SKWD_DECK_BIN_DIR": str(binaries)}

            subprocess.run(
                ["sh", str(SCRIPT), str(destination)],
                cwd=root,
                env=environment,
                check=True,
                capture_output=True,
                text=True,
            )

            installed = destination / (
                "usr/share/licenses/skwd-deck/ffmpeg-sys-the-third-LICENSE"
            )
            self.assertEqual(installed.read_bytes(), LICENSE.read_bytes())
            self.assertEqual(stat.S_IMODE(installed.stat().st_mode), 0o644)
            for name in BINARIES:
                mode = stat.S_IMODE((destination / "usr/bin" / name).stat().st_mode)
                self.assertEqual(mode, 0o755)
            self.assertFalse(
                (
                    destination
                    / "usr/share/licenses/skwd-deck/ffmpeg-sys-the-third-WTFPL.txt"
                ).exists()
            )

    def test_missing_binary_fails_before_creating_the_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binaries = root / "release"
            destination = root / "stage"
            release_binaries(binaries, BINARIES[1:])
            environment = {**os.environ, "SKWD_DECK_BIN_DIR": str(binaries)}

            result = subprocess.run(
                ["sh", str(SCRIPT), str(destination)],
                cwd=root,
                env=environment,
                check=False,
                capture_output=True,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("skwd-walld", result.stderr)
            self.assertFalse(destination.exists())


if __name__ == "__main__":
    unittest.main()
