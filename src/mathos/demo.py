from collections import Counter
import json
from pathlib import Path
from typing import Any

from .engine import ClaimEngine


CANONICAL_FIXTURES: tuple[dict[str, Any], ...] = (
    {
        "name": "excluded_middle",
        "informal_statement": "For every Boolean p, p or not p.",
        "formal_spec": {
            "schema": "mathos.finite/v1",
            "quantifier": "forall",
            "variables": {"p": [False, True]},
            "predicate": {
                "op": "or",
                "args": [
                    {"var": "p"},
                    {"op": "not", "arg": {"var": "p"}},
                ],
            },
        },
        "max_assignments": 16,
    },
    {
        "name": "universal_implication",
        "informal_statement": "For every pair of Booleans p and q, p implies q.",
        "formal_spec": {
            "schema": "mathos.finite/v1",
            "quantifier": "forall",
            "variables": {"p": [False, True], "q": [False, True]},
            "predicate": {
                "op": "implies",
                "left": {"var": "p"},
                "right": {"var": "q"},
            },
        },
        "max_assignments": 16,
    },
    {
        "name": "budget_limited_excluded_middle",
        "informal_statement": (
            "For every assignment to twelve Booleans, p0 or not p0."
        ),
        "formal_spec": {
            "schema": "mathos.finite/v1",
            "quantifier": "forall",
            "variables": {f"p{index}": [False, True] for index in range(12)},
            "predicate": {
                "op": "or",
                "args": [
                    {"var": "p0"},
                    {"op": "not", "arg": {"var": "p0"}},
                ],
            },
        },
        "max_assignments": 8,
    },
)


def run_demo(workspace: Path, *, reset: bool = False) -> dict[str, Any]:
    workspace.mkdir(parents=True, exist_ok=True)
    marker = workspace / ".mathos-demo"
    database = workspace / "mathos.db"
    exports = workspace / "exports"
    if reset:
        unknown_entries = [
            path for path in workspace.iterdir() if path.name not in {
                ".mathos-demo", "mathos.db", "mathos.db-shm", "mathos.db-wal", "exports"
            }
        ]
        if unknown_entries:
            raise ValueError("refusing to reset a non-demo directory")
        for path in (database, workspace / "mathos.db-shm", workspace / "mathos.db-wal"):
            if path.exists():
                path.unlink()
        if exports.exists():
            for path in exports.glob("claim_*.json"):
                path.unlink()
    marker.touch(exist_ok=True)
    exports.mkdir(exist_ok=True)

    engine = ClaimEngine.open(database)
    results: list[dict[str, Any]] = []
    exported: list[str] = []
    try:
        for fixture in CANONICAL_FIXTURES:
            claim = engine.submit(
                fixture["informal_statement"], fixture["formal_spec"]
            )
            report = engine.process(
                claim.claim_id, max_assignments=fixture["max_assignments"]
            )
            trajectory = engine.export_trajectory(claim.claim_id)
            output = exports / f"{claim.claim_id}.json"
            output.write_text(
                json.dumps(trajectory, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            results.append({"name": fixture["name"], **report.to_dict()})
            exported.append(str(output))
        chain = engine.verify_provenance()
    finally:
        engine.close()

    return {
        "schema": "mathos.demo/v1",
        "workspace": str(workspace),
        "summary": dict(Counter(item["claim"]["status"] for item in results)),
        "provenance_valid": chain.valid,
        "provenance_events": chain.events_checked,
        "chain_head": chain.chain_head,
        "exports": exported,
        "results": results,
    }
