import copy
from dataclasses import replace
import random
from pathlib import Path
import tempfile
import unittest

from mathos.engine import ClaimEngine
from mathos.finite import FiniteDomainVerifier, FiniteSearchEngine
from mathos.models import CandidateKind, VerificationOutcome
from mathos.lean import LeanSubprocessVerifier

from tests.helpers import load_fixture


class AdversarialTests(unittest.TestCase):
    @staticmethod
    def reference_boolean(expression: dict, assignment: dict[str, bool]) -> bool:
        if "var" in expression:
            return assignment[expression["var"]]
        if "literal" in expression:
            return expression["literal"]
        operation = expression["op"]
        if operation == "not":
            return not AdversarialTests.reference_boolean(expression["arg"], assignment)
        if operation == "and":
            values = [
                AdversarialTests.reference_boolean(arg, assignment)
                for arg in expression["args"]
            ]
            return all(values)
        if operation == "or":
            values = [
                AdversarialTests.reference_boolean(arg, assignment)
                for arg in expression["args"]
            ]
            return any(values)
        if operation == "implies":
            left = AdversarialTests.reference_boolean(expression["left"], assignment)
            right = AdversarialTests.reference_boolean(expression["right"], assignment)
            return (not left) or right
        raise AssertionError(operation)

    @staticmethod
    def random_boolean_expression(
        generator: random.Random, depth: int
    ) -> dict:
        if depth == 0 or generator.random() < 0.3:
            if generator.random() < 0.25:
                return {"literal": generator.choice([False, True])}
            return {"var": generator.choice(["a", "b", "c"])}
        operation = generator.choice(["not", "and", "or", "implies"])
        if operation == "not":
            return {
                "op": "not",
                "arg": AdversarialTests.random_boolean_expression(generator, depth - 1),
            }
        if operation in {"and", "or"}:
            return {
                "op": operation,
                "args": [
                    AdversarialTests.random_boolean_expression(generator, depth - 1),
                    AdversarialTests.random_boolean_expression(generator, depth - 1),
                ],
            }
        return {
            "op": "implies",
            "left": AdversarialTests.random_boolean_expression(generator, depth - 1),
            "right": AdversarialTests.random_boolean_expression(generator, depth - 1),
        }

    def test_unsupported_operator_fails_closed(self) -> None:
        spec = copy.deepcopy(load_fixture("proved")["formal_spec"])
        spec["predicate"] = {"op": "execute_shell", "arg": {"literal": True}}

        candidate = FiniteSearchEngine(16).search(spec)
        result = FiniteDomainVerifier(16).verify(spec, candidate)

        self.assertEqual(candidate.kind, CandidateKind.UNKNOWN)
        self.assertEqual(result.outcome, VerificationOutcome.UNKNOWN)

    def test_domain_escape_in_witness_is_rejected(self) -> None:
        fixture = load_fixture("disproved")
        candidate = FiniteSearchEngine(16).search(fixture["formal_spec"])
        forged = replace(candidate, payload={"witness": {"p": True, "q": "False"}})

        result = FiniteDomainVerifier(16).verify(fixture["formal_spec"], forged)
        self.assertEqual(result.outcome, VerificationOutcome.REJECTED)
        self.assertEqual(result.details["reason"], "invalid_counterexample_witness")

    def test_search_budget_cannot_be_claimed_as_proof(self) -> None:
        fixture = load_fixture("unresolved")
        candidate = FiniteSearchEngine(8).search(fixture["formal_spec"])

        self.assertEqual(candidate.kind, CandidateKind.UNKNOWN)
        self.assertNotIn("truth_table_hash", candidate.payload)

    def test_deep_expression_is_bounded(self) -> None:
        expression = {"var": "p"}
        for _ in range(100):
            expression = {"op": "not", "arg": expression}
        spec = {
            "schema": "mathos.finite/v1",
            "quantifier": "forall",
            "variables": {"p": [False, True]},
            "predicate": expression,
        }

        candidate = FiniteSearchEngine(16).search(spec)
        self.assertEqual(candidate.kind, CandidateKind.UNKNOWN)
        self.assertEqual(candidate.payload["reason"], "invalid_formal_spec")

    def test_unreachable_ill_typed_branch_is_still_rejected(self) -> None:
        spec = {
            "schema": "mathos.finite/v1",
            "quantifier": "forall",
            "variables": {"p": [False, True]},
            "predicate": {
                "op": "or",
                "args": [
                    {"literal": True},
                    {"op": "add", "left": {"literal": 1}, "right": {"literal": 2}},
                ],
            },
        }

        candidate = FiniteSearchEngine(16).search(spec)
        self.assertEqual(candidate.kind, CandidateKind.UNKNOWN)
        self.assertEqual(candidate.payload["reason"], "invalid_formal_spec")

    def test_mixed_type_domain_is_rejected(self) -> None:
        spec = copy.deepcopy(load_fixture("proved")["formal_spec"])
        spec["variables"]["p"] = [False, 0]

        candidate = FiniteSearchEngine(16).search(spec)
        self.assertEqual(candidate.kind, CandidateKind.UNKNOWN)
        self.assertEqual(candidate.payload["reason"], "invalid_formal_spec")

    def test_excessive_integer_growth_is_rejected(self) -> None:
        large = 1 << 3_000
        spec = {
            "schema": "mathos.finite/v1",
            "quantifier": "forall",
            "variables": {"p": [False, True]},
            "predicate": {
                "op": "eq",
                "left": {
                    "op": "mul",
                    "left": {"literal": large},
                    "right": {"literal": large},
                },
                "right": {"literal": 0},
            },
        }

        candidate = FiniteSearchEngine(16).search(spec)
        self.assertEqual(candidate.kind, CandidateKind.UNKNOWN)
        self.assertEqual(candidate.payload["reason"], "invalid_formal_spec")

    def test_oversized_statement_is_rejected_before_persistence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            engine = ClaimEngine.open(Path(directory) / "mathos.db")
            try:
                with self.assertRaisesRegex(ValueError, "too long"):
                    engine.submit("x" * 100_001)
                self.assertEqual(engine.list_claims(), [])
            finally:
                engine.close()

    def test_sql_metacharacters_are_stored_as_data(self) -> None:
        fixture = load_fixture("proved")
        hostile = "'); DROP TABLE claims; --"
        with tempfile.TemporaryDirectory() as directory:
            engine = ClaimEngine.open(Path(directory) / "mathos.db")
            try:
                claim = engine.submit(hostile, fixture["formal_spec"])
                engine.process(claim.claim_id, max_assignments=16)
                stored = engine.get_claim(claim.claim_id)
                self.assertEqual(stored.informal_statement, hostile)
                self.assertEqual(len(engine.list_claims()), 1)
            finally:
                engine.close()

    def test_missing_lean_toolchain_fails_closed(self) -> None:
        result = LeanSubprocessVerifier().verify_file("Unavailable.lean")
        self.assertEqual(result.outcome, VerificationOutcome.UNKNOWN)
        self.assertEqual(result.details["reason"], "lean_toolchain_unavailable")

    def test_randomized_boolean_claims_match_independent_oracle(self) -> None:
        generator = random.Random(5601)
        assignments = [
            {"a": a, "b": b, "c": c}
            for a in (False, True)
            for b in (False, True)
            for c in (False, True)
        ]
        for _ in range(200):
            predicate = self.random_boolean_expression(generator, depth=5)
            spec = {
                "schema": "mathos.finite/v1",
                "quantifier": "forall",
                "variables": {
                    "a": [False, True],
                    "b": [False, True],
                    "c": [False, True],
                },
                "predicate": predicate,
            }
            expected = all(
                self.reference_boolean(predicate, assignment)
                for assignment in assignments
            )
            candidate = FiniteSearchEngine(8).search(spec)
            result = FiniteDomainVerifier(8).verify(spec, candidate)
            self.assertEqual(
                result.outcome,
                VerificationOutcome.PROVED
                if expected
                else VerificationOutcome.DISPROVED,
            )


if __name__ == "__main__":
    unittest.main()
