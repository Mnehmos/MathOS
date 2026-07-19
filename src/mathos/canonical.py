import hashlib
import json
from typing import Any


def canonical_json(value: Any) -> str:
    """Return a stable JSON representation suitable for hashing."""
    return json.dumps(
        value,
        ensure_ascii=False,
        allow_nan=False,
        separators=(",", ":"),
        sort_keys=True,
    )


def hash_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def hash_json(value: Any) -> str:
    return hash_text(canonical_json(value))
