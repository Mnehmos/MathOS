from pathlib import Path
from typing import Any

from .canonical import canonical_json, hash_json
from .finite import FiniteDomainVerifier, FiniteSearchEngine
from .ledger import ProvenanceLedger
from .models import (
    ChainVerification,
    Claim,
    ClaimStatus,
    RunReport,
    VerificationOutcome,
)
from .pedagogy import generate_pedagogy


class ClaimEngine:
    MAX_STATEMENT_LENGTH = 100_000
    MAX_FORMAL_SPEC_BYTES = 1_000_000
    MAX_ASSIGNMENTS = 100_000

    def __init__(self, ledger: ProvenanceLedger) -> None:
        self.ledger = ledger

    @classmethod
    def open(cls, database: str | Path) -> "ClaimEngine":
        return cls(ProvenanceLedger(database))

    def close(self) -> None:
        self.ledger.close()

    def submit(
        self, informal_statement: str, formal_spec: dict[str, Any] | None = None
    ) -> Claim:
        if not isinstance(informal_statement, str) or not informal_statement.strip():
            raise ValueError("informal_statement must be a non-empty string")
        if len(informal_statement) > self.MAX_STATEMENT_LENGTH:
            raise ValueError("informal_statement is too long")
        if formal_spec is not None and not isinstance(formal_spec, dict):
            raise ValueError("formal_spec must be an object or null")
        if formal_spec is not None:
            try:
                formal_size = len(canonical_json(formal_spec).encode("utf-8"))
            except (TypeError, ValueError) as error:
                raise ValueError("formal_spec must contain JSON values") from error
            if formal_size > self.MAX_FORMAL_SPEC_BYTES:
                raise ValueError("formal_spec is too large")
        identity = {
            "informal_statement": informal_statement,
            "formal_spec": formal_spec,
        }
        claim_id = f"claim_{hash_json(identity)[:24]}"
        return self.ledger.insert_claim(
            claim_id, informal_statement, formal_spec
        )

    def get_claim(self, claim_id: str) -> Claim:
        return self.ledger.get_claim(claim_id)

    def list_claims(self) -> list[Claim]:
        return self.ledger.list_claims()

    def process(self, claim_id: str, *, max_assignments: int = 10_000) -> RunReport:
        if type(max_assignments) is not int or not 1 <= max_assignments <= self.MAX_ASSIGNMENTS:
            raise ValueError(
                f"max_assignments must be between 1 and {self.MAX_ASSIGNMENTS}"
            )
        claim = self.get_claim(claim_id)
        if claim.status in {
            ClaimStatus.VERIFIED_PROVED,
            ClaimStatus.VERIFIED_DISPROVED,
        }:
            raise ValueError("verified claims are immutable; submit a new claim version")

        if claim.formal_spec is None:
            from .models import CandidateKind, SearchCandidate

            candidate = SearchCandidate(
                CandidateKind.UNKNOWN, {"reason": "missing_formal_spec"}
            )
        else:
            candidate = FiniteSearchEngine(max_assignments).search(claim.formal_spec)

        verifier = FiniteDomainVerifier(max_assignments=max(max_assignments, 1))
        if claim.formal_spec is None:
            verification = verifier.verify({}, candidate)
        else:
            verification = verifier.verify(claim.formal_spec, candidate)

        status = {
            VerificationOutcome.PROVED: ClaimStatus.VERIFIED_PROVED,
            VerificationOutcome.DISPROVED: ClaimStatus.VERIFIED_DISPROVED,
            VerificationOutcome.UNKNOWN: ClaimStatus.UNRESOLVED,
            VerificationOutcome.REJECTED: ClaimStatus.UNRESOLVED,
        }[verification.outcome]
        pedagogy = generate_pedagogy(
            claim.informal_statement, status, verification
        )
        claim = self.ledger.record_run(
            claim_id,
            expected_status=claim.status,
            new_status=status,
            reason=verification.outcome.value,
            candidate=candidate.to_dict(),
            verification=verification.to_dict(),
            pedagogy=pedagogy,
        )
        return RunReport(
            claim=claim,
            candidate=candidate.to_dict(),
            verification=verification.to_dict(),
            pedagogy=pedagogy,
        )

    def verify_provenance(self) -> ChainVerification:
        return self.ledger.verify_chain()

    def export_trajectory(self, claim_id: str) -> dict[str, Any]:
        claim = self.get_claim(claim_id)
        if claim.status not in {
            ClaimStatus.VERIFIED_PROVED,
            ClaimStatus.VERIFIED_DISPROVED,
            ClaimStatus.UNRESOLVED,
        }:
            raise ValueError("cannot export a trajectory for a non-terminal claim")
        events = self.ledger.events_for_claim(claim_id)
        chain = self.verify_provenance()
        if not chain.valid:
            raise ValueError("cannot export from an invalid provenance ledger")
        trajectory = {
            "schema": "mathos.rl-trajectory/v1",
            "claim": claim.to_dict(),
            "outcome": claim.status.value,
            "steps": events,
            "provenance": chain.to_dict(),
        }
        return {**trajectory, "trajectory_hash": hash_json(trajectory)}
