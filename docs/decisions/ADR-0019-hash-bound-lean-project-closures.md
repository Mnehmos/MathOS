# ADR-0019: Hash-bound Lean project closures

Status: accepted

Date: 2026-07-25

## Context

The original verifier materialized one registered `.lean` file and ran a controlled Lean driver in
an otherwise empty workspace. That boundary is sufficient for standalone fixtures, but the BH
Pilot C theorem imports a pinned multi-file Lake project. A fresh CLI playtest therefore failed
with `unknown module prefix 'BH'` even though the exact headline module, declaration, environment,
and upstream commit were registered.

Copying an arbitrary checkout into the worker would fix module resolution by opening a larger
trust hole. Lake configuration, locked dependencies, imported sources, hidden build outputs,
archive links, and the selected module all affect what Lean sees. Caller-authored build commands
would also bypass the typed verifier-command boundary.

## Decision

MathOS adds an optional exact project binding to the formalization and verifier request:

- one registered raw tar artifact;
- the fixed archive root `project`;
- one validated relative `.lean` module path.

Project extraction is application-owned. It rejects absolute paths, traversal, links, special or
sparse entries, metadata entries, duplicate paths, hidden VCS/build/cache trees, excessive
inventory or size, and any entry outside the bound root. The extracted module must be byte-identical
to the separately registered Lean source artifact. Every retained Lean source is scanned for the
forbidden proof-authority tokens.

The environment binds:

- the exact Lean toolchain;
- the complete sorted dependency revision list;
- every allowed import;
- the SHA-256 of `lake-manifest.json`, `lakefile.lean`, and `lean-toolchain`;
- the closed `lake env lean {module_path}` verifier shape;
- optional allowlisted Mathlib cache preparation;
- timeout, output, and concurrency limits.

The worker passes the canonical concurrency limit as its application-owned `LEAN_NUM_THREADS`
value to Lake's task manager after clearing the inherited environment. Cache download and unpack
are fixed phases with one shared timeout/output budget. Under the publication profile they run in
the same capability-free, home-masked, workspace-only filesystem sandbox with network deliberately
shared so exact pinned cache objects can be acquired. They do not themselves claim network
isolation. The build target is the validated bound module's `olean` facet; unneeded C and
editor-metadata outputs cannot consume the proof-checking budget. A verifier-controlled driver then
imports its module name and performs both `#check` and `#print axioms` for the exact declaration.

A Linux `publication` environment additionally requires an exact memory bound. Before execution,
the worker verifies that the input is an application-created disposable workspace, temporarily
makes that exact tree root-owned, and opens traversal only on the required mount-source directory
chain. It then runs the build and driver through the fixed non-interactive `sudo` -> Bubblewrap ->
`prlimit` chain: all namespaces are unshared, the network namespace is empty, capabilities and
inherited environment are removed, the exact writable project workspace is mounted at `/mnt`, the
pinned toolchain root is read-only at `/opt`, and only application-owned command arguments and
environment values enter the namespace. Workspace ownership and traversal permissions are restored
when the execution scope ends. A successful publication execution report can claim memory and
network controls only after the inner command exits successfully. The report remains
non-authoritative.

On Windows, project workspaces use an exclusive directory below the bounded operating-system
temporary root. This avoids legacy path-length failure in deep Mathlib cache paths without
changing project bytes or accepting a caller path.

The project binding is carried unchanged through diagnostic evidence, project audit, publication
request/stage/retained closure, authority replay, portable release, and MathCorpus/MCIP export.
Project audit rematerializes and rescans the exact closure, then revalidates the immutable
diagnostic driver's axiom output. It does not call a cached local build an independent authority
boundary. In protected publication the diagnostic execution itself uses the publication profile,
so the one clean resource-bounded, network-isolated build supplies both the typed verifier report
and the immutable audit stream. The workflow does not repeat that multi-hour build merely to create
duplicate log roles.

A project publication closure has 20 typed roles, including one exact
`lean_project_archive` role and the publication-profile verifier/audit reports and raw streams. It
does not retain the six legacy duplicate `protected_*` streams. Portable releases retain the
archive as `replay/project.tar`; corpus exports retain it as `lean-project/project.tar`. Offline
verification safely extracts, revalidates, builds, and runs the same publication-profile driver
without opening SQLite. Standalone requests and their historical 25-role closures remain
byte-compatible.

## Consequences

- A project is one immutable, hash-bound input rather than ambient workspace state.
- Archive metadata remains part of artifact identity even when two safe archives contain the same
  source bytes.
- Local execution remains diagnostic. Publication-profile execution is protected but still
  explicitly non-authoritative until the attested authority gate replays the whole closure.
- The generated certificate sources and their provenance survive publication and offline export
  instead of being collapsed into `Final.lean`.
- Project builds are more expensive than standalone checks. Canonical concurrency is enforced,
  the audit reuses only the exact immutable diagnostic execution, and candidate generation does
  not repeat the same protected build.
- Supporting another build system or dependency-preparation action requires a new reviewed closed
  contract; it cannot be expressed through a request string.

## Rejected alternatives

### Copy only `Final.lean`

Rejected because imports, generated certificate modules, Lake configuration, and locked
dependencies disappear.

### Accept a checkout path or arbitrary command

Rejected because caller-controlled filesystem and command surfaces would define the proof
environment outside canonical state.

### Retain precompiled project outputs in the input archive

Rejected because hidden `.lake` outputs could replace source elaboration and make a coherent
archive substitution appear to be a clean build.

### Treat a local project audit or successful workflow as authority

Rejected because local diagnostics do not enforce the publication isolation boundary, a
publication-profile report remains non-authoritative, and workflow success alone is not
mathematical evidence.
