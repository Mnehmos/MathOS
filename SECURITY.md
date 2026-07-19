# Security Policy

## Private vulnerability reporting

Do not report security vulnerabilities in a public issue, discussion, or pull request.

Use the [MathOS private vulnerability reporting form](https://github.com/Mnehmos/MathOS/security/advisories/new). This opens a private GitHub Security Advisory visible to the reporter and repository maintainers.

Include:

- A concise description of the vulnerability
- Affected component, branch, commit, or version
- Reproduction steps or a minimal proof of concept
- Expected and observed behavior
- Potential impact
- Any known mitigation
- Whether the report affects a mathematical trust boundary

If GitHub does not present the private-report form, contact [@Mnehmos](https://github.com/Mnehmos) without including sensitive details and request a private reporting channel.

## Security scope

Security reports may include:

- Authentication, authorization, secret, or private-data exposure
- Remote code execution or command injection
- Malicious MCP, API, file, or model input handling
- Dependency or build-pipeline compromise
- Provenance, artifact-hash, or identity tampering
- A path that marks an unverified claim, proof, counterexample, or trajectory as verified
- Verifier bypass or generator and verifier separation failure
- Unauthorized corpus or training-data disclosure

Ordinary mathematical disagreements, unsupported claims, and non-security correctness bugs may use a public issue when disclosure does not create risk.

## Response and disclosure

Maintainers will acknowledge reports as soon as practical, assess severity and scope, and coordinate remediation and disclosure with the reporter. Do not publish vulnerability details until a fix or mitigation is available and coordinated disclosure has been agreed.

Repository owners should keep GitHub private vulnerability reporting enabled.
