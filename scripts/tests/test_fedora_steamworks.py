import os
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SPEC = ROOT / "packaging/fedora/skwd-deck-steamworks.spec"
BUILDER = ROOT / "scripts/package-fedora-steamworks.sh"
INSTALLED_TEST = ROOT / "packaging/fedora/test-installed.sh"


class FedoraSteamworksTests(unittest.TestCase):
    def test_companion_owns_only_the_helper_runtime_and_notice(self):
        spec = SPEC.read_text(encoding="utf-8")
        self.assertIn("License:        LicenseRef-Proprietary", spec)
        self.assertIn("Recommends:     skwd-deck = %{version}", spec)
        self.assertIn("%{_bindir}/skwd-steam", spec)
        self.assertIn("%{_libexecdir}/skwd-deck/skwd-steam", spec)
        self.assertIn("%{_libexecdir}/skwd-deck/libsteam_api.so", spec)
        self.assertNotIn("skwd-walld", spec)
        self.assertNotIn("%post", spec)

    def test_release_helper_resolves_only_its_sibling_runtime(self):
        helper = ROOT / "target/release/skwd-steam"
        runtime = ROOT / "target/release/libsteam_api.so"
        if not helper.is_file() or not runtime.is_file():
            self.skipTest("release Steamworks artifacts are not built")
        dynamic = subprocess.check_output(["readelf", "-d", helper], text=True)
        self.assertIn("Library runpath: [$ORIGIN]", dynamic)
        self.assertIn("Shared library: [libsteam_api.so]", dynamic)

    def test_scripts_are_syntax_checked_and_document_the_absent_steam_case(self):
        subprocess.run(["sh", "-n", str(BUILDER)], check=True)
        subprocess.run(["sh", "-n", str(INSTALLED_TEST)], check=True)
        installed = INSTALLED_TEST.read_text(encoding="utf-8")
        self.assertIn("Steam is not running", installed)
        self.assertIn("error while loading shared libraries", installed)
        self.assertTrue(os.access(BUILDER, os.X_OK))
        self.assertTrue(os.access(INSTALLED_TEST, os.X_OK))


if __name__ == "__main__":
    unittest.main()
