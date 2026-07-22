# Comparator-ready packages

Comparator package construction is an offline projection of one already verified portable release.
It prepares inputs; it does not run Comparator or grant authority.

## Build

Create a canonical `comparator_package_plan/1` and supply the frozen release plus its separately
trusted manifest hash:

```text
mcl --root <missing-path-is-allowed> --json release export-comparator \
  --plan <canonical-plan.json> \
  --bundle-dir <portable-release> \
  --expected-release-manifest-hash <trusted-release-sha256> \
  --output-dir <new-package-directory>
```

Add `--dry-run` to derive the same identity without writing the destination. The destination must
not exist. The output inventory is exactly `Challenge.lean`, `Solution.lean`, `config.json`,
`formalization.yaml`, and `verification.json`.

The plan owns the reviewed challenge, theorem name, allowed axioms, nanoda choice, descriptive
formalization metadata, and full upstream tool pins. `Solution.lean` always comes from the frozen
release; callers cannot replace it. Version 1 packages exactly the release's headline
formalization and one theorem.

## Verify without an instance

```text
mcl --root <missing-path-is-allowed> --json release verify-comparator-package \
  --package-dir <copied-package> \
  --expected-verification-hash <trusted-verification-json-sha256> \
  --plan <canonical-plan.json> \
  --bundle-dir <portable-release> \
  --expected-release-manifest-hash <trusted-release-sha256>
```

Verification checks the exact five-file tree, rejects links and reparse points, recomputes every
binding, revalidates the source release and exact formalization/environment/module relationship,
and requires byte-identical reprojection. It opens no database and performs no network access.

## Status language

- `ready`: the five files and their frozen-source bindings verify.
- `Comparator-verified`: an exact official Linux sandboxed Comparator run has accepted this package
  and toolchain. Its report remains non-authoritative.
- `current` or `stale`: the separate controlled authority layer derives whether the promoted
  receipt-bound evidence still matches every canonical input.

The v1 package record can encode only `ready`, `comparator_verified: false`, and
`authoritative: false`. A normal Lean build, offline reprojection, fake-landrun, or a green badge is
not Comparator verification.

The separate [controlled Comparator authority](COMPARATOR_AUTHORITY.md) workflow stages and
attests an exact accepted report, then derives `evidence/3` only from its immutable receipt. Package
readiness and the report's `comparator_verified` field cannot self-promote.

The trust decision is recorded in
[ADR-0014](../decisions/ADR-0014-deterministic-comparator-ready-package-boundary.md).
