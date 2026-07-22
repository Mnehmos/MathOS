# ADR-0015: Official Comparator verification is an attested non-authoritative execution result

Date: 2026-07-22

Status: accepted

## Context

A deterministic `ready` package is not proof that official Comparator accepted its solution.
Comparator's security argument also requires a checking environment that has never compiled the
solution, the exact reviewed Comparator and lean4export sources, real Landlock enforcement, a
non-root user, the upstream systemd address-family mitigation, and replay by the matching Lean
kernel.

Running Comparator after readiness diagnostics in the package-publication job would reuse an
environment that already elaborated `Solution.lean`. Treating an exit code, mutable tool checkout,
or caller-written JSON as evidence would not preserve the operational assumptions.

## Decision

The publication workflow runs official Comparator only in a separate GitHub-hosted Linux job that
depends on a successful protected-main boundary job. The boundary job exports trusted hashes for
the package verification, release manifest, and package plan. The fresh job downloads that exact
same-run artifact and runs `mcl release verify-comparator-package` against all three identities
before any solution compilation.

The job builds and retains binaries from these exact source identities:

- Comparator `68a064109f01c08f47c8edc9f51d6a2bbffaa188`, tree
  `0bb408593d6e5f625db53b3be16e3f1cc91a7524`;
- lean4export `af5aa64bb914c3c2c781f378088dbd38acf4f804`, tree
  `5058a7945d24656600ca05917e3c8c174485bcf5`;
- landrun `5ed4a3db3a4ad930d577215c6b9abaa19df7f99f`, tree
  `890013a5099a92792cbacd2cfff91af3f13cec9c`.

Comparator and lean4export are built with Lean `v4.32.0`; landrun is built with pinned Go
`1.24.2`. The report binds those build toolchains as well as the resulting binaries.

Before copying `Challenge.lean`, `Solution.lean`, and `config.json`, the runner creates the
dependency-free Lake manifest under Lean `v4.32.0`. The resulting harness must have exactly those
three source files and three runner-owned Lake files, with no `.lake` directory or `.olean` file.

Landrun is first exercised in strict V5 mode without `--best-effort`; failure aborts the run. The
official Comparator may then use its reviewed `--best-effort` invocation because strict V5 support
has already been established. Comparator runs as a non-root user in a live user systemd unit with
`NoNewPrivileges=yes` and `RestrictAddressFamilies=~AF_UNIX`. The unit is held at a runner-owned
gate outside the harness until those properties and the pristine harness are inspected.
After the gate opens, a reviewed probe must fail to create an AF_UNIX socket and must fail to
connect to a live loopback TCP listener through Comparator's exact `--best-effort --ro / --rw
/dev -ldd -add-exec` landrun base arguments. The report verifier requires one success marker from
each denial challenge.

The closed canonical `comparator_run_report/1` binds the exact package, reprojection receipt,
workflow and runner, source commit and tree, tool commits, trees and binaries, harness metadata,
sandbox predicates, raw output, exit result, and the retained runner script that defines the exact
invocation. Acceptance requires empty stderr and one ordered instance of each official success
path marker, including equal challenge/solution export targets and Lean-kernel acceptance.

`mcl release verify-comparator-run` verifies the exact 20-file tree without opening SQLite. It
streams binary hashes, revalidates the five-file package, checks every report binding, parses the
Lake and reprojection records, verifies live sandbox records, and independently classifies the raw
Comparator output. The canonical report is attested and the attestation is verified by the pinned
GitHub CLI verifier.

An accepted report may set `comparator_verified: true`. It must set `authoritative: false` and
cannot create evidence, update research status, or complete Pilot C. Canonical evidence ingestion
is a separate policy gate.

## Consequences

- Prior solution diagnostics cannot contaminate the official checking environment.
- Package, plan, release, workflow, runner, tool, harness, sandbox, log, or invocation changes make
  the report stale or invalid.
- Failed controls and unexpected output cannot be represented as accepted verification.
- The retained binaries and runner script permit independent artifact audit without retaining
  large build trees.
- Attestation authenticates the protected execution context but does not grant MathOS authority.

## Rejected alternatives

### Run Comparator at the end of the boundary job

Rejected because that job already compiles the solution during readiness checks.

### Trust Comparator's exit code alone

Rejected because exit status does not bind the package, tool binaries, sandbox, runner, output
path, or workflow identity.

### Let the report promote evidence directly

Rejected because execution authenticity and canonical evidence authority are distinct trust
transitions.
