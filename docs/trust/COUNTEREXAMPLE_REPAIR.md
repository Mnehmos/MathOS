# Counterexample Packages and Claim Repair

MathOS treats a counterexample as a repair input only after the original exact claim derives `disproved`. Search output alone is non-authoritative. The repair application replays the complete live fidelity and protected refutation chains, selects one current refutation formalization, and binds the witness to an exact verified `counterexample_search` run head.

The closed `counterexample_package/1` artifact records the exact source and false claim, typed canonical witness, Lean checker derived from the negation formalization, qualifying fidelity and authority identities, optional minimization support, failure explanation, repair operation, full proposed `claim/1`, and exact search provenance. Its SHA-256 is the hash of its RFC 8785 canonical JSON bytes. Metadata fixes it as a private generated package and repeats its claim, formalization, and run identities.

Repair is one controlled logical commit:

1. stage the derived package bytes in CAS;
2. acquire an immediate SQLite transaction;
3. recheck every captured truth input and the search-run head;
4. register the package metadata;
5. create a new immutable claim object;
6. create one `research.repairs` edge from the new claim version to the old claim version; and
7. store the exact idempotency result.

A fault rolls back steps 4–7 together. Pre-staged CAS bytes may remain, but unregistered bytes are not canonical state. Generic edge creation cannot create `research.repairs`, and neither CLI nor MCP accepts caller-authored status, evidence, checker, hash, object ID, package bytes, or edge payload.

Package retrieval is an audit operation, not a projection lookup. It rehashes CAS bytes and metadata, reloads all exact records, verifies the Lean module and optional support, confirms the bound run-chain prefix, replays fidelity and protected authority, and requires the exact new claim and unique controlled edge.

The original claim remains current and `disproved`. The repaired claim has a distinct stable object ID and starts `not_started`. It receives `proved` only after a separate formalization, fidelity review, and protected proof-authority lifecycle.
