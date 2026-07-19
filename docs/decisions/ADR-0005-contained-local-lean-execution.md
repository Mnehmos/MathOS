# ADR-0005: Contained local Lean execution is not proof authority

Date: 2026-07-19

Status: accepted

## Context

MathOS must execute untrusted, model-proposed Lean modules without granting those modules shell access, unrestricted filesystem access, network authority, or mathematical status. The local development profile must work on supported Linux and Windows hosts, but ordinary child-process controls on those hosts are not a hardened virtualization boundary.

The engine already persists exact verifier intent, immutable environment manifests, content-addressed Lean source, durable leases, and idempotent attempts. The next smallest complete boundary is controlled elaboration with durable diagnostics. Kernel authority, proof closure, axiom audit, fidelity review, and publication isolation are separate later gates.

## Decision

The local worker leases one canonical verifier job and constructs its invocation from typed state. It accepts only the configured `lean` or `lean.exe` executable, passes only `--version` or a verifier-selected module file, clears the child environment, restores a narrow runtime-variable allowlist, supplies null stdin, and runs inside a fresh temporary directory below the configured instance root.

The worker copies verified source bytes from CAS into that directory and creates a controlled driver that checks the requested declaration. The request cannot supply a path, command, shell fragment, argument, or environment variable.

Before launch, a conservative lexical policy rejects explicit holes, custom source axioms, unsafe declarations, native evaluation, command elaborators, initialization hooks, file inclusion, and related execution surfaces. This scan is defense in depth. It is not a substitute for kernel checking or transitive proof-closure audit.

Wall-clock time and combined retained output are bounded. The worker kills and reaps a child that exceeds either bound, stores bounded stdout and stderr as private content-addressed artifacts, and stores a canonical structured execution report as the terminal job result.

Every report from this boundary has `authoritative: false`. An `elaborated` classification means only that the selected Lean executable accepted the controlled driver in the observed local environment. A `rejected` classification is an execution result, not a source-claim refutation. Operational job success means the attempt completed and its report was committed. It does not mean the theorem or source claim succeeded.

The local report states that memory enforcement and network isolation are false. The local worker refuses publication-profile environments. Publication evidence requires a later protected Linux CI boundary with dependency closure, network isolation, proof-closure scans, axiom audit, and retained reports.

## Consequences

- Model-proposed source can reach only a bounded typed Lean invocation.
- Process diagnostics survive restart and remain linked to the exact job, environment, module, actor, and attempt.
- Local process containment is reported at its actual strength.
- Dependency-manifest closure is not yet established by this slice.
- A child process that leaves inheriting descendants may require stronger operating-system process-tree controls before publication use.
- No evidence or derived truth status may consume this execution report as authority.

## Rejected alternatives

### Treat successful elaboration as proof evidence

Rejected because elaboration alone does not establish hole freedom, axiom policy, dependency closure, source fidelity, or publication reproducibility.

### Expose an agent-supplied command

Rejected because arbitrary command assembly would collapse the process trust boundary into prompt policy.

### Claim a hardened sandbox on every host

Rejected because the current operating-system controls do not enforce that claim consistently on Linux and Windows.

### Delay all execution until publication isolation exists

Rejected because a truthful local profile is useful for iterative formalization, diagnostics, and repair while stronger authority gates remain explicit.
