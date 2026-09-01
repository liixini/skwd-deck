import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


host = load("theme_provider_vm", ROOT / "scripts" / "theme-provider-vm.py")
guest = load("theme_provider_guest", ROOT / "scripts" / "theme_provider_matrix" / "guest.py")


class ThemeProviderVmTests(unittest.TestCase):
    def test_pins_are_full_commits_and_image_is_immutable(self):
        value = json.loads(host.PINS_PATH.read_text())
        self.assertEqual(value["schema"], 1)
        self.assertIn("/images/v", value["arch_image"]["url"])
        self.assertEqual(len(value["arch_image"]["sha256"]), 64)
        self.assertEqual(set(value["providers"]), set(guest.PROVIDERS))
        self.assertEqual(set(value["dependencies"]), {"libcava"})
        for provider in [*value["providers"].values(), *value["dependencies"].values()]:
            self.assertEqual(len(provider["commit"]), 40)
            int(provider["commit"], 16)

    def test_cache_rejects_symlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            real = root / "real"
            real.mkdir()
            linked = root / "linked"
            linked.symlink_to(real, target_is_directory=True)
            with self.assertRaises(host.MatrixError):
                host.direct_dir(linked)

    def test_native_mutations_round_trip_sentinel_shape(self):
        fixtures = {
            "caelestia": {"colours": {"primary": "000000", **{f"r{i}": "000000" for i in range(48)}}},
            "dms": {"colors": {"dark": {"primary": "#000000"}, "light": {}}},
            "noctalia": {"dark": {"mPrimary": "#000000"}, "light": {}},
            "end4": {"primary": "#000000", **{f"r{i}": "#000000" for i in range(48)}},
        }
        for name, value in fixtures.items():
            guest.validate_native(name, value)
            changed = guest.mutate_native(name, value)
            if name == "caelestia":
                sentinel = changed["colours"]["primary"]
            elif name == "dms":
                sentinel = changed["colors"]["dark"]["primary"]
            elif name == "noctalia":
                sentinel = changed["dark"]["mPrimary"]
            else:
                sentinel = changed["primary"]
            self.assertEqual(sentinel.lstrip("#"), "11aa77")

    def test_provider_paths_follow_isolated_xdg_roots(self):
        env = {
            "XDG_STATE_HOME": "/state",
            "XDG_CACHE_HOME": "/cache",
            "XDG_CONFIG_HOME": "/config",
        }
        self.assertEqual(guest.provider_path("caelestia", env), Path("/state/caelestia/scheme.json"))
        self.assertEqual(guest.provider_path("dms", env), Path("/cache/DankMaterialShell/dms-colors.json"))
        self.assertEqual(guest.provider_path("noctalia", env), Path("/config/noctalia/palettes/skwd-wall.json"))
        self.assertEqual(guest.provider_path("end4", env), Path("/state/quickshell/user/generated/colors.json"))

    def test_providers_run_under_their_supported_compositor(self):
        self.assertEqual(guest.PROVIDER_COMPOSITORS, {
            "caelestia": "hyprland",
            "dms": "sway",
            "noctalia": "sway",
            "end4": "hyprland",
        })

    def test_hyprland_protocol_failures_are_gating(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "caelestia.log"
            log.write_text("The active compositor does not support hyprland_global_shortcuts_v1.\n")
            with self.assertRaises(guest.GuestError):
                guest.validate_compositor(
                    "caelestia", {"HYPRLAND_INSTANCE_SIGNATURE": "test"}, log
                )

    def test_hyprland_ipc_failure_is_gating(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "end4.log"
            log.write_text("Hyprland ipc status request failed.\n")
            with self.assertRaises(guest.GuestError):
                guest.validate_compositor(
                    "end4", {"HYPRLAND_INSTANCE_SIGNATURE": "test"}, log
                )

    def test_sway_provider_requires_its_ipc_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "noctalia.log"
            log.write_text("")
            with self.assertRaises(guest.GuestError):
                guest.validate_compositor("noctalia", {}, log)

    def test_prepared_recipe_tracks_guest_and_pins(self):
        recipe = host.prepared_recipe()
        self.assertEqual(recipe["schema"], 2)
        self.assertEqual(recipe["pins_sha256"], host.sha256(host.PINS_PATH))
        self.assertEqual(recipe["guest_sha256"], host.sha256(host.GUEST_PATH))

    def test_prepared_image_rejects_a_stale_recipe(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            prepared = root / "prepared.qcow2"
            metadata = root / "prepared.json"
            prepared.write_bytes(b"qcow")
            value = host.prepared_recipe()
            value["guest_sha256"] = "0" * 64
            metadata.write_text(json.dumps(value))
            self.assertFalse(host.prepared_is_current(prepared, metadata))


if __name__ == "__main__":
    unittest.main()
