import argparse
import json
from pathlib import Path
import sys
from typing import Any

from . import __version__
from .demo import run_demo
from .engine import ClaimEngine
from .trajectory import verify_trajectory


def _write(value: Any) -> None:
    sys.stdout.write(json.dumps(value, indent=2, sort_keys=True) + "\n")


def _read_json(path: str) -> dict[str, Any]:
    value = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError("formal specification must contain a JSON object")
    return value


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="mathos",
        description="Verifier-gated mathematical claim lifecycle",
    )
    parser.add_argument("--version", action="version", version=__version__)
    commands = parser.add_subparsers(dest="command", required=True)

    initialize = commands.add_parser("init", help="Initialize a MathOS ledger")
    initialize.add_argument("--db", required=True, type=Path)

    submit = commands.add_parser("submit", help="Submit a claim")
    submit.add_argument("--db", required=True, type=Path)
    submit.add_argument("--statement", required=True)
    submit.add_argument("--formal-file")

    run = commands.add_parser("run", help="Search and verify a claim")
    run.add_argument("--db", required=True, type=Path)
    run.add_argument("claim_id")
    run.add_argument("--max-assignments", type=int, default=10_000)

    show = commands.add_parser("show", help="Show a claim and its events")
    show.add_argument("--db", required=True, type=Path)
    show.add_argument("claim_id")

    listing = commands.add_parser("list", help="List claims")
    listing.add_argument("--db", required=True, type=Path)

    export = commands.add_parser("export", help="Export an RL trajectory")
    export.add_argument("--db", required=True, type=Path)
    export.add_argument("claim_id")
    export.add_argument("--output", required=True, type=Path)

    replay = commands.add_parser("replay", help="Verify the provenance chain")
    replay.add_argument("--db", required=True, type=Path)

    validate_export = commands.add_parser(
        "validate-export", help="Validate an exported RL trajectory"
    )
    validate_export.add_argument("--input", required=True, type=Path)

    demo = commands.add_parser("demo", help="Run the canonical 0-to-1 fixtures")
    demo.add_argument("--workspace", required=True, type=Path)
    demo.add_argument("--reset", action="store_true")
    return parser


def _engine_command(args: argparse.Namespace) -> dict[str, Any] | list[dict[str, Any]]:
    engine = ClaimEngine.open(args.db)
    try:
        if args.command == "init":
            return {"database": str(args.db), "initialized": True}
        if args.command == "submit":
            formal = _read_json(args.formal_file) if args.formal_file else None
            return engine.submit(args.statement, formal).to_dict()
        if args.command == "run":
            return engine.process(
                args.claim_id, max_assignments=args.max_assignments
            ).to_dict()
        if args.command == "show":
            return {
                "claim": engine.get_claim(args.claim_id).to_dict(),
                "events": engine.ledger.events_for_claim(args.claim_id),
            }
        if args.command == "list":
            return [claim.to_dict() for claim in engine.list_claims()]
        if args.command == "export":
            trajectory = engine.export_trajectory(args.claim_id)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(
                json.dumps(trajectory, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            return {
                "claim_id": args.claim_id,
                "output": str(args.output),
                "trajectory_hash": trajectory["trajectory_hash"],
            }
        if args.command == "replay":
            return engine.verify_provenance().to_dict()
        raise AssertionError(f"unhandled command: {args.command}")
    finally:
        engine.close()


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        result = (
            run_demo(args.workspace, reset=args.reset)
            if args.command == "demo"
            else verify_trajectory(
                json.loads(args.input.read_text(encoding="utf-8"))
            ).to_dict()
            if args.command == "validate-export"
            else _engine_command(args)
        )
        _write(result)
        if args.command == "validate-export" and not result["valid"]:
            return 1
        if args.command == "replay" and not result["valid"]:
            return 1
        return 0
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        _write({"error": type(error).__name__, "message": str(error)})
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
