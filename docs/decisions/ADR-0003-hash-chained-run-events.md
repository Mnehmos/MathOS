# ADR-0003: Hash-chained run events with anchored heads

Status: Accepted

Date: 2026-07-19

Issue: [#7](https://github.com/Mnehmos/MathOS/issues/7)

## Context

Research, formalization, audit, pedagogy, release, and migration work must survive process restarts without turning an execution transcript into mathematical authority. Failed attempts must remain visible. Concurrent writers must not silently construct two incompatible histories from one event head.

An event hash must be reproducible without local timestamps, database row numbers, or machine identity. Verification must also detect removal of the final event, which an event chain alone cannot reveal if no later event names the missing hash.

## Decision

Each run has a UUIDv7 identity, an exhaustive run kind, explicit actor, RFC 8785 canonical budget, lifecycle state, and an anchored event count and head hash.

Run creation atomically writes a `run_started` origin event. Generic append accepts only observational execution events. Origin and terminal lifecycle events can be emitted only by their controlled lifecycle operations. There is no event or mutation that marks a mathematical claim proved or disproved.

Event identity is:

```text
event_envelope = RFC8785({
  "run_id": run_id,
  "sequence": sequence,
  "event_type": event_type,
  "payload": payload,
  "actor": actor
})

event_hash = SHA256(previous_event_hash_utf8 || event_envelope)
```

The origin event omits the predecessor bytes. The predecessor is the 64-byte lowercase hexadecimal text returned for the prior hash, not its decoded 32-byte digest.

Event IDs and timestamps are excluded from identity. They describe storage and observation time, not semantic event content.

Every append supplies the expected head hash. An immediate SQLite transaction compares that head, writes the next contiguous event, and advances the run anchor. Reusing an idempotency key with the same input returns the original event. Reusing it with different input fails.

SQLite triggers reject event update and deletion, run deletion, run-origin mutation, unknown event kinds, sequence gaps, and predecessor mismatch. Chain verification recomputes every hash and compares the resulting count and head against the run anchor, so missing final events are detected as well as reordered or forged events.

## Trust boundary

A valid event chain establishes trace integrity for recorded actions. It does not establish:

- proof or refutation;
- source-statement fidelity;
- novelty;
- correctness of model output;
- correctness of an informal explanation.

Those conclusions require their own evidence records and authority policies.

## Consequences

- Independent readers can verify event order and content from SQLite state or a future portable release.
- Concurrent append conflicts are explicit and retryable.
- Failed attempts cannot be edited away through the application path.
- A trusted anchor is required to detect truncation. The run row supplies that anchor inside the operational store. Future releases must carry the run anchor in their immutable manifest.
- Lifecycle transitions require dedicated operations that atomically change state and append the matching event.

## Rejected alternatives

### Mutable JSON transcript

Rejected because a rewrite can erase failure history or change prior inputs without detection.

### Hash chain without a run anchor

Rejected because deleting the final event leaves the remaining prefix internally valid.

### Timestamp in the event hash

Rejected because timestamps do not define semantic content and complicate deterministic replay.

### Let generic append emit terminal events

Rejected because an event could claim a state transition that the run state machine never performed.
