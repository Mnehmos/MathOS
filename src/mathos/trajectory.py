from copy import deepcopy
from typing import Any

from .canonical import hash_json
from .finite import FiniteDomainVerifier
from .models import CandidateKind, SearchCandidate, TrajectoryVerification


def verify_trajectory(value: Any) -> TrajectoryVerification:
    if not isinstance(value, dict):
        return TrajectoryVerification(False, 0, None, "trajectory_must_be_an_object")
    supplied_hash = value.get("trajectory_hash")
    body = deepcopy(value)
    body.pop("trajectory_hash", None)
    if not isinstance(supplied_hash, str) or hash_json(body) != supplied_hash:
        return TrajectoryVerification(False, 0, supplied_hash, "trajectory_hash_mismatch")
    if value.get("schema") != "mathos.rl-trajectory/v1":
        return TrajectoryVerification(False, 0, supplied_hash, "unsupported_schema")

    claim = value.get("claim")
    steps = value.get("steps")
    if not isinstance(claim, dict) or not isinstance(claim.get("claim_id"), str):
        return TrajectoryVerification(False, 0, supplied_hash, "invalid_claim")
    if value.get("outcome") != claim.get("status"):
        return TrajectoryVerification(False, 0, supplied_hash, "outcome_status_mismatch")
    if not isinstance(steps, list) or not steps:
        return TrajectoryVerification(False, 0, supplied_hash, "missing_steps")

    event_types = [
        event.get("event_type") if isinstance(event, dict) else None
        for event in steps
    ]
    cycle = [
        "search.completed",
        "verification.completed",
        "claim.status_changed",
        "pedagogy.generated",
    ]
    if (
        event_types[0] != "claim.submitted"
        or len(event_types) < 5
        or event_types[1:] != cycle * ((len(event_types) - 1) // len(cycle))
    ):
        return TrajectoryVerification(
            False, 0, supplied_hash, "lifecycle_order_invalid"
        )

    previous_sequence = 0
    for index, event in enumerate(steps, start=1):
        if not isinstance(event, dict):
            return TrajectoryVerification(False, index - 1, supplied_hash, "invalid_event")
        required = {
            "sequence",
            "event_id",
            "claim_id",
            "event_type",
            "payload",
            "previous_hash",
            "event_hash",
            "created_at",
        }
        if set(event) != required or event["claim_id"] != claim["claim_id"]:
            return TrajectoryVerification(False, index - 1, supplied_hash, "invalid_event")
        if type(event["sequence"]) is not int or event["sequence"] <= previous_sequence:
            return TrajectoryVerification(False, index - 1, supplied_hash, "event_order_invalid")
        event_body = {key: event[key] for key in required if key != "event_hash"}
        if hash_json(event_body) != event["event_hash"]:
            return TrajectoryVerification(
                False, index - 1, supplied_hash, "event_hash_mismatch"
            )
        previous_sequence = event["sequence"]
    submitted = steps[0]
    submitted_payload = submitted["payload"]
    if not isinstance(submitted_payload, dict):
        return TrajectoryVerification(False, 1, supplied_hash, "invalid_submission")
    initial_status = "formalized" if claim.get("formal_spec") is not None else "ingested"
    if (
        submitted_payload.get("informal_statement") != claim.get("informal_statement")
        or submitted_payload.get("formal_spec") != claim.get("formal_spec")
        or submitted_payload.get("initial_status") != initial_status
        or submitted.get("created_at") != claim.get("created_at")
    ):
        return TrajectoryVerification(False, 1, supplied_hash, "invalid_submission")

    current_status = initial_status
    updated_at = submitted["created_at"]
    outcome_to_status = {
        "proved": "verified_proved",
        "disproved": "verified_disproved",
        "unknown": "unresolved",
        "rejected": "unresolved",
    }
    for offset in range(1, len(steps), 4):
        search_event = steps[offset]
        verification_event = steps[offset + 1]
        status_event = steps[offset + 2]
        pedagogy_event = steps[offset + 3]
        verification = verification_event["payload"]
        status_change = status_event["payload"]
        pedagogy = pedagogy_event["payload"]
        if not all(isinstance(payload, dict) for payload in (
            verification, status_change, pedagogy
        )):
            return TrajectoryVerification(
                False, offset, supplied_hash, "invalid_lifecycle_payload"
            )
        candidate_payload = search_event["payload"]
        try:
            if (
                not isinstance(candidate_payload, dict)
                or set(candidate_payload) != {"kind", "payload", "search_engine"}
                or not isinstance(candidate_payload["payload"], dict)
                or candidate_payload["search_engine"] != "finite-search-v1"
            ):
                raise ValueError("invalid candidate")
            candidate = SearchCandidate(
                kind=CandidateKind(candidate_payload["kind"]),
                payload=candidate_payload["payload"],
                search_engine=candidate_payload["search_engine"],
            )
        except (KeyError, TypeError, ValueError):
            return TrajectoryVerification(
                False, offset, supplied_hash, "invalid_search_candidate"
            )
        evidence_body = {
            "outcome": verification.get("outcome"),
            "verifier": verification.get("verifier"),
            "verifier_version": verification.get("verifier_version"),
            "details": verification.get("details"),
        }
        if hash_json(evidence_body) != verification.get("evidence_hash"):
            return TrajectoryVerification(
                False, offset + 1, supplied_hash, "verification_evidence_mismatch"
            )
        formal_spec = claim.get("formal_spec")
        replayed = FiniteDomainVerifier().verify(
            formal_spec if isinstance(formal_spec, dict) else {}, candidate
        )
        if replayed.to_dict() != verification:
            return TrajectoryVerification(
                False, offset + 1, supplied_hash, "candidate_verification_mismatch"
            )
        expected_status = outcome_to_status.get(verification.get("outcome"))
        if (
            expected_status is None
            or status_change.get("from") != current_status
            or status_change.get("to") != expected_status
            or status_change.get("reason") != verification.get("outcome")
        ):
            return TrajectoryVerification(
                False, offset + 2, supplied_hash, "status_evidence_mismatch"
            )
        if pedagogy.get("verification_evidence_hash") != verification.get(
            "evidence_hash"
        ):
            return TrajectoryVerification(
                False, offset + 3, supplied_hash, "pedagogy_evidence_mismatch"
            )
        current_status = expected_status
        updated_at = status_event["created_at"]

    if current_status != value["outcome"] or updated_at != claim.get("updated_at"):
        return TrajectoryVerification(
            False, len(steps), supplied_hash, "verification_outcome_mismatch"
        )
    return TrajectoryVerification(True, len(steps), supplied_hash)
