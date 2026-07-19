from collections.abc import Iterator
from itertools import product
import re
from typing import Any

from .canonical import hash_json
from .models import (
    CandidateKind,
    SearchCandidate,
    VerificationOutcome,
    VerificationResult,
)


SCHEMA = "mathos.finite/v1"
_VARIABLE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]{0,63}$")
_MAX_VARIABLES = 32
_MAX_DOMAIN_SIZE = 256
_MAX_EXPRESSION_DEPTH = 64
_MAX_EXPRESSION_NODES = 2_048
_MAX_INTEGER_BITS = 4_096
_MAX_STRING_LENGTH = 16_384


class FormalSpecError(ValueError):
    pass


def _same_json_value(left: Any, right: Any) -> bool:
    return type(left) is type(right) and left == right


def _is_scalar(value: Any) -> bool:
    if value is None or type(value) is bool:
        return True
    if type(value) is int:
        return value.bit_length() <= _MAX_INTEGER_BITS
    if type(value) is str:
        return len(value) <= _MAX_STRING_LENGTH
    return False


def _bounded_int(value: int, context: str) -> int:
    if value.bit_length() > _MAX_INTEGER_BITS:
        raise FormalSpecError(f"{context} exceeds the integer size limit")
    return value


def _require_bool(value: Any, context: str) -> bool:
    if type(value) is not bool:
        raise FormalSpecError(f"{context} must evaluate to a Boolean")
    return value


def _require_int(value: Any, context: str) -> int:
    if type(value) is not int:
        raise FormalSpecError(f"{context} must evaluate to an integer")
    return value


def validate_spec(spec: dict[str, Any]) -> tuple[list[str], dict[str, list[Any]]]:
    if not isinstance(spec, dict):
        raise FormalSpecError("formal specification must be an object")
    if spec.get("schema") != SCHEMA:
        raise FormalSpecError(f"unsupported schema: {spec.get('schema')!r}")
    if spec.get("quantifier") != "forall":
        raise FormalSpecError("only the forall quantifier is supported")

    variables = spec.get("variables")
    if not isinstance(variables, dict) or not variables:
        raise FormalSpecError("variables must be a non-empty object")
    if len(variables) > _MAX_VARIABLES:
        raise FormalSpecError("too many variables")

    normalized: dict[str, list[Any]] = {}
    for name, domain in variables.items():
        if not isinstance(name, str) or not _VARIABLE.fullmatch(name):
            raise FormalSpecError(f"invalid variable name: {name!r}")
        if not isinstance(domain, list) or not domain:
            raise FormalSpecError(f"domain for {name} must be a non-empty array")
        if len(domain) > _MAX_DOMAIN_SIZE:
            raise FormalSpecError(f"domain for {name} is too large")
        if not all(_is_scalar(value) for value in domain):
            raise FormalSpecError(f"domain for {name} contains unsupported values")
        if any(type(value) is not type(domain[0]) for value in domain[1:]):
            raise FormalSpecError(f"domain for {name} mixes value types")
        for index, value in enumerate(domain):
            if any(_same_json_value(value, prior) for prior in domain[:index]):
                raise FormalSpecError(f"domain for {name} contains duplicate values")
        normalized[name] = list(domain)

    predicate = spec.get("predicate")
    _validate_expression(predicate, set(normalized), depth=0, counter=[0])
    variable_types = {name: type(domain[0]) for name, domain in normalized.items()}
    if _infer_type(predicate, variable_types) is not bool:
        raise FormalSpecError("predicate must have Boolean type")
    return sorted(normalized), normalized


def _infer_type(expression: dict[str, Any], variables: dict[str, type]) -> type:
    if "var" in expression:
        return variables[expression["var"]]
    if "literal" in expression:
        return type(expression["literal"])

    operation = expression["op"]
    if operation == "not":
        if _infer_type(expression["arg"], variables) is not bool:
            raise FormalSpecError("not requires a Boolean arg")
        return bool
    if operation in {"and", "or"}:
        if any(_infer_type(arg, variables) is not bool for arg in expression["args"]):
            raise FormalSpecError(f"{operation} requires Boolean args")
        return bool

    left = _infer_type(expression["left"], variables)
    right = _infer_type(expression["right"], variables)
    if operation == "implies":
        if left is not bool or right is not bool:
            raise FormalSpecError("implies requires Boolean operands")
        return bool
    if operation in {"eq", "ne"}:
        if left is not right:
            raise FormalSpecError(f"{operation} requires operands of the same type")
        return bool
    if operation in {"lt", "lte", "gt", "gte", "add", "sub", "mul"}:
        if left is not int or right is not int:
            raise FormalSpecError(f"{operation} requires integer operands")
        return int if operation in {"add", "sub", "mul"} else bool
    raise FormalSpecError(f"unsupported operation: {operation!r}")


def _validate_expression(
    expression: Any,
    variables: set[str],
    *,
    depth: int,
    counter: list[int],
) -> None:
    if depth > _MAX_EXPRESSION_DEPTH:
        raise FormalSpecError("expression depth limit exceeded")
    counter[0] += 1
    if counter[0] > _MAX_EXPRESSION_NODES:
        raise FormalSpecError("expression node limit exceeded")
    if not isinstance(expression, dict):
        raise FormalSpecError("each expression must be an object")

    if "var" in expression:
        if set(expression) != {"var"} or expression["var"] not in variables:
            raise FormalSpecError("invalid variable expression")
        return
    if "literal" in expression:
        if set(expression) != {"literal"} or not _is_scalar(expression["literal"]):
            raise FormalSpecError("invalid literal expression")
        return

    operation = expression.get("op")
    if operation == "not":
        if set(expression) != {"op", "arg"}:
            raise FormalSpecError("not requires exactly one arg")
        _validate_expression(expression["arg"], variables, depth=depth + 1, counter=counter)
        return
    if operation in {"and", "or"}:
        if set(expression) != {"op", "args"}:
            raise FormalSpecError(f"{operation} requires args")
        arguments = expression["args"]
        if not isinstance(arguments, list) or not arguments:
            raise FormalSpecError(f"{operation} args must be a non-empty array")
        for argument in arguments:
            _validate_expression(argument, variables, depth=depth + 1, counter=counter)
        return
    if operation == "implies":
        expected = {"op", "left", "right"}
    elif operation in {"eq", "ne", "lt", "lte", "gt", "gte", "add", "sub", "mul"}:
        expected = {"op", "left", "right"}
    else:
        raise FormalSpecError(f"unsupported operation: {operation!r}")
    if set(expression) != expected:
        raise FormalSpecError(f"{operation} requires left and right")
    _validate_expression(expression["left"], variables, depth=depth + 1, counter=counter)
    _validate_expression(expression["right"], variables, depth=depth + 1, counter=counter)


def evaluate(expression: dict[str, Any], assignment: dict[str, Any]) -> Any:
    if "var" in expression:
        return assignment[expression["var"]]
    if "literal" in expression:
        return expression["literal"]

    operation = expression["op"]
    if operation == "not":
        return not _require_bool(evaluate(expression["arg"], assignment), "not arg")
    if operation == "and":
        values = [
            _require_bool(evaluate(arg, assignment), "and arg")
            for arg in expression["args"]
        ]
        return all(values)
    if operation == "or":
        values = [
            _require_bool(evaluate(arg, assignment), "or arg")
            for arg in expression["args"]
        ]
        return any(values)

    left = evaluate(expression["left"], assignment)
    right = evaluate(expression["right"], assignment)
    if operation == "implies":
        left_bool = _require_bool(left, "implies left")
        right_bool = _require_bool(right, "implies right")
        return (not left_bool) or right_bool
    if operation == "eq":
        return _same_json_value(left, right)
    if operation == "ne":
        return not _same_json_value(left, right)
    if operation in {"lt", "lte", "gt", "gte"}:
        left_int = _require_int(left, f"{operation} left")
        right_int = _require_int(right, f"{operation} right")
        return {
            "lt": left_int < right_int,
            "lte": left_int <= right_int,
            "gt": left_int > right_int,
            "gte": left_int >= right_int,
        }[operation]
    left_int = _require_int(left, f"{operation} left")
    right_int = _require_int(right, f"{operation} right")
    if operation == "add":
        return _bounded_int(left_int + right_int, "add result")
    if operation == "sub":
        return _bounded_int(left_int - right_int, "sub result")
    if operation == "mul":
        return _bounded_int(left_int * right_int, "mul result")
    raise FormalSpecError(f"unsupported operation: {operation!r}")


def assignment_count(names: list[str], domains: dict[str, list[Any]]) -> int:
    total = 1
    for name in names:
        total *= len(domains[name])
    return total


def assignments(names: list[str], domains: dict[str, list[Any]]) -> Iterator[dict[str, Any]]:
    for values in product(*(domains[name] for name in names)):
        yield dict(zip(names, values, strict=True))


def _unknown(reason: str, **details: Any) -> SearchCandidate:
    return SearchCandidate(CandidateKind.UNKNOWN, {"reason": reason, **details})


class FiniteSearchEngine:
    def __init__(self, max_assignments: int = 10_000) -> None:
        if max_assignments < 1:
            raise ValueError("max_assignments must be positive")
        self.max_assignments = max_assignments

    def search(self, spec: dict[str, Any]) -> SearchCandidate:
        try:
            names, domains = validate_spec(spec)
            total = assignment_count(names, domains)
            rows: list[dict[str, Any]] = []
            for index, assignment in enumerate(assignments(names, domains)):
                if index >= self.max_assignments:
                    return _unknown(
                        "assignment_budget_exhausted",
                        assignments_checked=index,
                        total_assignments=total,
                        max_assignments=self.max_assignments,
                    )
                truth = _require_bool(
                    evaluate(spec["predicate"], assignment), "predicate"
                )
                rows.append({"assignment": assignment, "result": truth})
                if not truth:
                    return SearchCandidate(
                        CandidateKind.COUNTEREXAMPLE,
                        {
                            "witness": assignment,
                            "assignments_checked": index + 1,
                            "total_assignments": total,
                        },
                    )
            return SearchCandidate(
                CandidateKind.ENUMERATION_PROOF,
                {
                    "assignments_checked": len(rows),
                    "total_assignments": total,
                    "truth_table_hash": hash_json(rows),
                },
            )
        except (FormalSpecError, KeyError, TypeError, ValueError) as error:
            return _unknown("invalid_formal_spec", error=str(error))


class FiniteDomainVerifier:
    name = "finite-domain-v1"
    version = "1.0.0"

    def __init__(self, max_assignments: int = 100_000) -> None:
        if max_assignments < 1:
            raise ValueError("max_assignments must be positive")
        self.max_assignments = max_assignments

    def _result(
        self, outcome: VerificationOutcome, details: dict[str, Any]
    ) -> VerificationResult:
        evidence = {
            "outcome": outcome.value,
            "verifier": self.name,
            "verifier_version": self.version,
            "details": details,
        }
        return VerificationResult(
            outcome=outcome,
            verifier=self.name,
            verifier_version=self.version,
            details=details,
            evidence_hash=hash_json(evidence),
        )

    def verify(
        self, spec: dict[str, Any], candidate: SearchCandidate
    ) -> VerificationResult:
        if candidate.kind is CandidateKind.UNKNOWN:
            return self._result(VerificationOutcome.UNKNOWN, dict(candidate.payload))
        try:
            names, domains = validate_spec(spec)
            total = assignment_count(names, domains)
        except (FormalSpecError, KeyError, TypeError, ValueError) as error:
            return self._result(
                VerificationOutcome.UNKNOWN,
                {"reason": "invalid_formal_spec", "error": str(error)},
            )

        if candidate.kind is CandidateKind.COUNTEREXAMPLE:
            return self._verify_counterexample(spec, candidate, names, domains)
        if candidate.kind is CandidateKind.ENUMERATION_PROOF:
            if total > self.max_assignments:
                return self._result(
                    VerificationOutcome.UNKNOWN,
                    {
                        "reason": "verifier_budget_exceeded",
                        "total_assignments": total,
                        "max_assignments": self.max_assignments,
                    },
                )
            return self._verify_enumeration(spec, candidate, names, domains, total)
        return self._result(
            VerificationOutcome.REJECTED,
            {"reason": "unsupported_candidate_kind"},
        )

    def _verify_counterexample(
        self,
        spec: dict[str, Any],
        candidate: SearchCandidate,
        names: list[str],
        domains: dict[str, list[Any]],
    ) -> VerificationResult:
        witness = candidate.payload.get("witness")
        if not isinstance(witness, dict) or set(witness) != set(names):
            return self._result(
                VerificationOutcome.REJECTED,
                {"reason": "invalid_counterexample_witness"},
            )
        for name in names:
            if not any(
                _same_json_value(witness[name], allowed) for allowed in domains[name]
            ):
                return self._result(
                    VerificationOutcome.REJECTED,
                    {"reason": "invalid_counterexample_witness"},
                )
        try:
            truth = _require_bool(evaluate(spec["predicate"], witness), "predicate")
        except (FormalSpecError, KeyError, TypeError, ValueError) as error:
            return self._result(
                VerificationOutcome.REJECTED,
                {"reason": "counterexample_evaluation_failed", "error": str(error)},
            )
        if truth:
            return self._result(
                VerificationOutcome.REJECTED,
                {"reason": "witness_does_not_disprove_claim"},
            )
        return self._result(
            VerificationOutcome.DISPROVED,
            {"witness": witness, "predicate_result": False},
        )

    def _verify_enumeration(
        self,
        spec: dict[str, Any],
        candidate: SearchCandidate,
        names: list[str],
        domains: dict[str, list[Any]],
        total: int,
    ) -> VerificationResult:
        rows: list[dict[str, Any]] = []
        try:
            for assignment in assignments(names, domains):
                truth = _require_bool(
                    evaluate(spec["predicate"], assignment), "predicate"
                )
                rows.append({"assignment": assignment, "result": truth})
                if not truth:
                    return self._result(
                        VerificationOutcome.REJECTED,
                        {
                            "reason": "proof_candidate_has_counterexample",
                            "witness": assignment,
                        },
                    )
        except (FormalSpecError, KeyError, TypeError, ValueError) as error:
            return self._result(
                VerificationOutcome.REJECTED,
                {"reason": "proof_evaluation_failed", "error": str(error)},
            )

        expected_hash = hash_json(rows)
        if (
            candidate.payload.get("assignments_checked") != total
            or candidate.payload.get("total_assignments") != total
            or candidate.payload.get("truth_table_hash") != expected_hash
        ):
            return self._result(
                VerificationOutcome.REJECTED,
                {
                    "reason": "proof_certificate_mismatch",
                    "assignments_checked": total,
                    "truth_table_hash": expected_hash,
                },
            )
        return self._result(
            VerificationOutcome.PROVED,
            {
                "assignments_checked": total,
                "truth_table_hash": expected_hash,
            },
        )
