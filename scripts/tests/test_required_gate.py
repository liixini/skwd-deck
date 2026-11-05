import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".forgejo/workflows/verify.yml"
CATALOG = ROOT / "scripts/ci-suites.json"
EXTERNAL_FIXTURES = {
    "other_audio_live": (
        "deck.audio-live",
        "private Pulse server",
        ["SKWD_TEST_PULSE_SERVER", "SKWD_TEST_PULSE_SINK", "SKWD_TEST_AUDIO_CLIP"],
    ),
}
PROCESS_GATES = ("scripts/test-e2e.sh", "scripts/test-drawing-live.sh")
PINS = (
    "packaging/skwd-drawing-model-1.0.0.tar.xz.sha256",
    "packaging/skwd-lens-model-1.0.0.tar.xz.sha256",
)


def workflow_step(workflow, needle):
    return next(step for step in workflow.split("      - name:") if needle in step)


class RequiredGateTests(unittest.TestCase):
    def test_process_gate_runs_every_opt_in_integration_target(self):
        command = "".join((ROOT / gate).read_text(encoding="utf-8") for gate in PROCESS_GATES)
        for target in (ROOT / "crates/e2e/tests").glob("*.rs"):
            if target.stem in EXTERNAL_FIXTURES:
                continue
            if "#[ignore" in target.read_text(encoding="utf-8"):
                self.assertIn(f"--test {target.stem}", command, target.name)

    def test_external_fixtures_are_accounted(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        suites = {suite["id"]: suite for suite in json.loads(CATALOG.read_text())["suites"]}
        for name, (suite, phrase, variables) in EXTERNAL_FIXTURES.items():
            target = (ROOT / f"crates/e2e/tests/{name}.rs").read_text()
            self.assertGreater(target.count("#[ignore"), 0, name)
            self.assertEqual(suites[suite]["expectedSkips"], 1, suite)
            self.assertIn(phrase, suites[suite]["coverage"], suite)
            step = workflow_step(workflow, f"--suite {suite} ")
            self.assertIn("if: always()", step)
            self.assertIn(f"ci-report.py\" skip --suite {suite} ", step)
            for variable in variables:
                self.assertIn(variable, target)
                self.assertIn(variable, step)

    def test_drawing_gate_runs_pinned_pack(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        gate = (ROOT / "scripts/test-drawing-live.sh").read_text(encoding="utf-8")
        target = (ROOT / "crates/e2e/tests/drawing_live.rs").read_text(encoding="utf-8")
        suites = {suite["id"]: suite for suite in json.loads(CATALOG.read_text())["suites"]}
        deferred = re.search(r"(?m)^deferred=(\w+)$", gate).group(1)
        self.assertIn(f"fn {deferred}()", target)
        self.assertEqual(suites["deck.drawing-live"]["expectedSkips"], 1)
        self.assertIn("Paper and Wallpaper Engine", suites["deck.drawing-live"]["coverage"])
        step = workflow_step(workflow, "--suite deck.drawing-live ")
        self.assertIn("-- scripts/test-drawing-live.sh", step)
        for needed in ("tests", "release", "drawing-pack"):
            self.assertIn(f"steps.{needed}.outcome == 'success'", step)
        install = workflow_step(workflow, "install_drawing_model.py")
        self.assertIn("id: drawing-pack", install)
        self.assertIn('--device cpu >> "$GITHUB_ENV"', install)
        restore = workflow_step(workflow, "Restore Drawing model cache")
        save = workflow_step(workflow, "Save Drawing model cache")
        action, ref = re.search(r"uses: (\S+)/restore@(\w+)\n", restore).groups()
        self.assertIn(f"uses: {action}/save@{ref}\n", save)
        self.assertIn("id: drawing-cache", restore)
        self.assertIn("steps.drawing-pack.outcome == 'success'", save)
        self.assertIn("steps.drawing-cache.outputs.cache-hit != 'true'", save)
        self.assertLess(workflow.index(install), workflow.index(save))
        for pin in PINS:
            self.assertTrue((ROOT / pin).is_file(), pin)
            self.assertIn(f"'skwd-deck/{pin}'", restore)
            self.assertIn(f"'skwd-deck/{pin}'", save)
        self.assertEqual(
            re.search(r"key: (.+)", restore).group(1), re.search(r"key: (.+)", save).group(1)
        )
        cleanup = workflow_step(workflow, "Remove Drawing model files")
        self.assertIn("if: always()", cleanup)
        self.assertIn('rm -rf "$SKWD_MODEL_CACHE" "$TMPDIR/drawing-install"', cleanup)
        self.assertGreater(workflow.index(cleanup), workflow.index(save))

    def test_workspace_skip_budget_accounts_for_every_ignored_test(self):
        suites = {suite["id"]: suite for suite in json.loads(CATALOG.read_text())["suites"]}
        ignored = sum(len(re.findall(r"(?m)^\s*#\[ignore\b", source.read_text()))
                      for source in (ROOT / "crates").rglob("*.rs"))
        self.assertEqual(suites["deck.tests"]["expectedSkips"], ignored)

    def test_compiled_package_checks_follow_a_successful_release_build(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertLess(workflow.index("name: Release build"),
                        workflow.index("name: Packaging and workflow tests"))
        packaging = workflow_step(workflow, "--suite deck.packaging ")
        self.assertIn("if: steps.release.outcome == 'success'", packaging)

    def test_local_gate_discovers_the_same_python_tests_as_ci(self):
        discovery = "python3 -m unittest discover -s scripts/tests -p test_*.py"
        self.assertIn(discovery, WORKFLOW.read_text(encoding="utf-8"))
        self.assertIn(discovery, (ROOT / "scripts/test-all.sh").read_text(encoding="utf-8"))

    def test_local_gate_fails_when_a_new_python_guard_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tests = root / "scripts/tests"
            tests.mkdir(parents=True)
            shutil.copy2(ROOT / "scripts/test-all.sh", root / "scripts/test-all.sh")
            (tests / "test_future_guard.py").write_text(
                "import unittest\n"
                "class FutureGuard(unittest.TestCase):\n"
                "    def test_new_guard(self):\n"
                "        self.fail('new Python guard was discovered')\n"
            )
            tools = root / "bin"
            tools.mkdir()
            cargo = tools / "cargo"
            cargo.write_text("#!/bin/sh\nexit 0\n")
            cargo.chmod(0o755)
            environment = dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}")
            result = subprocess.run(
                ["sh", str(root / "scripts/test-all.sh")], env=environment,
                text=True, capture_output=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("new Python guard was discovered", result.stderr)

    def test_workflow_emits_the_exact_catalog_and_retains_reports(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        value = json.loads(CATALOG.read_text(encoding="utf-8"))
        suites = [suite["id"] for suite in value["suites"]]
        self.assertEqual(len(suites), len(set(suites)))
        for suite in suites:
            self.assertEqual(workflow.count(f"--suite {suite} "), 1, suite)
        self.assertIn("name: Forgejo / Deck required", workflow)
        self.assertIn("pull_request: {}", workflow)
        self.assertIn("SKWD_VERIFY_ROOT: ../skwd-verify", workflow)
        self.assertIn(
            'if: always()\n        run: python3 "$SKWD_VERIFY_ROOT/scripts/ci-report.py" aggregate',
            workflow,
        )
        self.assertIn("retention-days: 14", workflow)
        self.assertIn(
            "actions/upload-artifact@c6a3b2bd78b3985e4b2f15397fec357f0fd808de",
            workflow,
        )


if __name__ == "__main__":
    unittest.main()
