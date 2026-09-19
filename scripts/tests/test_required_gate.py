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


class RequiredGateTests(unittest.TestCase):
    def test_process_gate_runs_every_opt_in_integration_target(self):
        command = (ROOT / "scripts/test-e2e.sh").read_text(encoding="utf-8")
        for target in (ROOT / "crates/e2e/tests").glob("*.rs"):
            if target.stem == "other_audio_live":
                continue
            if "#[ignore" in target.read_text(encoding="utf-8"):
                self.assertIn(f"--test {target.stem}", command, target.name)

    def test_live_audio_has_explicit_external_fixture_accounting(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        suites = {suite["id"]: suite for suite in json.loads(CATALOG.read_text())["suites"]}
        target = (ROOT / "crates/e2e/tests/other_audio_live.rs").read_text()
        self.assertEqual(target.count("#[ignore"), 1)
        self.assertEqual(suites["deck.audio-live"]["expectedSkips"], 1)
        self.assertIn("private Pulse server", suites["deck.audio-live"]["coverage"])
        audio_step = next(step for step in workflow.split("      - name:")
                          if "--suite deck.audio-live " in step)
        self.assertIn("if: always()", audio_step)
        self.assertIn("ci-report.py\" skip --suite deck.audio-live ", audio_step)
        for variable in ["SKWD_TEST_PULSE_SERVER", "SKWD_TEST_PULSE_SINK", "SKWD_TEST_AUDIO_CLIP"]:
            self.assertIn(variable, target)
            self.assertIn(variable, audio_step)

    def test_workspace_skip_budget_accounts_for_every_ignored_test(self):
        suites = {suite["id"]: suite for suite in json.loads(CATALOG.read_text())["suites"]}
        ignored = sum(len(re.findall(r"(?m)^\s*#\[ignore\b", source.read_text()))
                      for source in (ROOT / "crates").rglob("*.rs"))
        self.assertEqual(suites["deck.tests"]["expectedSkips"], ignored)

    def test_compiled_package_checks_follow_a_successful_release_build(self):
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertLess(workflow.index("name: Release build"),
                        workflow.index("name: Packaging and workflow tests"))
        packaging = next(step for step in workflow.split("      - name:")
                         if "--suite deck.packaging " in step)
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
