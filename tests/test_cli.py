import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class CliTests(unittest.TestCase):
    def run_cli(self, *arguments: str) -> dict:
        completed = subprocess.run(
            [sys.executable, "-m", "mathos", *arguments],
            check=True,
            capture_output=True,
            text=True,
        )
        return json.loads(completed.stdout)

    def test_demo_runs_complete_vertical_slice(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            completed = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "mathos",
                    "demo",
                    "--workspace",
                    directory,
                    "--reset",
                ],
                check=True,
                capture_output=True,
                text=True,
            )
            result = json.loads(completed.stdout)

        self.assertEqual(result["summary"]["verified_proved"], 1)
        self.assertEqual(result["summary"]["verified_disproved"], 1)
        self.assertEqual(result["summary"]["unresolved"], 1)
        self.assertTrue(result["provenance_valid"])
        self.assertEqual(len(result["exports"]), 3)

    def test_individual_commands_form_a_replayable_workflow(self) -> None:
        root = Path(__file__).parents[1]
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "mathos.db"
            export = Path(directory) / "trajectory.json"
            initialized = self.run_cli("init", "--db", str(database))
            self.assertTrue(initialized["initialized"])

            submitted = self.run_cli(
                "submit",
                "--db",
                str(database),
                "--statement",
                "For every Boolean p, p or not p.",
                "--formal-file",
                str(root / "examples" / "finite" / "excluded_middle.json"),
            )
            claim_id = submitted["claim_id"]
            processed = self.run_cli(
                "run",
                "--db",
                str(database),
                claim_id,
                "--max-assignments",
                "16",
            )
            self.assertEqual(processed["claim"]["status"], "verified_proved")

            exported = self.run_cli(
                "export",
                "--db",
                str(database),
                claim_id,
                "--output",
                str(export),
            )
            self.assertTrue(export.exists())
            self.assertEqual(exported["claim_id"], claim_id)
            self.assertTrue(
                self.run_cli("validate-export", "--input", str(export))["valid"]
            )
            self.assertTrue(
                self.run_cli("replay", "--db", str(database))["valid"]
            )


if __name__ == "__main__":
    unittest.main()
