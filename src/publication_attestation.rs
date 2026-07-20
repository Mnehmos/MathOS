use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::Value;

use crate::domain::{PublicationPolicy, PublicationReport};
use crate::error::AppError;

const MAX_RAW_OUTPUT_BYTES: usize = 1_048_576;
const MAX_BUNDLE_BYTES: usize = 512 * 1_024;
const MAX_VERIFIED_TIMESTAMPS: usize = 8;
const VERIFICATION_RESULT_MEDIA_TYPE: &str =
    "application/vnd.dev.sigstore.verificationresult+json;version=0.1";
const IN_TOTO_STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
const GITHUB_WORKFLOW_BUILD_TYPE: &str = "https://actions.github.io/buildtypes/workflow/v1";
const GITHUB_OIDC_ISSUER: &str = "https://token.actions.githubusercontent.com";
const REKOR_URI: &str = "https://rekor.sigstore.dev";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ParsedGhAttestation {
    pub verified_attestation_count: u32,
    pub verified_timestamp_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhAttestationProcessingResult {
    attestation: GhAttestation,
    #[serde(rename = "verificationResult")]
    verification_result: GhVerificationResult,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhAttestation {
    bundle: Value,
    bundle_url: String,
    initiator: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhVerificationResult {
    #[serde(rename = "mediaType")]
    media_type: String,
    signature: GhSignature,
    #[serde(rename = "verifiedTimestamps")]
    verified_timestamps: Vec<GhVerifiedTimestamp>,
    #[serde(rename = "verifiedIdentity")]
    verified_identity: GhVerifiedIdentity,
    statement: GhStatement,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhSignature {
    certificate: GhCertificate,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GhCertificate {
    certificate_issuer: String,
    subject_alternative_name: String,
    issuer: String,
    github_workflow_trigger: String,
    #[serde(rename = "githubWorkflowSHA")]
    github_workflow_sha: String,
    github_workflow_name: String,
    github_workflow_repository: String,
    github_workflow_ref: String,
    #[serde(rename = "buildSignerURI")]
    build_signer_uri: String,
    build_signer_digest: String,
    runner_environment: String,
    #[serde(rename = "sourceRepositoryURI")]
    source_repository_uri: String,
    source_repository_digest: String,
    source_repository_ref: String,
    source_repository_identifier: String,
    #[serde(rename = "sourceRepositoryOwnerURI")]
    source_repository_owner_uri: String,
    source_repository_owner_identifier: String,
    #[serde(rename = "buildConfigURI")]
    build_config_uri: String,
    build_config_digest: String,
    build_trigger: String,
    #[serde(rename = "runInvocationURI")]
    run_invocation_uri: String,
    source_repository_visibility_at_signing: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhVerifiedTimestamp {
    #[serde(rename = "type")]
    kind: String,
    uri: String,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GhVerifiedIdentity {
    subject_alternative_name: GhSubjectAlternativeNameMatcher,
    issuer: GhIssuerMatcher,
    runner_environment: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GhSubjectAlternativeNameMatcher {
    subject_alternative_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhIssuerMatcher {
    issuer: String,
    regexp: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhStatement {
    #[serde(rename = "_type")]
    statement_type: String,
    subject: Vec<GhSubject>,
    #[serde(rename = "predicateType")]
    predicate_type: String,
    predicate: GhSlsaPredicate,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhSubject {
    name: String,
    digest: GhSha256Digest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhSha256Digest {
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GhSlsaPredicate {
    build_definition: GhBuildDefinition,
    run_details: GhRunDetails,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GhBuildDefinition {
    build_type: String,
    external_parameters: GhExternalParameters,
    internal_parameters: GhInternalParameters,
    resolved_dependencies: Vec<GhResolvedDependency>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhExternalParameters {
    workflow: GhWorkflowParameters,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhWorkflowParameters {
    path: String,
    #[serde(rename = "ref")]
    source_ref: String,
    repository: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhInternalParameters {
    github: GhGithubInternalParameters,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhGithubInternalParameters {
    event_name: String,
    repository_id: String,
    repository_owner_id: String,
    runner_environment: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhResolvedDependency {
    digest: GhGitDigest,
    uri: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhGitDigest {
    #[serde(rename = "gitCommit")]
    git_commit: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhRunDetails {
    builder: GhBuilder,
    metadata: GhRunMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GhBuilder {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GhRunMetadata {
    invocation_id: String,
}

pub(crate) fn validate_gh_attestation_output(
    raw: &[u8],
    bundle: &Value,
    report: &PublicationReport,
    policy: &PublicationPolicy,
) -> Result<ParsedGhAttestation, AppError> {
    report.validate_candidate(policy)?;

    require(
        !raw.is_empty() && raw.len() <= MAX_RAW_OUTPUT_BYTES,
        "GitHub attestation verifier output is empty or exceeds the reviewed byte bound",
    )?;
    require(
        bundle.is_object(),
        "registered attestation bundle is not one JSON bundle object",
    )?;
    let bundle_bytes = serde_json::to_vec(bundle).map_err(|error| {
        attestation_error(format!(
            "registered attestation bundle cannot be measured safely: {error}"
        ))
    })?;
    require(
        bundle_bytes.len() <= MAX_BUNDLE_BYTES,
        "registered attestation bundle exceeds the reviewed byte bound",
    )?;

    let mut results: Vec<GhAttestationProcessingResult> =
        serde_json::from_slice(raw).map_err(|error| {
            attestation_error(format!(
                "GitHub attestation verifier output violates its closed JSON contract: {error}"
            ))
        })?;
    require(
        results.len() == 1,
        "GitHub attestation verifier must return exactly one verified result for one JSON bundle",
    )?;
    let result = results.pop().expect("exactly one result was checked");

    require(
        result.attestation.bundle == *bundle,
        "GitHub attestation verifier output does not contain the registered bundle",
    )?;
    require(
        result.attestation.bundle_url.is_empty() && result.attestation.initiator.is_empty(),
        "GitHub attestation verifier output did not originate from the controlled local bundle",
    )?;

    let expected_report_hash = report.report_hash(policy)?;
    let repository_url = format!("https://github.com/{}", policy.repository);
    let owner = policy
        .repository
        .split_once('/')
        .map(|(owner, _)| owner)
        .ok_or_else(|| {
            attestation_error("publication repository cannot produce a certificate owner identity")
        })?;
    let owner_url = format!("https://github.com/{owner}");
    let certificate_identity = format!(
        "{repository_url}/{}@{}",
        policy.workflow_path, policy.required_source_ref
    );
    let run_invocation_uri = format!(
        "{repository_url}/actions/runs/{}/attempts/{}",
        report.workflow_run_id, report.workflow_run_attempt
    );
    let dependency_uri = format!("git+{repository_url}@{}", policy.required_source_ref);
    let repository_identifier = policy.repository_id.to_string();
    let repository_owner_identifier = policy.repository_owner_id.to_string();
    let verification = result.verification_result;

    require(
        verification.media_type == VERIFICATION_RESULT_MEDIA_TYPE,
        "GitHub attestation verifier returned an unexpected verification-result media type",
    )?;

    let certificate = &verification.signature.certificate;
    require(
        is_bounded_text(&certificate.certificate_issuer, 256)
            && is_bounded_text(&certificate.github_workflow_name, 256),
        "verified certificate contains an invalid issuer or workflow name",
    )?;
    require(
        certificate.subject_alternative_name == certificate_identity
            && certificate.issuer == GITHUB_OIDC_ISSUER
            && certificate.github_workflow_repository == policy.repository
            && certificate.github_workflow_ref == policy.required_source_ref
            && certificate.build_signer_uri == certificate_identity
            && certificate.runner_environment == "github-hosted"
            && certificate.source_repository_uri == repository_url
            && certificate.source_repository_ref == policy.required_source_ref
            && certificate.source_repository_owner_uri == owner_url
            && certificate.build_config_uri == certificate_identity
            && certificate.run_invocation_uri == run_invocation_uri
            && certificate.source_repository_visibility_at_signing == "public",
        "verified certificate does not bind the exact repository, workflow, ref, runner, and run",
    )?;
    require(
        certificate.github_workflow_sha == report.request.source_commit_sha
            && certificate.build_signer_digest == report.request.source_commit_sha
            && certificate.source_repository_digest == report.request.source_commit_sha
            && certificate.build_config_digest == report.request.source_commit_sha,
        "verified certificate does not bind every source and workflow digest to the report commit",
    )?;
    require(
        certificate.source_repository_identifier == repository_identifier
            && certificate.source_repository_owner_identifier == repository_owner_identifier,
        "verified certificate does not bind the policy-pinned immutable repository and owner identifiers",
    )?;
    require(
        certificate.github_workflow_trigger == certificate.build_trigger
            && matches!(
                certificate.build_trigger.as_str(),
                "push" | "workflow_dispatch"
            ),
        "verified certificate contains a mismatched or disallowed workflow trigger",
    )?;

    let identity = &verification.verified_identity;
    require(
        identity.subject_alternative_name.subject_alternative_name == certificate_identity
            && identity.issuer.issuer.is_empty()
            && identity.issuer.regexp == ".*"
            && identity.runner_environment == "github-hosted",
        "GitHub verifier identity result does not repeat the exact certificate policy",
    )?;

    let statement = &verification.statement;
    require(
        statement.statement_type == IN_TOTO_STATEMENT_TYPE
            && statement.predicate_type == policy.attestation_predicate_type,
        "verified statement has an unexpected type or predicate type",
    )?;
    require(
        statement.subject.len() == 1,
        "verified statement must bind exactly one publication report subject",
    )?;
    let subject = &statement.subject[0];
    require(
        subject.name == "publication-report.json" && subject.digest.sha256 == expected_report_hash,
        "verified statement subject does not bind the exact publication report",
    )?;

    let build = &statement.predicate.build_definition;
    require(
        build.build_type == GITHUB_WORKFLOW_BUILD_TYPE,
        "verified statement has an unexpected GitHub Actions build type",
    )?;
    let workflow = &build.external_parameters.workflow;
    require(
        workflow.path == policy.workflow_path
            && workflow.source_ref == policy.required_source_ref
            && workflow.repository == repository_url,
        "verified statement workflow parameters do not match publication policy",
    )?;
    let github = &build.internal_parameters.github;
    require(
        github.event_name == certificate.build_trigger
            && github.runner_environment == certificate.runner_environment
            && github.repository_id == certificate.source_repository_identifier
            && github.repository_owner_id == certificate.source_repository_owner_identifier,
        "verified statement internal parameters disagree with the certificate",
    )?;
    require(
        build.resolved_dependencies.len() == 1,
        "verified statement must contain exactly one source dependency",
    )?;
    let dependency = &build.resolved_dependencies[0];
    require(
        dependency.uri == dependency_uri
            && dependency.digest.git_commit == report.request.source_commit_sha,
        "verified statement dependency does not bind the exact source ref and commit",
    )?;
    require(
        statement.predicate.run_details.builder.id == certificate_identity
            && statement.predicate.run_details.metadata.invocation_id == run_invocation_uri,
        "verified statement run details do not bind the exact builder and workflow run",
    )?;

    let timestamps = &verification.verified_timestamps;
    require(
        (1..=MAX_VERIFIED_TIMESTAMPS).contains(&timestamps.len()),
        "GitHub attestation verifier returned an invalid timestamp count",
    )?;
    let mut saw_transparency_log = false;
    let mut unique_timestamps = BTreeSet::new();
    for timestamp in timestamps {
        require(
            is_rfc3339_utc_timestamp(&timestamp.timestamp),
            "GitHub attestation verifier returned an invalid verified timestamp",
        )?;
        let supported = match timestamp.kind.as_str() {
            "Tlog" => {
                saw_transparency_log = true;
                timestamp.uri == REKOR_URI
            }
            "TimestampAuthority" => {
                is_bounded_text(&timestamp.uri, 256) && timestamp.uri.starts_with("https://")
            }
            _ => false,
        };
        require(
            supported,
            "GitHub attestation verifier returned an unsupported timestamp source",
        )?;
        require(
            unique_timestamps.insert((
                timestamp.kind.as_str(),
                timestamp.uri.as_str(),
                timestamp.timestamp.as_str(),
            )),
            "GitHub attestation verifier returned a duplicate verified timestamp",
        )?;
    }
    require(
        saw_transparency_log,
        "GitHub attestation verifier returned no Rekor transparency-log timestamp",
    )?;

    Ok(ParsedGhAttestation {
        verified_attestation_count: 1,
        verified_timestamp_count: timestamps.len() as u32,
    })
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), AppError> {
    if condition {
        Ok(())
    } else {
        Err(attestation_error(message))
    }
}

fn attestation_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_PUBLICATION_ATTESTATION_INVALID",
        message,
        false,
        "Re-run the pinned GitHub attestation verifier over the exact registered report and bundle.",
    )
}

fn is_bounded_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

fn is_rfc3339_utc_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !value.is_ascii() || !(20..=30).contains(&bytes.len()) {
        return false;
    }
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || *bytes.last().expect("timestamp is nonempty") != b'Z'
    {
        return false;
    }
    for index in [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18] {
        if !bytes[index].is_ascii_digit() {
            return false;
        }
    }
    if bytes.len() == 20 {
        if bytes[19] != b'Z' {
            return false;
        }
    } else if bytes[19] != b'.'
        || bytes[20..bytes.len() - 1].is_empty()
        || !bytes[20..bytes.len() - 1].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    let parse = |range: std::ops::Range<usize>| {
        std::str::from_utf8(&bytes[range])
            .ok()
            .and_then(|part| part.parse::<u32>().ok())
    };
    matches!(parse(0..4), Some(1..=9999))
        && matches!(parse(5..7), Some(1..=12))
        && matches!(parse(8..10), Some(1..=31))
        && matches!(parse(11..13), Some(0..=23))
        && matches!(parse(14..16), Some(0..=59))
        && matches!(parse(17..19), Some(0..=59))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::domain::publication::committed_publication_policy;
    use crate::domain::schemas::ExactVersionReference;
    use crate::domain::{
        PublicationClassification, PublicationOutcome, PublicationRequest,
        PublicationRunnerEnvironment,
    };

    fn report_and_policy() -> (PublicationReport, PublicationPolicy) {
        let policy = committed_publication_policy().expect("committed policy");
        let request = PublicationRequest {
            schema_version: "publication_request/1".to_owned(),
            subject: ExactVersionReference {
                object_id: "019f7dc2-fc5e-7f60-a22d-add750d1f0e3".to_owned(),
                version_hash: "1".repeat(64),
            },
            outcome: PublicationOutcome::Proof,
            diagnostic_evidence_id: "019f7dc2-fd75-7f70-a0f3-bca08b1ea241".to_owned(),
            diagnostic_evidence_hash: "2".repeat(64),
            proof_closure_evidence_id: "019f7dc2-fe92-7450-ba61-ad71f95fa4b4".to_owned(),
            proof_closure_evidence_hash: "3".repeat(64),
            axiom_audit_evidence_id: "019f7dc2-fe92-7450-ba61-ad891b665d1b".to_owned(),
            axiom_audit_evidence_hash: "4".repeat(64),
            environment_hash: "5".repeat(64),
            module_artifact_hash: "6".repeat(64),
            declaration_name: "MathOS.Publication.smoke".to_owned(),
            policy_hash: policy.policy_hash().expect("policy hash"),
            source_commit_sha: "a".repeat(40),
            source_tree_sha: "b".repeat(40),
        };
        let report = PublicationReport {
            schema_version: "publication_report/1".to_owned(),
            request_hash: request.request_hash().expect("request hash"),
            request,
            classification: PublicationClassification::Passed,
            repository: policy.repository.clone(),
            workflow_path: policy.workflow_path.clone(),
            source_ref: policy.required_source_ref.clone(),
            workflow_run_id: 29_716_676_599,
            workflow_run_attempt: 1,
            runner_environment: PublicationRunnerEnvironment::GithubHosted,
            observed_lean_toolchain: policy.required_lean_toolchain.clone(),
            observed_axioms: Vec::new(),
            retained_artifact_hashes: vec!["7".repeat(64)],
            clean_checkout: true,
            dependency_closure_complete: true,
            network_isolation_enforced: true,
            memory_limit_enforced: true,
            authoritative: false,
        };
        (report, policy)
    }

    fn valid_output(
        report: &PublicationReport,
        policy: &PublicationPolicy,
        bundle: &Value,
    ) -> Value {
        let repository_url = format!("https://github.com/{}", policy.repository);
        let owner = policy.repository.split_once('/').expect("repository").0;
        let identity = format!(
            "{repository_url}/{}@{}",
            policy.workflow_path, policy.required_source_ref
        );
        let run_uri = format!(
            "{repository_url}/actions/runs/{}/attempts/{}",
            report.workflow_run_id, report.workflow_run_attempt
        );
        json!([{
            "attestation": {
                "bundle": bundle,
                "bundle_url": "",
                "initiator": ""
            },
            "verificationResult": {
                "mediaType": VERIFICATION_RESULT_MEDIA_TYPE,
                "signature": {
                    "certificate": {
                        "certificateIssuer": "CN=sigstore-intermediate,O=sigstore.dev",
                        "subjectAlternativeName": identity,
                        "issuer": GITHUB_OIDC_ISSUER,
                        "githubWorkflowTrigger": "push",
                        "githubWorkflowSHA": report.request.source_commit_sha,
                        "githubWorkflowName": "Publication authority boundary",
                        "githubWorkflowRepository": policy.repository,
                        "githubWorkflowRef": policy.required_source_ref,
                        "buildSignerURI": identity,
                        "buildSignerDigest": report.request.source_commit_sha,
                        "runnerEnvironment": "github-hosted",
                        "sourceRepositoryURI": repository_url,
                        "sourceRepositoryDigest": report.request.source_commit_sha,
                        "sourceRepositoryRef": policy.required_source_ref,
                        "sourceRepositoryIdentifier": "1305399818",
                        "sourceRepositoryOwnerURI": format!("https://github.com/{owner}"),
                        "sourceRepositoryOwnerIdentifier": "193347153",
                        "buildConfigURI": identity,
                        "buildConfigDigest": report.request.source_commit_sha,
                        "buildTrigger": "push",
                        "runInvocationURI": run_uri,
                        "sourceRepositoryVisibilityAtSigning": "public"
                    }
                },
                "verifiedTimestamps": [{
                    "type": "Tlog",
                    "uri": REKOR_URI,
                    "timestamp": "2026-07-20T04:22:41Z"
                }],
                "verifiedIdentity": {
                    "subjectAlternativeName": {"subjectAlternativeName": identity},
                    "issuer": {"issuer": "", "regexp": ".*"},
                    "runnerEnvironment": "github-hosted"
                },
                "statement": {
                    "_type": IN_TOTO_STATEMENT_TYPE,
                    "subject": [{
                        "name": "publication-report.json",
                        "digest": {"sha256": report.report_hash(policy).expect("report hash")}
                    }],
                    "predicateType": policy.attestation_predicate_type,
                    "predicate": {
                        "buildDefinition": {
                            "buildType": GITHUB_WORKFLOW_BUILD_TYPE,
                            "externalParameters": {
                                "workflow": {
                                    "path": policy.workflow_path,
                                    "ref": policy.required_source_ref,
                                    "repository": repository_url
                                }
                            },
                            "internalParameters": {
                                "github": {
                                    "event_name": "push",
                                    "repository_id": "1305399818",
                                    "repository_owner_id": "193347153",
                                    "runner_environment": "github-hosted"
                                }
                            },
                            "resolvedDependencies": [{
                                "digest": {"gitCommit": report.request.source_commit_sha},
                                "uri": format!("git+{repository_url}@{}", policy.required_source_ref)
                            }]
                        },
                        "runDetails": {
                            "builder": {"id": identity},
                            "metadata": {"invocationId": run_uri}
                        }
                    }
                }
            }
        }])
    }

    fn reject(
        value: &Value,
        bundle: &Value,
        report: &PublicationReport,
        policy: &PublicationPolicy,
    ) {
        let raw = serde_json::to_vec(value).expect("test output");
        assert_eq!(
            validate_gh_attestation_output(&raw, bundle, report, policy)
                .expect_err("altered verifier output must fail")
                .code,
            "MCL_PUBLICATION_ATTESTATION_INVALID"
        );
    }

    #[test]
    fn exact_closed_output_is_accepted() {
        let (report, policy) = report_and_policy();
        let bundle = json!({"mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"});
        let output = valid_output(&report, &policy, &bundle);
        assert_eq!(
            validate_gh_attestation_output(
                &serde_json::to_vec(&output).expect("output"),
                &bundle,
                &report,
                &policy,
            )
            .expect("valid verifier output"),
            ParsedGhAttestation {
                verified_attestation_count: 1,
                verified_timestamp_count: 1,
            }
        );
    }

    #[test]
    fn unknown_nested_field_is_rejected() {
        let (report, policy) = report_and_policy();
        let bundle = json!({"mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"});
        let mut output = valid_output(&report, &policy, &bundle);
        output[0]["verificationResult"]["statement"]["unexpected"] = json!(true);
        reject(&output, &bundle, &report, &policy);
    }

    #[test]
    fn wrong_subject_and_bundle_are_rejected() {
        let (report, policy) = report_and_policy();
        let bundle = json!({"mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"});
        let mut wrong_subject = valid_output(&report, &policy, &bundle);
        wrong_subject[0]["verificationResult"]["statement"]["subject"][0]["digest"]["sha256"] =
            json!("8".repeat(64));
        reject(&wrong_subject, &bundle, &report, &policy);

        let wrong_bundle = json!({"mediaType": "different"});
        let output = valid_output(&report, &policy, &bundle);
        reject(&output, &wrong_bundle, &report, &policy);
    }

    #[test]
    fn wrong_certificate_and_run_binding_are_rejected() {
        let (report, policy) = report_and_policy();
        let bundle = json!({"mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"});
        let mut wrong_certificate = valid_output(&report, &policy, &bundle);
        wrong_certificate[0]["verificationResult"]["signature"]["certificate"]["runnerEnvironment"] =
            json!("self-hosted");
        reject(&wrong_certificate, &bundle, &report, &policy);

        let mut wrong_run = valid_output(&report, &policy, &bundle);
        wrong_run[0]["verificationResult"]["signature"]["certificate"]["runInvocationURI"] =
            json!("https://github.com/Mnehmos/MathOS/actions/runs/1/attempts/1");
        reject(&wrong_run, &bundle, &report, &policy);

        let mut recreated_repository = valid_output(&report, &policy, &bundle);
        recreated_repository[0]["verificationResult"]["signature"]["certificate"]["sourceRepositoryIdentifier"] =
            json!("9999999999");
        recreated_repository[0]["verificationResult"]["statement"]["predicate"]["buildDefinition"]
            ["internalParameters"]["github"]["repository_id"] = json!("9999999999");
        reject(&recreated_repository, &bundle, &report, &policy);

        let mut recreated_owner = valid_output(&report, &policy, &bundle);
        recreated_owner[0]["verificationResult"]["signature"]["certificate"]["sourceRepositoryOwnerIdentifier"] =
            json!("9999999999");
        recreated_owner[0]["verificationResult"]["statement"]["predicate"]["buildDefinition"]["internalParameters"]
            ["github"]["repository_owner_id"] = json!("9999999999");
        reject(&recreated_owner, &bundle, &report, &policy);
    }

    #[test]
    fn missing_transparency_log_and_invalid_timestamp_are_rejected() {
        let (report, policy) = report_and_policy();
        let bundle = json!({"mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"});
        let mut no_tlog = valid_output(&report, &policy, &bundle);
        no_tlog[0]["verificationResult"]["verifiedTimestamps"][0]["type"] =
            json!("TimestampAuthority");
        no_tlog[0]["verificationResult"]["verifiedTimestamps"][0]["uri"] =
            json!("https://timestamp.example");
        reject(&no_tlog, &bundle, &report, &policy);

        let mut invalid_timestamp = valid_output(&report, &policy, &bundle);
        invalid_timestamp[0]["verificationResult"]["verifiedTimestamps"][0]["timestamp"] =
            json!("now");
        reject(&invalid_timestamp, &bundle, &report, &policy);
    }

    #[test]
    fn multiple_results_subjects_and_dependencies_are_rejected() {
        let (report, policy) = report_and_policy();
        let bundle = json!({"mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"});
        let output = valid_output(&report, &policy, &bundle);

        let mut multiple_results = output.clone();
        multiple_results
            .as_array_mut()
            .expect("result array")
            .push(output[0].clone());
        reject(&multiple_results, &bundle, &report, &policy);

        let mut multiple_subjects = output.clone();
        let second_subject =
            multiple_subjects[0]["verificationResult"]["statement"]["subject"][0].clone();
        multiple_subjects[0]["verificationResult"]["statement"]["subject"]
            .as_array_mut()
            .expect("subjects")
            .push(second_subject);
        reject(&multiple_subjects, &bundle, &report, &policy);

        let mut multiple_dependencies = output;
        let second_dependency = multiple_dependencies[0]["verificationResult"]["statement"]
            ["predicate"]["buildDefinition"]["resolvedDependencies"][0]
            .clone();
        multiple_dependencies[0]["verificationResult"]["statement"]["predicate"]["buildDefinition"]
            ["resolvedDependencies"]
            .as_array_mut()
            .expect("dependencies")
            .push(second_dependency);
        reject(&multiple_dependencies, &bundle, &report, &policy);
    }
}
