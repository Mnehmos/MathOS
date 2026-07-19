from dataclasses import dataclass
from enum import StrEnum
from typing import Any


class ClaimStatus(StrEnum):
    INGESTED = "ingested"
    FORMALIZED = "formalized"
    VERIFIED_PROVED = "verified_proved"
    VERIFIED_DISPROVED = "verified_disproved"
    UNRESOLVED = "unresolved"


class CandidateKind(StrEnum):
    ENUMERATION_PROOF = "enumeration_proof"
    COUNTEREXAMPLE = "counterexample"
    UNKNOWN = "unknown"


class VerificationOutcome(StrEnum):
    PROVED = "proved"
    DISPROVED = "disproved"
    UNKNOWN = "unknown"
    REJECTED = "rejected"


class StateTransitionError(ValueError):
    pass


ALLOWED_TRANSITIONS: dict[ClaimStatus, set[ClaimStatus]] = {
    ClaimStatus.INGESTED: {ClaimStatus.FORMALIZED, ClaimStatus.UNRESOLVED},
    ClaimStatus.FORMALIZED: {
        ClaimStatus.VERIFIED_PROVED,
        ClaimStatus.VERIFIED_DISPROVED,
        ClaimStatus.UNRESOLVED,
    },
    ClaimStatus.UNRESOLVED: {
        ClaimStatus.VERIFIED_PROVED,
        ClaimStatus.VERIFIED_DISPROVED,
        ClaimStatus.UNRESOLVED,
    },
    ClaimStatus.VERIFIED_PROVED: set(),
    ClaimStatus.VERIFIED_DISPROVED: set(),
}


@dataclass(frozen=True, slots=True)
class Claim:
    claim_id: str
    informal_statement: str
    formal_spec: dict[str, Any] | None
    status: ClaimStatus
    created_at: str
    updated_at: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "claim_id": self.claim_id,
            "informal_statement": self.informal_statement,
            "formal_spec": self.formal_spec,
            "status": self.status.value,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        }


@dataclass(frozen=True, slots=True)
class SearchCandidate:
    kind: CandidateKind
    payload: dict[str, Any]
    search_engine: str = "finite-search-v1"

    def to_dict(self) -> dict[str, Any]:
        return {
            "kind": self.kind.value,
            "payload": self.payload,
            "search_engine": self.search_engine,
        }


@dataclass(frozen=True, slots=True)
class VerificationResult:
    outcome: VerificationOutcome
    verifier: str
    verifier_version: str
    details: dict[str, Any]
    evidence_hash: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "outcome": self.outcome.value,
            "verifier": self.verifier,
            "verifier_version": self.verifier_version,
            "details": self.details,
            "evidence_hash": self.evidence_hash,
        }


@dataclass(frozen=True, slots=True)
class RunReport:
    claim: Claim
    candidate: dict[str, Any]
    verification: dict[str, Any]
    pedagogy: dict[str, Any]

    def to_dict(self) -> dict[str, Any]:
        return {
            "claim": self.claim.to_dict(),
            "candidate": self.candidate,
            "verification": self.verification,
            "pedagogy": self.pedagogy,
        }


@dataclass(frozen=True, slots=True)
class ChainVerification:
    valid: bool
    events_checked: int
    chain_head: str | None
    broken_sequence: int | None = None
    reason: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "valid": self.valid,
            "events_checked": self.events_checked,
            "chain_head": self.chain_head,
            "broken_sequence": self.broken_sequence,
            "reason": self.reason,
        }


@dataclass(frozen=True, slots=True)
class TrajectoryVerification:
    valid: bool
    events_checked: int
    trajectory_hash: str | None
    reason: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "valid": self.valid,
            "events_checked": self.events_checked,
            "trajectory_hash": self.trajectory_hash,
            "reason": self.reason,
        }
