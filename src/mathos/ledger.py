from datetime import UTC, datetime
import json
from pathlib import Path
import sqlite3
from typing import Any
from uuid import uuid4

from .canonical import canonical_json, hash_json
from .models import (
    ALLOWED_TRANSITIONS,
    ChainVerification,
    Claim,
    ClaimStatus,
    StateTransitionError,
)


def utc_now() -> str:
    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


class ProvenanceLedger:
    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._connection = sqlite3.connect(self.path)
        self._connection.row_factory = sqlite3.Row
        self._connection.execute("PRAGMA foreign_keys = ON")
        self._connection.execute("PRAGMA journal_mode = WAL")
        self._initialize()

    def _initialize(self) -> None:
        self._connection.executescript(
            """
            CREATE TABLE IF NOT EXISTS claims (
                claim_id TEXT PRIMARY KEY,
                informal_statement TEXT NOT NULL,
                formal_spec_json TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                sequence INTEGER PRIMARY KEY,
                event_id TEXT NOT NULL UNIQUE,
                claim_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                previous_hash TEXT,
                event_hash TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                FOREIGN KEY (claim_id) REFERENCES claims(claim_id)
            );

            CREATE INDEX IF NOT EXISTS events_claim_id
                ON events(claim_id, sequence);
            """
        )

    def close(self) -> None:
        self._connection.close()

    def _claim_from_row(self, row: sqlite3.Row) -> Claim:
        return Claim(
            claim_id=row["claim_id"],
            informal_statement=row["informal_statement"],
            formal_spec=(
                json.loads(row["formal_spec_json"])
                if row["formal_spec_json"] is not None
                else None
            ),
            status=ClaimStatus(row["status"]),
            created_at=row["created_at"],
            updated_at=row["updated_at"],
        )

    def get_claim(self, claim_id: str) -> Claim:
        row = self._connection.execute(
            "SELECT * FROM claims WHERE claim_id = ?", (claim_id,)
        ).fetchone()
        if row is None:
            raise KeyError(f"unknown claim: {claim_id}")
        return self._claim_from_row(row)

    def list_claims(self) -> list[Claim]:
        rows = self._connection.execute(
            "SELECT * FROM claims ORDER BY created_at, claim_id"
        ).fetchall()
        return [self._claim_from_row(row) for row in rows]

    def insert_claim(
        self,
        claim_id: str,
        informal_statement: str,
        formal_spec: dict[str, Any] | None,
    ) -> Claim:
        existing = self._connection.execute(
            "SELECT * FROM claims WHERE claim_id = ?", (claim_id,)
        ).fetchone()
        if existing is not None:
            claim = self._claim_from_row(existing)
            if (
                claim.informal_statement != informal_statement
                or claim.formal_spec != formal_spec
            ):
                raise ValueError("claim identifier collision")
            return claim

        status = ClaimStatus.FORMALIZED if formal_spec is not None else ClaimStatus.INGESTED
        created_at = utc_now()
        formal_json = canonical_json(formal_spec) if formal_spec is not None else None
        try:
            self._connection.execute("BEGIN IMMEDIATE")
            self._connection.execute(
                """
                INSERT INTO claims (
                    claim_id, informal_statement, formal_spec_json,
                    status, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                """,
                (
                    claim_id,
                    informal_statement,
                    formal_json,
                    status.value,
                    created_at,
                    created_at,
                ),
            )
            self._append_event_in_transaction(
                claim_id,
                "claim.submitted",
                {
                    "informal_statement": informal_statement,
                    "formal_spec": formal_spec,
                    "initial_status": status.value,
                },
                created_at=created_at,
            )
            self._connection.commit()
        except Exception:
            self._connection.rollback()
            raise
        return self.get_claim(claim_id)

    def transition_status(
        self, claim_id: str, new_status: ClaimStatus, *, reason: str
    ) -> Claim:
        current = self.get_claim(claim_id)
        if new_status not in ALLOWED_TRANSITIONS[current.status]:
            raise StateTransitionError(
                f"invalid claim transition: {current.status.value} -> {new_status.value}"
            )
        updated_at = utc_now()
        try:
            self._connection.execute("BEGIN IMMEDIATE")
            changed = self._connection.execute(
                """
                UPDATE claims
                SET status = ?, updated_at = ?
                WHERE claim_id = ? AND status = ?
                """,
                (new_status.value, updated_at, claim_id, current.status.value),
            ).rowcount
            if changed != 1:
                raise StateTransitionError("claim changed concurrently")
            self._append_event_in_transaction(
                claim_id,
                "claim.status_changed",
                {
                    "from": current.status.value,
                    "to": new_status.value,
                    "reason": reason,
                },
                created_at=updated_at,
            )
            self._connection.commit()
        except Exception:
            self._connection.rollback()
            raise
        return self.get_claim(claim_id)

    def append_event(
        self, claim_id: str, event_type: str, payload: dict[str, Any]
    ) -> dict[str, Any]:
        self.get_claim(claim_id)
        try:
            self._connection.execute("BEGIN IMMEDIATE")
            event = self._append_event_in_transaction(claim_id, event_type, payload)
            self._connection.commit()
            return event
        except Exception:
            self._connection.rollback()
            raise

    def record_run(
        self,
        claim_id: str,
        *,
        expected_status: ClaimStatus,
        new_status: ClaimStatus,
        reason: str,
        candidate: dict[str, Any],
        verification: dict[str, Any],
        pedagogy: dict[str, Any],
    ) -> Claim:
        if new_status not in ALLOWED_TRANSITIONS[expected_status]:
            raise StateTransitionError(
                f"invalid claim transition: {expected_status.value} -> {new_status.value}"
            )
        try:
            self._connection.execute("BEGIN IMMEDIATE")
            row = self._connection.execute(
                "SELECT status FROM claims WHERE claim_id = ?", (claim_id,)
            ).fetchone()
            if row is None:
                raise KeyError(f"unknown claim: {claim_id}")
            if ClaimStatus(row["status"]) is not expected_status:
                raise StateTransitionError("claim changed concurrently")

            self._append_event_in_transaction(
                claim_id, "search.completed", candidate
            )
            self._append_event_in_transaction(
                claim_id, "verification.completed", verification
            )
            updated_at = utc_now()
            changed = self._connection.execute(
                """
                UPDATE claims
                SET status = ?, updated_at = ?
                WHERE claim_id = ? AND status = ?
                """,
                (
                    new_status.value,
                    updated_at,
                    claim_id,
                    expected_status.value,
                ),
            ).rowcount
            if changed != 1:
                raise StateTransitionError("claim changed concurrently")
            self._append_event_in_transaction(
                claim_id,
                "claim.status_changed",
                {
                    "from": expected_status.value,
                    "to": new_status.value,
                    "reason": reason,
                },
                created_at=updated_at,
            )
            self._append_event_in_transaction(
                claim_id, "pedagogy.generated", pedagogy
            )
            self._connection.commit()
        except Exception:
            self._connection.rollback()
            raise
        return self.get_claim(claim_id)

    def _append_event_in_transaction(
        self,
        claim_id: str,
        event_type: str,
        payload: dict[str, Any],
        *,
        created_at: str | None = None,
    ) -> dict[str, Any]:
        last = self._connection.execute(
            "SELECT sequence, event_hash FROM events ORDER BY sequence DESC LIMIT 1"
        ).fetchone()
        sequence = 1 if last is None else int(last["sequence"]) + 1
        previous_hash = None if last is None else last["event_hash"]
        event_id = f"event_{uuid4().hex}"
        timestamp = created_at or utc_now()
        body = {
            "sequence": sequence,
            "event_id": event_id,
            "claim_id": claim_id,
            "event_type": event_type,
            "payload": payload,
            "previous_hash": previous_hash,
            "created_at": timestamp,
        }
        event_hash = hash_json(body)
        self._connection.execute(
            """
            INSERT INTO events (
                sequence, event_id, claim_id, event_type, payload_json,
                previous_hash, event_hash, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                sequence,
                event_id,
                claim_id,
                event_type,
                canonical_json(payload),
                previous_hash,
                event_hash,
                timestamp,
            ),
        )
        return {**body, "event_hash": event_hash}

    def events_for_claim(self, claim_id: str) -> list[dict[str, Any]]:
        self.get_claim(claim_id)
        rows = self._connection.execute(
            "SELECT * FROM events WHERE claim_id = ? ORDER BY sequence", (claim_id,)
        ).fetchall()
        return [self._event_from_row(row) for row in rows]

    def _event_from_row(self, row: sqlite3.Row) -> dict[str, Any]:
        return {
            "sequence": row["sequence"],
            "event_id": row["event_id"],
            "claim_id": row["claim_id"],
            "event_type": row["event_type"],
            "payload": json.loads(row["payload_json"]),
            "previous_hash": row["previous_hash"],
            "event_hash": row["event_hash"],
            "created_at": row["created_at"],
        }

    def verify_chain(self) -> ChainVerification:
        rows = self._connection.execute(
            "SELECT * FROM events ORDER BY sequence"
        ).fetchall()
        expected_previous: str | None = None
        expected_sequence = 1
        for row in rows:
            event = self._event_from_row(row)
            body = {key: event[key] for key in (
                "sequence",
                "event_id",
                "claim_id",
                "event_type",
                "payload",
                "previous_hash",
                "created_at",
            )}
            if event["sequence"] != expected_sequence:
                return ChainVerification(
                    False,
                    expected_sequence - 1,
                    expected_previous,
                    event["sequence"],
                    "non_contiguous_sequence",
                )
            if event["previous_hash"] != expected_previous:
                return ChainVerification(
                    False,
                    expected_sequence - 1,
                    expected_previous,
                    event["sequence"],
                    "previous_hash_mismatch",
                )
            if hash_json(body) != event["event_hash"]:
                return ChainVerification(
                    False,
                    expected_sequence - 1,
                    expected_previous,
                    event["sequence"],
                    "event_hash_mismatch",
                )
            expected_previous = event["event_hash"]
            expected_sequence += 1

        projections: dict[str, dict[str, Any]] = {}
        for row in rows:
            event = self._event_from_row(row)
            claim_id = event["claim_id"]
            payload = event["payload"]
            if event["event_type"] == "claim.submitted":
                try:
                    initial = ClaimStatus(payload["initial_status"])
                    informal = payload["informal_statement"]
                    formal = payload["formal_spec"]
                except (KeyError, TypeError, ValueError):
                    return ChainVerification(
                        False, len(rows), expected_previous, event["sequence"],
                        "claim_projection_invalid",
                    )
                expected_initial = (
                    ClaimStatus.FORMALIZED if formal is not None else ClaimStatus.INGESTED
                )
                if claim_id in projections or initial is not expected_initial:
                    return ChainVerification(
                        False, len(rows), expected_previous, event["sequence"],
                        "claim_projection_invalid",
                    )
                projections[claim_id] = {
                    "informal_statement": informal,
                    "formal_spec": formal,
                    "status": initial,
                    "created_at": event["created_at"],
                    "updated_at": event["created_at"],
                }
                continue
            if claim_id not in projections:
                return ChainVerification(
                    False, len(rows), expected_previous, event["sequence"],
                    "claim_projection_invalid",
                )
            if event["event_type"] == "claim.status_changed":
                projection = projections[claim_id]
                try:
                    old_status = ClaimStatus(payload["from"])
                    new_status = ClaimStatus(payload["to"])
                except (KeyError, TypeError, ValueError):
                    return ChainVerification(
                        False, len(rows), expected_previous, event["sequence"],
                        "claim_projection_invalid",
                    )
                if (
                    projection["status"] is not old_status
                    or new_status not in ALLOWED_TRANSITIONS[old_status]
                ):
                    return ChainVerification(
                        False, len(rows), expected_previous, event["sequence"],
                        "claim_projection_invalid",
                    )
                projection["status"] = new_status
                projection["updated_at"] = event["created_at"]

        claim_rows = self._connection.execute("SELECT * FROM claims").fetchall()
        if {row["claim_id"] for row in claim_rows} != set(projections):
            return ChainVerification(
                False, len(rows), expected_previous, None, "claim_projection_mismatch"
            )
        for row in claim_rows:
            try:
                stored = self._claim_from_row(row)
            except (json.JSONDecodeError, KeyError, ValueError):
                return ChainVerification(
                    False, len(rows), expected_previous, None,
                    "claim_projection_mismatch",
                )
            projection = projections[stored.claim_id]
            if (
                stored.informal_statement != projection["informal_statement"]
                or stored.formal_spec != projection["formal_spec"]
                or stored.status is not projection["status"]
                or stored.created_at != projection["created_at"]
                or stored.updated_at != projection["updated_at"]
            ):
                return ChainVerification(
                    False, len(rows), expected_previous, None,
                    "claim_projection_mismatch",
                )
        return ChainVerification(True, len(rows), expected_previous)
