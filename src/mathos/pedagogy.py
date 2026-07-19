from typing import Any

from .models import ClaimStatus, VerificationResult


def generate_pedagogy(
    informal_statement: str,
    status: ClaimStatus,
    verification: VerificationResult,
) -> dict[str, Any]:
    if status is ClaimStatus.VERIFIED_PROVED:
        checked = verification.details["assignments_checked"]
        summary = (
            f"The claim was proved in its finite formal domain. The independent "
            f"verifier checked all {checked} assignments and found no counterexample."
        )
        certainty = "verified"
    elif status is ClaimStatus.VERIFIED_DISPROVED:
        witness = verification.details["witness"]
        summary = (
            "The universal claim was disproved. The independent verifier checked "
            f"the counterexample witness {witness!r} and confirmed that the predicate is false."
        )
        certainty = "verified"
    else:
        reason = verification.details.get("reason", "no verified conclusion")
        summary = (
            "The claim remains unresolved. MathOS did not establish a proof or a "
            f"verified counterexample. Reason: {reason}."
        )
        certainty = "unresolved"
    return {
        "schema": "mathos.pedagogy/v1",
        "claim": informal_statement,
        "certainty": certainty,
        "summary": summary,
        "verification_evidence_hash": verification.evidence_hash,
    }
