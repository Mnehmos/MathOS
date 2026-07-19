from dataclasses import replace
import unittest

from mathos.finite import FiniteDomainVerifier, FiniteSearchEngine
from mathos.models import CandidateKind, VerificationOutcome

from tests.helpers import load_fixture


class FiniteVerifierTests(unittest.TestCase):
    def setUp(self) -> None:
        self.verifier = FiniteDomainVerifier(max_assignments=10_000)

    def test_exhaustive_proof_is_independently_verified(self) -> None:
        fixture = load_fixture("proved")
        candidate = FiniteSearchEngine(fixture["max_assignments"]).search(
            fixture["formal_spec"]
        )

        self.assertEqual(candidate.kind, CandidateKind.ENUMERATION_PROOF)
        result = self.verifier.verify(fixture["formal_spec"], candidate)
        self.assertEqual(result.outcome, VerificationOutcome.PROVED)
        self.assertEqual(result.details["assignments_checked"], 2)

    def test_counterexample_witness_is_independently_checked(self) -> None:
        fixture = load_fixture("disproved")
        candidate = FiniteSearchEngine(fixture["max_assignments"]).search(
            fixture["formal_spec"]
        )

        self.assertEqual(candidate.kind, CandidateKind.COUNTEREXAMPLE)
        result = self.verifier.verify(fixture["formal_spec"], candidate)
        self.assertEqual(result.outcome, VerificationOutcome.DISPROVED)
        self.assertEqual(result.details["witness"], {"p": True, "q": False})

    def test_budget_exhaustion_remains_unknown(self) -> None:
        fixture = load_fixture("unresolved")
        candidate = FiniteSearchEngine(fixture["max_assignments"]).search(
            fixture["formal_spec"]
        )

        self.assertEqual(candidate.kind, CandidateKind.UNKNOWN)
        result = self.verifier.verify(fixture["formal_spec"], candidate)
        self.assertEqual(result.outcome, VerificationOutcome.UNKNOWN)
        self.assertEqual(result.details["reason"], "assignment_budget_exhausted")

    def test_forged_proof_digest_is_rejected(self) -> None:
        fixture = load_fixture("proved")
        candidate = FiniteSearchEngine(16).search(fixture["formal_spec"])
        forged = replace(
            candidate,
            payload={**candidate.payload, "truth_table_hash": "0" * 64},
        )

        result = self.verifier.verify(fixture["formal_spec"], forged)
        self.assertEqual(result.outcome, VerificationOutcome.REJECTED)
        self.assertEqual(result.details["reason"], "proof_certificate_mismatch")

    def test_false_counterexample_is_rejected(self) -> None:
        fixture = load_fixture("disproved")
        candidate = FiniteSearchEngine(16).search(fixture["formal_spec"])
        forged = replace(candidate, payload={"witness": {"p": False, "q": False}})

        result = self.verifier.verify(fixture["formal_spec"], forged)
        self.assertEqual(result.outcome, VerificationOutcome.REJECTED)
        self.assertEqual(result.details["reason"], "witness_does_not_disprove_claim")


if __name__ == "__main__":
    unittest.main()
