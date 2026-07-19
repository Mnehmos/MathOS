import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

from mathos.engine import ClaimEngine
from mathos.models import ClaimStatus, StateTransitionError
from mathos.canonical import hash_json
from mathos.trajectory import verify_trajectory

from tests.helpers import load_fixture


class ClaimEngineTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.db_path = Path(self.temp.name) / "mathos.db"
        self.engine = ClaimEngine.open(self.db_path)

    def tearDown(self) -> None:
        self.engine.close()
        self.temp.cleanup()

    def test_canonical_fixtures_complete_all_three_outcomes(self) -> None:
        observed = {}
        for name in ("proved", "disproved", "unresolved"):
            fixture = load_fixture(name)
            claim = self.engine.submit(
                fixture["informal_statement"], fixture["formal_spec"]
            )
            report = self.engine.process(
                claim.claim_id, max_assignments=fixture["max_assignments"]
            )
            observed[name] = report.claim.status.value
            self.assertEqual(report.claim.status.value, fixture["expected_status"])
            self.assertEqual(report.verification["verifier"], "finite-domain-v1")

        self.assertEqual(
            observed,
            {
                "proved": "verified_proved",
                "disproved": "verified_disproved",
                "unresolved": "unresolved",
            },
        )
        self.assertTrue(self.engine.verify_provenance().valid)

    def test_submission_is_content_addressed_and_idempotent(self) -> None:
        fixture = load_fixture("proved")
        first = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )
        second = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )

        self.assertEqual(first.claim_id, second.claim_id)
        self.assertEqual(len(self.engine.list_claims()), 1)

    def test_missing_formalization_remains_explicitly_unresolved(self) -> None:
        claim = self.engine.submit("Every integer has a surprising property.")
        report = self.engine.process(claim.claim_id, max_assignments=16)

        self.assertEqual(report.claim.status, ClaimStatus.UNRESOLVED)
        self.assertEqual(
            report.verification["details"]["reason"], "missing_formal_spec"
        )
        self.assertEqual(report.pedagogy["certainty"], "unresolved")

    def test_verified_claim_cannot_be_downgraded(self) -> None:
        fixture = load_fixture("proved")
        claim = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )
        self.engine.process(claim.claim_id, max_assignments=16)

        with self.assertRaises(StateTransitionError):
            self.engine.ledger.transition_status(
                claim.claim_id, ClaimStatus.UNRESOLVED, reason="test"
            )

    def test_rl_export_contains_verifier_evidence_and_chain_head(self) -> None:
        fixture = load_fixture("disproved")
        claim = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )
        self.engine.process(claim.claim_id, max_assignments=16)

        trajectory = self.engine.export_trajectory(claim.claim_id)
        self.assertEqual(trajectory["schema"], "mathos.rl-trajectory/v1")
        self.assertEqual(trajectory["outcome"], "verified_disproved")
        self.assertEqual(len(trajectory["provenance"]["chain_head"]), 64)
        self.assertTrue(
            any(step["event_type"] == "verification.completed" for step in trajectory["steps"])
        )

        serialized = json.dumps(trajectory, sort_keys=True)
        self.assertNotIn("verified_proved", serialized)
        self.assertTrue(verify_trajectory(trajectory).valid)

    def test_rl_export_tampering_is_detected_even_if_outer_hash_is_recomputed(self) -> None:
        fixture = load_fixture("disproved")
        claim = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )
        self.engine.process(claim.claim_id, max_assignments=16)
        trajectory = self.engine.export_trajectory(claim.claim_id)

        trajectory["steps"][0]["payload"]["informal_statement"] = "tampered"
        body = {key: value for key, value in trajectory.items() if key != "trajectory_hash"}
        trajectory["trajectory_hash"] = hash_json(body)

        result = verify_trajectory(trajectory)
        self.assertFalse(result.valid)
        self.assertEqual(result.reason, "event_hash_mismatch")

    def test_rl_export_requires_a_completed_verification_cycle(self) -> None:
        fixture = load_fixture("proved")
        claim = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )

        with self.assertRaisesRegex(ValueError, "terminal claim"):
            self.engine.export_trajectory(claim.claim_id)

    def test_rl_export_rejects_missing_lifecycle_event(self) -> None:
        fixture = load_fixture("proved")
        claim = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )
        self.engine.process(claim.claim_id, max_assignments=16)
        trajectory = self.engine.export_trajectory(claim.claim_id)

        trajectory["steps"] = [
            event
            for event in trajectory["steps"]
            if event["event_type"] != "claim.status_changed"
        ]
        body = {key: value for key, value in trajectory.items() if key != "trajectory_hash"}
        trajectory["trajectory_hash"] = hash_json(body)

        result = verify_trajectory(trajectory)
        self.assertFalse(result.valid)
        self.assertEqual(result.reason, "lifecycle_order_invalid")

    def test_provenance_tampering_is_detected(self) -> None:
        fixture = load_fixture("proved")
        claim = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )
        self.engine.process(claim.claim_id, max_assignments=16)
        self.engine.close()

        with sqlite3.connect(self.db_path) as connection:
            connection.execute(
                "UPDATE events SET payload_json = ? WHERE sequence = 1",
                ('{"tampered":true}',),
            )

        self.engine = ClaimEngine.open(self.db_path)
        result = self.engine.verify_provenance()
        self.assertFalse(result.valid)
        self.assertEqual(result.broken_sequence, 1)

    def test_materialized_claim_tampering_is_detected_by_replay(self) -> None:
        fixture = load_fixture("proved")
        claim = self.engine.submit(
            fixture["informal_statement"], fixture["formal_spec"]
        )
        self.engine.process(claim.claim_id, max_assignments=16)
        self.engine.close()

        with sqlite3.connect(self.db_path) as connection:
            connection.execute(
                "UPDATE claims SET status = ? WHERE claim_id = ?",
                ("unresolved", claim.claim_id),
            )

        self.engine = ClaimEngine.open(self.db_path)
        result = self.engine.verify_provenance()
        self.assertFalse(result.valid)
        self.assertEqual(result.reason, "claim_projection_mismatch")


if __name__ == "__main__":
    unittest.main()
