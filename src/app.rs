use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::artifacts::ArtifactStore;
use crate::canonical::{canonical_json, record_version_hash};
use crate::config::ResolvedConfig;
use crate::domain::schemas::{
    ClaimPayload, ExactVersionReference, FormalizationClaimPolarity, FormalizationPayload,
    SourcePayload, SourceType, validate_record_payload,
};
use crate::domain::{
    ArtifactMetadata, ArtifactSnapshot, EdgeDraft, EdgeSnapshot, EnvironmentManifest,
    EnvironmentSnapshot, EvidenceAuthorityClass, EvidenceKind, EvidencePayload, EvidenceResult,
    EvidenceSnapshot, FidelityReviewHistoryEntry, FidelityReviewReport, FidelityReviewRequest,
    FidelityStatus, FidelityStatusSnapshot, FidelityVerdict, GraphTraversalHit,
    GraphTraversalRequest, LeanAuditClassification, LeanAuditJobSnapshot, LeanAuditReport,
    LeanAuditRequest, PublicationOutcome, PublicationReport, PublicationRequest,
    PublicationRetainedArtifactRole, PublicationRetainedClosure, RecordDraft, RecordKind,
    RecordSnapshot, RunChainReport, RunEventDraft, RunEventSnapshot, RunKind, RunSnapshot,
    VerifierExecutionClassification, VerifierExecutionReport, VerifierJobRequest,
    VerifierJobSnapshot, VerifierJobState,
};
use crate::error::AppError;
use crate::store::Store;

const DOCTOR_CANARY: &[u8] = b"mcl doctor artifact integrity canary v1";
const MAX_RETAINED_JSON_BYTES: usize = 1_048_576;
const MAX_RETAINED_LEAN_BYTES: usize = 1_048_576;
const MAX_RETAINED_LOG_BYTES: usize = 16 * 1_048_576;

#[derive(Debug, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub healthy: bool,
    pub detail: String,
    pub corrective_action: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticReport {
    pub healthy: bool,
    pub profile: &'static str,
    pub checks: Vec<Check>,
}

#[derive(Debug, Serialize)]
pub struct RecordMutationOutcome {
    pub dry_run: bool,
    pub proposed_version_hash: String,
    pub record: Option<RecordSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct EdgeMutationOutcome {
    pub dry_run: bool,
    pub edge: Option<EdgeSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct RunMutationOutcome {
    pub dry_run: bool,
    pub run: Option<RunSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct RunEventMutationOutcome {
    pub dry_run: bool,
    pub event: Option<RunEventSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct EnvironmentRegistrationOutcome {
    pub dry_run: bool,
    pub proposed_environment_hash: String,
    pub environment: Option<EnvironmentSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactIngestOutcome {
    pub dry_run: bool,
    pub proposed_artifact_hash: String,
    pub artifact: Option<ArtifactSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactVerificationReport {
    pub artifact: ArtifactSnapshot,
    pub content_hash_verified: bool,
    pub metadata_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct VerifierEnqueueOutcome {
    pub dry_run: bool,
    pub proposed_input_hash: String,
    pub job: Option<VerifierJobSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct VerifierWorkOutcome {
    pub job: VerifierJobSnapshot,
    pub report: VerifierExecutionReport,
}

#[derive(Debug, Serialize)]
pub struct EvidencePromotionOutcome {
    pub dry_run: bool,
    pub proposed_evidence_hash: String,
    pub evidence: Option<EvidenceSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct AuditEnqueueOutcome {
    pub dry_run: bool,
    pub proposed_input_hash: String,
    pub job: Option<LeanAuditJobSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct AuditWorkOutcome {
    pub job: LeanAuditJobSnapshot,
    pub report: LeanAuditReport,
}

#[derive(Debug, Serialize)]
pub struct AuditEvidencePromotionOutcome {
    pub dry_run: bool,
    pub proposed_evidence_hashes: Vec<String>,
    pub evidence: Option<Vec<EvidenceSnapshot>>,
}

#[derive(Debug, Serialize)]
pub struct PublicationRequestPreparationOutcome {
    pub dry_run: bool,
    pub proposed_request_hash: String,
    pub proposed_artifact_hash: String,
    pub request: PublicationRequest,
    pub artifact: Option<ArtifactSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct PublicationCandidateValidationOutcome {
    pub request_hash: String,
    pub report_content_hash: String,
    pub report_artifact_hash: String,
    pub retained_closure_hash: String,
    pub retained_closure_artifact_hash: String,
    pub authoritative: bool,
}

#[derive(Debug, Serialize)]
pub struct FidelityReviewOutcome {
    pub dry_run: bool,
    pub proposed_report_artifact_hash: String,
    pub proposed_evidence_hash: String,
    pub report: FidelityReviewReport,
    pub evidence: Option<EvidenceSnapshot>,
}

pub struct Application {
    store: Store,
    artifacts: ArtifactStore,
    verifier_command: String,
    workspace_root: PathBuf,
}

impl Application {
    pub fn open(config: &ResolvedConfig) -> Result<Self, AppError> {
        if !config.database.is_file() {
            return Err(AppError::new(
                "MCL_INSTANCE_NOT_INITIALIZED",
                format!("database does not exist at {}", config.database.display()),
                false,
                "Run `mcl init` for this instance root.",
            ));
        }
        let workspace_root = config.data_dir.join("workspaces");
        fs::create_dir_all(&workspace_root)
            .map_err(|error| AppError::io("create verifier workspace root", error))?;
        let workspace_root = workspace_root
            .canonicalize()
            .map_err(|error| AppError::io("canonicalize verifier workspace root", error))?;
        if !workspace_root.starts_with(&config.root) {
            return Err(AppError::new(
                "MCL_VERIFIER_WORKSPACE_UNSAFE",
                "verifier workspace root escaped the configured instance root",
                false,
                "Quarantine the instance and correct its configured data path.",
            ));
        }
        Ok(Self {
            store: Store::open(&config.database)?,
            artifacts: ArtifactStore::open(&config.artifacts)?,
            verifier_command: config.verifier.lean_command.clone(),
            workspace_root,
        })
    }

    pub fn initialize(
        config: &ResolvedConfig,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<Value, AppError> {
        validate_attribution(actor, idempotency_key)?;
        if dry_run {
            return Ok(json!({
                "message": "Initialization plan validated; no instance state was written.",
                "dry_run": true,
                "actor": actor,
                "idempotency_key": idempotency_key,
                "root": config.root,
                "data_dir": config.data_dir,
                "database": config.database,
                "artifacts": config.artifacts,
            }));
        }

        config.write_default()?;
        let artifacts = ArtifactStore::open(&config.artifacts)?;
        let mut store = Store::open(&config.database)?;
        store.migrate()?;
        let canary_hash = artifacts.put(DOCTOR_CANARY)?;
        let canary = artifacts.read(&canary_hash)?;
        if canary != DOCTOR_CANARY {
            return Err(AppError::new(
                "MCL_ARTIFACT_INTEGRITY_FAILED",
                "artifact canary did not survive initialization",
                false,
                "Quarantine the instance and inspect its storage before retrying.",
            ));
        }

        Ok(json!({
            "message": "Mathematical Claim Engine instance initialized.",
            "dry_run": false,
            "actor": actor,
            "idempotency_key": idempotency_key,
            "root": config.root,
            "data_dir": config.data_dir,
            "database": config.database,
            "artifacts": config.artifacts,
            "migration_version": store.migration_version()?,
            "journal_mode": store.journal_mode()?,
            "artifact_canary": canary_hash,
        }))
    }

    pub fn health(config: &ResolvedConfig) -> DiagnosticReport {
        let mut checks = Vec::new();
        if !config.database.is_file() {
            checks.push(failed_check(
                "database",
                format!("database does not exist at {}", config.database.display()),
                "Run `mcl init` for this instance root.",
            ));
        } else {
            match Store::open(&config.database).and_then(|store| {
                let integrity = store.integrity_check()?;
                let migration = store.migration_version()?;
                let journal = store.journal_mode()?;
                store.schema_check()?;
                store.fts5_check()?;
                Ok((integrity, migration, journal))
            }) {
                Ok((integrity, migration, journal))
                    if integrity == "ok" && migration >= 1 && journal == "wal" =>
                {
                    checks.push(passed_check(
                        "database",
                        format!(
                            "integrity={integrity}, migration={migration}, journal={journal}, schema=complete, fts5=available"
                        ),
                    ));
                }
                Ok((integrity, migration, journal)) => checks.push(failed_check(
                    "database",
                    format!("integrity={integrity}, migration={migration}, journal={journal}"),
                    "Run `mcl doctor`; restore a verified backup if integrity failed.",
                )),
                Err(error) => checks.push(failed_check(
                    "database",
                    error.to_string(),
                    &error.corrective_action,
                )),
            }
        }

        if !config.artifacts.is_dir() {
            checks.push(failed_check(
                "artifact_store",
                format!(
                    "artifact directory does not exist at {}",
                    config.artifacts.display()
                ),
                "Run `mcl init` for this instance root.",
            ));
        } else {
            match ArtifactStore::open(&config.artifacts) {
                Ok(_) => checks.push(passed_check(
                    "artifact_store",
                    format!(
                        "root={} is contained and readable",
                        config.artifacts.display()
                    ),
                )),
                Err(error) => checks.push(failed_check(
                    "artifact_store",
                    error.to_string(),
                    &error.corrective_action,
                )),
            }
        }

        report("health", checks)
    }

    pub fn doctor(config: &ResolvedConfig) -> DiagnosticReport {
        let mut report = Self::health(config);
        report.profile = "doctor";

        if config.artifacts.is_dir() {
            match ArtifactStore::open(&config.artifacts).and_then(|store| {
                let hash = store.put(DOCTOR_CANARY)?;
                let bytes = store.read(&hash)?;
                Ok((hash, bytes == DOCTOR_CANARY))
            }) {
                Ok((hash, true)) => report.checks.push(passed_check(
                    "artifact_round_trip",
                    format!("canary={hash}"),
                )),
                Ok((_, false)) => report.checks.push(failed_check(
                    "artifact_round_trip",
                    "artifact canary bytes changed",
                    "Quarantine the artifact store and restore a verified backup.",
                )),
                Err(error) => report.checks.push(failed_check(
                    "artifact_round_trip",
                    error.to_string(),
                    &error.corrective_action,
                )),
            }
        }

        if config.database.is_file() {
            match Store::open(&config.database).and_then(|store| store.stale_lease_count()) {
                Ok(count) => report
                    .checks
                    .push(passed_check("job_leases", format!("stale_leases={count}"))),
                Err(error) => report.checks.push(failed_check(
                    "job_leases",
                    error.to_string(),
                    &error.corrective_action,
                )),
            }

            match Store::open(&config.database).and_then(|store| store.environment_count()) {
                Ok(count) => report.checks.push(passed_check(
                    "environments",
                    format!(
                        "registered={count}; environment identity does not itself establish proof authority"
                    ),
                )),
                Err(error) => report.checks.push(failed_check(
                    "environments",
                    error.to_string(),
                    &error.corrective_action,
                )),
            }

            match Store::open(&config.database).and_then(|store| store.artifact_count()) {
                Ok(count) => report.checks.push(passed_check(
                    "artifacts",
                    format!(
                        "registered={count}; artifact presence does not itself establish mathematical authority"
                    ),
                )),
                Err(error) => report.checks.push(failed_check(
                    "artifacts",
                    error.to_string(),
                    &error.corrective_action,
                )),
            }

            let inventory = ArtifactStore::open(&config.artifacts).and_then(|artifacts| {
                let scan = artifacts.scan()?;
                let registered = Store::open(&config.database)?
                    .all_artifact_hashes()?
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                let canary_hash = format!("{:x}", Sha256::digest(DOCTOR_CANARY));
                let stored = scan.content_hashes.iter().cloned().collect::<BTreeSet<_>>();
                let orphan_count = stored
                    .difference(&registered)
                    .filter(|hash| hash.as_str() != canary_hash)
                    .count();
                let missing_count = registered.difference(&stored).count();
                Ok((orphan_count, missing_count, scan.incomplete_temporary_files))
            });
            match inventory {
                Ok((orphans, 0, temporary)) => report.checks.push(passed_check(
                    "artifact_inventory",
                    format!(
                        "unregistered_orphans={orphans}, incomplete_temporary_files={temporary}, missing_registered=0; orphans are retained but never canonical"
                    ),
                )),
                Ok((orphans, missing, temporary)) => report.checks.push(failed_check(
                    "artifact_inventory",
                    format!(
                        "unregistered_orphans={orphans}, incomplete_temporary_files={temporary}, missing_registered={missing}"
                    ),
                    "Quarantine the instance and restore missing registered artifacts from a verified backup.",
                )),
                Err(error) => report.checks.push(failed_check(
                    "artifact_inventory",
                    error.to_string(),
                    &error.corrective_action,
                )),
            }
        }

        match lean_version(&config.verifier.lean_command) {
            Ok(version) => report
                .checks
                .push(passed_check("lean", version.trim().to_owned())),
            Err(error) => report.checks.push(failed_check(
                "lean",
                error,
                "Install the pinned Lean 4 toolchain and ensure `lean` is available on PATH.",
            )),
        }

        report.healthy = report.checks.iter().all(|check| check.healthy);
        report
    }

    pub fn register_environment(
        &mut self,
        manifest: &EnvironmentManifest,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<EnvironmentRegistrationOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let (proposed_environment_hash, environment) = if dry_run {
            (
                self.store.validate_environment_registration(manifest)?,
                None,
            )
        } else {
            let environment = self
                .store
                .register_environment(manifest, actor, idempotency_key)?;
            (environment.environment_hash.clone(), Some(environment))
        };
        Ok(EnvironmentRegistrationOutcome {
            dry_run,
            proposed_environment_hash,
            environment,
        })
    }

    pub fn get_environment(&self, environment_hash: &str) -> Result<EnvironmentSnapshot, AppError> {
        self.store.get_environment(environment_hash)
    }

    pub fn list_environments(&self, limit: usize) -> Result<Vec<EnvironmentSnapshot>, AppError> {
        self.store.list_environments(limit)
    }

    pub fn ingest_artifact(
        &mut self,
        bytes: &[u8],
        metadata: &ArtifactMetadata,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<ArtifactIngestOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        if metadata.creation_source != crate::domain::ArtifactCreationSource::UserIngest {
            return Err(AppError::new(
                "MCL_ARTIFACT_CREATION_SOURCE_FORBIDDEN",
                "public artifact ingestion can create only user-ingested provenance",
                false,
                "Use the controlled verifier, import, migration, or generation path for system-produced artifacts.",
            ));
        }
        metadata.validate_bytes(bytes)?;
        let proposed_artifact_hash = format!("{:x}", Sha256::digest(bytes));
        self.store.validate_artifact_registration(
            &proposed_artifact_hash,
            bytes.len() as u64,
            metadata,
        )?;
        let artifact = if dry_run {
            None
        } else {
            let stored_hash = self.artifacts.put(bytes)?;
            if stored_hash != proposed_artifact_hash {
                return Err(AppError::new(
                    "MCL_ARTIFACT_INTEGRITY_FAILED",
                    "content-addressed store returned an unexpected artifact identity",
                    false,
                    "Quarantine the artifact store and restore a verified backup.",
                ));
            }
            Some(self.store.register_artifact(
                &stored_hash,
                bytes.len() as u64,
                metadata,
                actor,
                idempotency_key,
            )?)
        };
        Ok(ArtifactIngestOutcome {
            dry_run,
            proposed_artifact_hash,
            artifact,
        })
    }

    pub fn get_artifact(&self, artifact_hash: &str) -> Result<ArtifactSnapshot, AppError> {
        self.store.get_artifact(artifact_hash)
    }

    pub fn list_artifacts(&self, limit: usize) -> Result<Vec<ArtifactSnapshot>, AppError> {
        self.store.list_artifacts(limit)
    }

    pub fn verify_artifact(
        &self,
        artifact_hash: &str,
    ) -> Result<ArtifactVerificationReport, AppError> {
        let artifact = self.store.get_artifact(artifact_hash)?;
        let bytes = self.artifacts.read(artifact_hash)?;
        let metadata = ArtifactMetadata {
            schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: artifact.media_type,
            creation_source: artifact.creation_source,
            license_expression: artifact.license_expression.clone(),
            restriction: artifact.restriction,
            semantic_metadata: artifact.semantic_metadata.clone(),
        };
        metadata.validate_bytes(&bytes)?;
        if bytes.len() as u64 != artifact.byte_size {
            return Err(AppError::new(
                "MCL_ARTIFACT_INTEGRITY_FAILED",
                format!("artifact {artifact_hash} byte size disagrees with canonical metadata"),
                false,
                "Quarantine the artifact store and restore a verified backup.",
            ));
        }
        Ok(ArtifactVerificationReport {
            artifact,
            content_hash_verified: true,
            metadata_verified: true,
        })
    }

    pub fn enqueue_verifier_job(
        &mut self,
        request: &VerifierJobRequest,
        priority: i32,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<VerifierEnqueueOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let proposed_input_hash = self
            .store
            .validate_verifier_job_enqueue(request, priority)?;
        let job = if dry_run {
            None
        } else {
            Some(
                self.store
                    .enqueue_verifier_job(request, priority, actor, idempotency_key)?,
            )
        };
        Ok(VerifierEnqueueOutcome {
            dry_run,
            proposed_input_hash,
            job,
        })
    }

    pub fn get_verifier_job(&self, job_id: &str) -> Result<VerifierJobSnapshot, AppError> {
        self.store.get_verifier_job(job_id)
    }

    pub fn list_verifier_jobs(&self, limit: usize) -> Result<Vec<VerifierJobSnapshot>, AppError> {
        self.store.list_verifier_jobs(limit)
    }

    pub fn promote_verifier_diagnostic(
        &mut self,
        formalization_object_id: &str,
        formalization_version_hash: &str,
        job_id: &str,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<EvidencePromotionOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let formalization = self.store.get_record_version(formalization_version_hash)?;
        if formalization.object_id != formalization_object_id
            || formalization.kind != RecordKind::Formalization
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_SUBJECT_INVALID",
                "evidence subject is not the requested exact formalization version",
                false,
                "Use one exact canonical formalization object and version.",
            ));
        }
        let formal_payload: FormalizationPayload = serde_json::from_value(formalization.payload)
            .map_err(|error| {
                AppError::new(
                    "MCL_EVIDENCE_SUBJECT_INVALID",
                    error.to_string(),
                    false,
                    "Quarantine an invalid stored formalization and restore a verified backup.",
                )
            })?;
        let job = self.store.get_verifier_job(job_id)?;
        let report_hash = job.result_artifact_hash.as_deref().ok_or_else(|| {
            AppError::new(
                "MCL_EVIDENCE_JOB_INVALID",
                "verifier job has no terminal report artifact",
                true,
                "Wait for the exact verifier job to finish.",
            )
        })?;
        if formal_payload.environment_hash != job.request.environment_hash
            || formal_payload.module_artifact_hash != job.request.module_artifact_hash
            || formal_payload.declaration_name != job.request.declaration_name
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_FORMALIZATION_MISMATCH",
                "formalization and verifier request do not describe the same exact target",
                false,
                "Run verification against the exact formalization environment, module, and declaration.",
            ));
        }
        let report_artifact = self.store.get_artifact(report_hash)?;
        if report_artifact.media_type != crate::domain::ArtifactMediaType::Json
            || report_artifact.creation_source != crate::domain::ArtifactCreationSource::Verifier
            || report_artifact.restriction != crate::domain::ArtifactRestriction::Private
            || report_artifact
                .semantic_metadata
                .get("job_id")
                .is_none_or(|value| value != &job.job_id)
            || report_artifact
                .semantic_metadata
                .get("artifact_role")
                .is_none_or(|value| value != "verifier_report")
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_REPORT_INVALID",
                "verifier result artifact lacks controlled report provenance",
                false,
                "Quarantine the verifier result and rerun the exact job.",
            ));
        }
        let report_bytes = self.artifacts.read(report_hash)?;
        let report: VerifierExecutionReport =
            serde_json::from_slice(&report_bytes).map_err(|error| {
                AppError::new(
                    "MCL_EVIDENCE_REPORT_INVALID",
                    format!("verifier report is invalid: {error}"),
                    false,
                    "Quarantine the verifier result and rerun the exact job.",
                )
            })?;
        report.validate().map_err(|error| {
            AppError::new(
                "MCL_EVIDENCE_REPORT_INVALID",
                error.message,
                false,
                "Quarantine the verifier result and rerun the exact job.",
            )
        })?;
        if report.job_id != job.job_id
            || report.environment_hash != job.request.environment_hash
            || report.module_artifact_hash != job.request.module_artifact_hash
            || report.declaration_name != job.request.declaration_name
            || report.authoritative
        {
            return Err(AppError::new(
                "MCL_EVIDENCE_REPORT_MISMATCH",
                "verifier report does not match the exact terminal job",
                false,
                "Quarantine the verifier result and rerun the exact job.",
            ));
        }
        let result = match report.classification {
            VerifierExecutionClassification::Elaborated => EvidenceResult::Accepted,
            VerifierExecutionClassification::Rejected
            | VerifierExecutionClassification::UnsafeSource => EvidenceResult::Rejected,
            VerifierExecutionClassification::TimedOut
            | VerifierExecutionClassification::OutputLimitExceeded
            | VerifierExecutionClassification::ToolchainMismatch
            | VerifierExecutionClassification::LaunchFailed => EvidenceResult::Failed,
        };
        let mut artifact_hashes = vec![
            job.request.module_artifact_hash.clone(),
            report_hash.to_owned(),
        ];
        artifact_hashes.extend(report.stdout_artifact_hash.iter().cloned());
        artifact_hashes.extend(report.stderr_artifact_hash.iter().cloned());
        artifact_hashes.sort();
        artifact_hashes.dedup();
        let verifier_identity = format!(
            "lean:{}",
            report
                .observed_toolchain_version
                .as_deref()
                .unwrap_or("source-policy")
        );
        let payload = EvidencePayload {
            schema_version: crate::domain::evidence::EVIDENCE_SCHEMA_VERSION.to_owned(),
            subject: ExactVersionReference {
                object_id: formalization_object_id.to_owned(),
                version_hash: formalization_version_hash.to_owned(),
            },
            evidence_kind: EvidenceKind::LeanElaboration,
            result,
            authority_class: EvidenceAuthorityClass::Diagnostic,
            producing_run_id: None,
            producing_job_id: Some(job.job_id),
            artifact_hashes,
            verifier_or_reviewer_identity: verifier_identity,
            environment_hash: Some(report.environment_hash),
            supersedes_evidence_id: None,
            stale: false,
            stale_reason: None,
        };
        let proposed_evidence_hash = payload.evidence_hash()?;
        let evidence = if dry_run {
            None
        } else {
            Some(
                self.store
                    .create_diagnostic_evidence(&payload, actor, idempotency_key)?,
            )
        };
        Ok(EvidencePromotionOutcome {
            dry_run,
            proposed_evidence_hash,
            evidence,
        })
    }

    pub fn get_evidence(&self, evidence_id: &str) -> Result<EvidenceSnapshot, AppError> {
        self.store.get_evidence(evidence_id)
    }

    pub fn list_evidence(&self, limit: usize) -> Result<Vec<EvidenceSnapshot>, AppError> {
        self.store.list_evidence(limit)
    }

    pub fn review_fidelity(
        &mut self,
        request: &FidelityReviewRequest,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<FidelityReviewOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        request.validate()?;
        if request.reviewer_identity != actor {
            return Err(AppError::new(
                "MCL_FIDELITY_REVIEWER_MISMATCH",
                "reviewer identity must equal the attributed actor",
                false,
                "Submit the review under the reviewer's own actor identity.",
            ));
        }
        let source = self
            .store
            .get_record_version(&request.source.version_hash)?;
        let claim = self.store.get_record_version(&request.claim.version_hash)?;
        let formalization = self
            .store
            .get_record_version(&request.formalization.version_hash)?;
        if source.object_id != request.source.object_id
            || source.kind != RecordKind::Source
            || claim.object_id != request.claim.object_id
            || claim.kind != RecordKind::Claim
            || formalization.object_id != request.formalization.object_id
            || formalization.kind != RecordKind::Formalization
        {
            return Err(AppError::new(
                "MCL_FIDELITY_REFERENCE_INVALID",
                "fidelity review references do not resolve to the requested exact object families",
                false,
                "Use exact source, claim, and formalization versions.",
            ));
        }
        let source_payload: SourcePayload = decode_review_payload(source.payload, "source")?;
        let claim_payload: ClaimPayload = decode_review_payload(claim.payload, "claim")?;
        let formal_payload: FormalizationPayload =
            decode_review_payload(formalization.payload, "formalization")?;
        if claim_payload.source_reference != request.source
            || formal_payload.claim_version != request.claim
        {
            return Err(AppError::new(
                "MCL_FIDELITY_LINEAGE_MISMATCH",
                "source, claim, and formalization do not form one exact lineage",
                false,
                "Review the exact source referenced by the claim and the exact claim referenced by the formalization.",
            ));
        }
        if !claim_payload.ambiguity_notes.is_empty()
            && request.ambiguity_disposition == crate::domain::AmbiguityDisposition::NoAmbiguity
        {
            return Err(AppError::new(
                "MCL_FIDELITY_AMBIGUITY_INVALID",
                "claim ambiguity cannot be silently discarded by the review",
                false,
                "Preserve, resolve, or leave the recorded ambiguity explicitly unresolved.",
            ));
        }
        if (request.review_level == crate::domain::FidelityReviewLevel::SourcePaperCorrespondence
            && source_payload.source_type != SourceType::Paper)
            || (request.review_level == crate::domain::FidelityReviewLevel::BenchmarkHashAlignment
                && (source_payload.source_type != SourceType::Benchmark
                    || source_payload.content_hash.is_none()))
        {
            return Err(AppError::new(
                "MCL_FIDELITY_LEVEL_INVALID",
                "review level is not supported by the exact source type and identity",
                false,
                "Use a paper source for paper correspondence or a content-hashed benchmark source for benchmark alignment.",
            ));
        }
        let current = self.fidelity_status(&request.formalization)?;
        let may_be_exact_retry = current
            .history
            .iter()
            .any(|entry| entry.report.request == *request);
        if request.supersedes_evidence_id != current.head_evidence_id && !may_be_exact_retry {
            return Err(AppError::new(
                "MCL_FIDELITY_REVIEW_CONFLICT",
                "fidelity review does not supersede the current exact evidence head",
                true,
                "Reload fidelity status and retry against the current evidence head.",
            ));
        }
        self.store.get_run(&request.producing_run_id)?;
        for hash in &request.supporting_artifact_hashes {
            let artifact = self.store.get_artifact(hash)?;
            if artifact.creation_source == crate::domain::ArtifactCreationSource::HumanReview
                && artifact
                    .semantic_metadata
                    .get("artifact_role")
                    .is_some_and(|role| role == "fidelity_review_report")
            {
                return Err(AppError::new(
                    "MCL_FIDELITY_SUPPORT_INVALID",
                    "a controlled fidelity report cannot be supplied as supporting evidence",
                    false,
                    "Reference primary source artifacts and let MathOS create the review report.",
                ));
            }
            self.artifacts.read(hash)?;
        }
        let report = FidelityReviewReport {
            schema_version: crate::domain::fidelity::FIDELITY_REVIEW_REPORT_SCHEMA_VERSION
                .to_owned(),
            request_hash: request.request_hash()?,
            request: request.clone(),
            formalization_author: formalization.created_by,
            exact_theorem_type: formal_payload.exact_theorem_type,
            declaration_hash: formal_payload.declaration_hash,
        };
        report.validate()?;
        let report_bytes = canonical_json(&serde_json::to_value(&report).map_err(|error| {
            AppError::new(
                "MCL_FIDELITY_REPORT_INVALID",
                error.to_string(),
                false,
                "Report this deterministic fidelity report serialization defect.",
            )
        })?)?;
        let proposed_report_artifact_hash = format!("{:x}", Sha256::digest(&report_bytes));
        let mut artifact_hashes = request.supporting_artifact_hashes.clone();
        artifact_hashes.push(proposed_report_artifact_hash.clone());
        artifact_hashes.sort();
        artifact_hashes.dedup();
        let payload = EvidencePayload {
            schema_version: crate::domain::evidence::EVIDENCE_SCHEMA_VERSION.to_owned(),
            subject: request.formalization.clone(),
            evidence_kind: EvidenceKind::StatementFidelityReview,
            result: if request.verdict == FidelityVerdict::Rejected {
                EvidenceResult::Rejected
            } else {
                EvidenceResult::Accepted
            },
            authority_class: EvidenceAuthorityClass::Reviewed,
            producing_run_id: Some(request.producing_run_id.clone()),
            producing_job_id: None,
            artifact_hashes,
            verifier_or_reviewer_identity: actor.to_owned(),
            environment_hash: None,
            supersedes_evidence_id: request.supersedes_evidence_id.clone(),
            stale: false,
            stale_reason: None,
        };
        let proposed_evidence_hash = payload.evidence_hash()?;
        let evidence = if dry_run {
            None
        } else {
            let metadata = ArtifactMetadata {
                schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION
                    .to_owned(),
                media_type: crate::domain::ArtifactMediaType::Json,
                creation_source: crate::domain::ArtifactCreationSource::HumanReview,
                license_expression: None,
                restriction: crate::domain::ArtifactRestriction::Private,
                semantic_metadata: BTreeMap::from([
                    (
                        "artifact_role".to_owned(),
                        "fidelity_review_report".to_owned(),
                    ),
                    ("reviewer_identity".to_owned(), actor.to_owned()),
                    ("request_hash".to_owned(), report.request_hash.clone()),
                ]),
            };
            self.ensure_review_artifact(
                &report_bytes,
                &metadata,
                actor,
                &format!("{idempotency_key}:report"),
            )?;
            Some(
                self.store
                    .create_fidelity_evidence(&payload, actor, idempotency_key)?,
            )
        };
        Ok(FidelityReviewOutcome {
            dry_run,
            proposed_report_artifact_hash,
            proposed_evidence_hash,
            report,
            evidence,
        })
    }

    pub fn fidelity_status(
        &self,
        formalization: &ExactVersionReference,
    ) -> Result<FidelityStatusSnapshot, AppError> {
        let record = self.store.get_record_version(&formalization.version_hash)?;
        if record.object_id != formalization.object_id || record.kind != RecordKind::Formalization {
            return Err(AppError::new(
                "MCL_FIDELITY_SUBJECT_INVALID",
                "fidelity status subject is not the requested exact formalization version",
                false,
                "Use one exact canonical formalization object and version.",
            ));
        }
        let evidence = self.store.list_fidelity_evidence(formalization)?;
        if evidence.is_empty() {
            return Ok(FidelityStatusSnapshot {
                formalization: formalization.clone(),
                status: FidelityStatus::Unreviewed,
                head_evidence_id: None,
                history: Vec::new(),
            });
        }

        let ids = evidence
            .iter()
            .map(|entry| entry.evidence_id.as_str())
            .collect::<BTreeSet<_>>();
        let superseded = evidence
            .iter()
            .filter_map(|entry| entry.payload.supersedes_evidence_id.as_deref())
            .collect::<BTreeSet<_>>();
        if superseded.iter().any(|id| !ids.contains(id)) {
            return Err(fidelity_integrity_error(
                "fidelity review history supersedes evidence outside its exact subject chain",
            ));
        }
        let heads = ids
            .iter()
            .copied()
            .filter(|id| !superseded.contains(id))
            .collect::<Vec<_>>();
        let [head_evidence_id] = heads.as_slice() else {
            return Err(fidelity_integrity_error(
                "fidelity review history does not have exactly one current head",
            ));
        };
        let head_evidence_id = (*head_evidence_id).to_owned();

        let mut history = Vec::with_capacity(evidence.len());
        let mut head_status = None;
        for entry in evidence.into_iter().rev() {
            let (report_artifact_hash, report) = self.read_fidelity_report(&entry)?;
            let status = if entry.evidence_id == head_evidence_id {
                let status = fidelity_status_from_verdict(report.request.verdict);
                head_status = Some(status);
                status
            } else {
                FidelityStatus::Superseded
            };
            history.push(FidelityReviewHistoryEntry {
                status,
                evidence: entry,
                report_artifact_hash,
                report,
            });
        }
        let status = head_status.ok_or_else(|| {
            fidelity_integrity_error("fidelity review head did not resolve to its report")
        })?;
        Ok(FidelityStatusSnapshot {
            formalization: formalization.clone(),
            status,
            head_evidence_id: Some(head_evidence_id),
            history,
        })
    }

    fn read_fidelity_report(
        &self,
        evidence: &EvidenceSnapshot,
    ) -> Result<(String, FidelityReviewReport), AppError> {
        let mut reports = Vec::new();
        for hash in &evidence.payload.artifact_hashes {
            let artifact = self.store.get_artifact(hash)?;
            if artifact.media_type == crate::domain::ArtifactMediaType::Json
                && artifact.creation_source == crate::domain::ArtifactCreationSource::HumanReview
                && artifact.restriction == crate::domain::ArtifactRestriction::Private
                && artifact
                    .semantic_metadata
                    .get("artifact_role")
                    .is_some_and(|role| role == "fidelity_review_report")
            {
                let bytes = self.artifacts.read(hash)?;
                let report: FidelityReviewReport =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        fidelity_integrity_error(format!(
                            "stored fidelity report is invalid: {error}"
                        ))
                    })?;
                report.validate().map_err(|error| {
                    fidelity_integrity_error(format!(
                        "stored fidelity report failed validation: {}",
                        error.message
                    ))
                })?;
                reports.push((hash.clone(), artifact, report));
            }
        }
        let [(hash, artifact, report)] = reports.as_slice() else {
            return Err(fidelity_integrity_error(
                "fidelity evidence does not resolve to exactly one controlled review report",
            ));
        };
        if report.request.formalization != evidence.payload.subject
            || report.request.producing_run_id.as_str()
                != evidence
                    .payload
                    .producing_run_id
                    .as_deref()
                    .unwrap_or_default()
            || report.request.supersedes_evidence_id != evidence.payload.supersedes_evidence_id
            || report.request.reviewer_identity != evidence.payload.verifier_or_reviewer_identity
            || artifact.semantic_metadata.get("request_hash") != Some(&report.request_hash)
            || artifact.semantic_metadata.get("reviewer_identity")
                != Some(&report.request.reviewer_identity)
        {
            return Err(fidelity_integrity_error(
                "fidelity report, evidence, and artifact provenance disagree",
            ));
        }
        Ok((hash.clone(), report.clone()))
    }

    pub fn enqueue_audit_job(
        &mut self,
        subject: &ExactVersionReference,
        diagnostic_evidence_id: &str,
        priority: i32,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<AuditEnqueueOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let formalization = self.store.get_record_version(&subject.version_hash)?;
        if formalization.object_id != subject.object_id
            || formalization.kind != RecordKind::Formalization
        {
            return Err(AppError::new(
                "MCL_AUDIT_SUBJECT_INVALID",
                "audit subject is not the requested exact formalization version",
                false,
                "Use one exact canonical formalization object and version.",
            ));
        }
        let formal_payload: FormalizationPayload = serde_json::from_value(formalization.payload)
            .map_err(|error| {
                AppError::new(
                    "MCL_AUDIT_SUBJECT_INVALID",
                    error.to_string(),
                    false,
                    "Quarantine an invalid stored formalization and restore a verified backup.",
                )
            })?;
        let evidence = self.store.get_evidence(diagnostic_evidence_id)?;
        if evidence.payload.subject != *subject
            || evidence.payload.evidence_kind != EvidenceKind::LeanElaboration
            || evidence.payload.result != EvidenceResult::Accepted
            || evidence.payload.authority_class != EvidenceAuthorityClass::Diagnostic
            || evidence.payload.stale
            || evidence.payload.environment_hash.as_deref()
                != Some(formal_payload.environment_hash.as_str())
            || !evidence
                .payload
                .artifact_hashes
                .iter()
                .any(|hash| hash == &formal_payload.module_artifact_hash)
        {
            return Err(AppError::new(
                "MCL_AUDIT_EVIDENCE_INVALID",
                "audit requires accepted diagnostic elaboration evidence for the exact formalization",
                false,
                "Promote an accepted exact verifier diagnostic before requesting an audit.",
            ));
        }
        let policy = crate::domain::audit::committed_audit_policy()?;
        let request = LeanAuditRequest {
            schema_version: crate::domain::audit::AUDIT_REQUEST_SCHEMA_VERSION.to_owned(),
            subject: subject.clone(),
            diagnostic_evidence_id: evidence.evidence_id,
            diagnostic_evidence_hash: evidence.evidence_hash,
            environment_hash: formal_payload.environment_hash,
            module_artifact_hash: formal_payload.module_artifact_hash,
            declaration_name: formal_payload.declaration_name,
            policy_hash: policy.policy_hash()?,
        };
        let proposed_input_hash = self.store.validate_audit_job_enqueue(&request, priority)?;
        let job = if dry_run {
            None
        } else {
            Some(
                self.store
                    .enqueue_audit_job(&request, priority, actor, idempotency_key)?,
            )
        };
        Ok(AuditEnqueueOutcome {
            dry_run,
            proposed_input_hash,
            job,
        })
    }

    pub fn get_audit_job(&self, job_id: &str) -> Result<LeanAuditJobSnapshot, AppError> {
        self.store.get_audit_job(job_id)
    }

    pub fn list_audit_jobs(&self, limit: usize) -> Result<Vec<LeanAuditJobSnapshot>, AppError> {
        self.store.list_audit_jobs(limit)
    }

    pub fn promote_audit_evidence(
        &mut self,
        formalization_object_id: &str,
        formalization_version_hash: &str,
        audit_job_id: &str,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<AuditEvidencePromotionOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let formalization = self.store.get_record_version(formalization_version_hash)?;
        if formalization.object_id != formalization_object_id
            || formalization.kind != RecordKind::Formalization
        {
            return Err(AppError::new(
                "MCL_AUDIT_SUBJECT_INVALID",
                "audit evidence subject is not the requested exact formalization version",
                false,
                "Use one exact canonical formalization object and version.",
            ));
        }
        let job = self.store.get_audit_job(audit_job_id)?;
        if job.request.subject.object_id != formalization_object_id
            || job.request.subject.version_hash != formalization_version_hash
        {
            return Err(AppError::new(
                "MCL_AUDIT_EVIDENCE_MISMATCH",
                "audit job does not target the requested exact formalization",
                false,
                "Promote evidence only against the audit job's exact subject.",
            ));
        }
        let report_hash = job.result_artifact_hash.as_deref().ok_or_else(|| {
            AppError::new(
                "MCL_AUDIT_EVIDENCE_JOB_INVALID",
                "audit job has no terminal report artifact",
                true,
                "Wait for the exact audit job to finish.",
            )
        })?;
        let report_artifact = self.store.get_artifact(report_hash)?;
        if report_artifact.media_type != crate::domain::ArtifactMediaType::Json
            || report_artifact.creation_source != crate::domain::ArtifactCreationSource::Verifier
            || report_artifact.restriction != crate::domain::ArtifactRestriction::Private
            || report_artifact
                .semantic_metadata
                .get("job_id")
                .is_none_or(|value| value != &job.job_id)
            || report_artifact
                .semantic_metadata
                .get("artifact_role")
                .is_none_or(|value| value != "audit_report")
        {
            return Err(AppError::new(
                "MCL_AUDIT_REPORT_INVALID",
                "audit result artifact lacks controlled private report provenance",
                false,
                "Quarantine the audit result and rerun the exact job.",
            ));
        }
        let report_bytes = self.artifacts.read(report_hash)?;
        let report: LeanAuditReport = serde_json::from_slice(&report_bytes).map_err(|error| {
            AppError::new(
                "MCL_AUDIT_REPORT_INVALID",
                format!("audit report is invalid: {error}"),
                false,
                "Quarantine the audit result and rerun the exact job.",
            )
        })?;
        let policy = crate::domain::audit::committed_audit_policy()?;
        report.validate_against_policy(&policy)?;
        if report.job_id != job.job_id
            || report.request_hash != job.canonical_input_hash
            || report.subject != job.request.subject
            || report.diagnostic_evidence_hash != job.request.diagnostic_evidence_hash
            || report.environment_hash != job.request.environment_hash
            || report.module_artifact_hash != job.request.module_artifact_hash
            || report.declaration_name != job.request.declaration_name
            || report.policy_hash != job.request.policy_hash
            || report.authoritative
        {
            return Err(AppError::new(
                "MCL_AUDIT_REPORT_MISMATCH",
                "audit report does not match the exact terminal job",
                false,
                "Quarantine the audit result and rerun the exact job.",
            ));
        }
        let result = match report.classification {
            LeanAuditClassification::Passed => EvidenceResult::Accepted,
            LeanAuditClassification::Rejected => EvidenceResult::Rejected,
            LeanAuditClassification::Inconclusive => EvidenceResult::Inconclusive,
            LeanAuditClassification::Failed => EvidenceResult::Failed,
        };
        let mut artifact_hashes = vec![
            job.request.module_artifact_hash.clone(),
            report_hash.to_owned(),
        ];
        artifact_hashes.extend(report.stdout_artifact_hash.iter().cloned());
        artifact_hashes.extend(report.stderr_artifact_hash.iter().cloned());
        artifact_hashes.sort();
        artifact_hashes.dedup();
        let identity = format!(
            "lean-audit:{}",
            report
                .observed_toolchain_version
                .as_deref()
                .unwrap_or("source-policy")
        );
        let build = |evidence_kind| EvidencePayload {
            schema_version: crate::domain::evidence::EVIDENCE_SCHEMA_VERSION.to_owned(),
            subject: job.request.subject.clone(),
            evidence_kind,
            result,
            authority_class: EvidenceAuthorityClass::Diagnostic,
            producing_run_id: None,
            producing_job_id: Some(job.job_id.clone()),
            artifact_hashes: artifact_hashes.clone(),
            verifier_or_reviewer_identity: identity.clone(),
            environment_hash: Some(job.request.environment_hash.clone()),
            supersedes_evidence_id: None,
            stale: false,
            stale_reason: None,
        };
        let payloads = [
            build(EvidenceKind::ProofClosureScan),
            build(EvidenceKind::AxiomAudit),
        ];
        let proposed_evidence_hashes = payloads
            .iter()
            .map(EvidencePayload::evidence_hash)
            .collect::<Result<Vec<_>, _>>()?;
        let evidence = if dry_run {
            None
        } else {
            Some(
                self.store
                    .create_audit_evidence_pair(&payloads, actor, idempotency_key)?,
            )
        };
        Ok(AuditEvidencePromotionOutcome {
            dry_run,
            proposed_evidence_hashes,
            evidence,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_publication_request(
        &mut self,
        subject: &ExactVersionReference,
        outcome: PublicationOutcome,
        diagnostic_evidence_id: &str,
        proof_closure_evidence_id: &str,
        axiom_audit_evidence_id: &str,
        source_commit_sha: &str,
        source_tree_sha: &str,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<PublicationRequestPreparationOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let current = self.store.get_record(&subject.object_id)?;
        if current.version_hash != subject.version_hash {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_SUBJECT_STALE",
                "publication subject is no longer the current canonical formalization version",
                "Select the current formalization head and reproduce its exact diagnostic and audit evidence.",
            ));
        }
        let formalization = self.store.get_record_version(&subject.version_hash)?;
        if formalization.object_id != subject.object_id
            || formalization.kind != RecordKind::Formalization
        {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_SUBJECT_INVALID",
                "publication subject is not the requested exact formalization version",
                "Use one exact canonical formalization object and version.",
            ));
        }
        validate_record_payload(
            formalization.kind,
            &formalization.schema_version,
            &formalization.payload,
        )
        .map_err(|error| {
            publication_preparation_error(
                "MCL_PUBLICATION_SUBJECT_INVALID",
                format!(
                    "stored formalization payload fails validation: {}",
                    error.message
                ),
                "Quarantine the invalid formalization and restore a verified backup.",
            )
        })?;
        if record_version_hash(&formalization.schema_version, &formalization.payload)?
            != subject.version_hash
        {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_SUBJECT_INVALID",
                "stored formalization payload does not reproduce its exact version identity",
                "Quarantine the invalid formalization and restore a verified backup.",
            ));
        }
        let formal_payload: FormalizationPayload = serde_json::from_value(formalization.payload)
            .map_err(|error| {
                publication_preparation_error(
                    "MCL_PUBLICATION_SUBJECT_INVALID",
                    format!("stored formalization payload is invalid: {error}"),
                    "Quarantine the invalid formalization and restore a verified backup.",
                )
            })?;
        let bound_outcome = match formal_payload.claim_polarity {
            Some(FormalizationClaimPolarity::Claim) => PublicationOutcome::Proof,
            Some(FormalizationClaimPolarity::Negation) => PublicationOutcome::Refutation,
            None => {
                return Err(publication_preparation_error(
                    "MCL_PUBLICATION_OUTCOME_UNBOUND",
                    "formalization does not declare whether its exact theorem proves the claim or its negation",
                    "Create a new exact formalization version with typed claim_polarity before publication.",
                ));
            }
        };
        if outcome != bound_outcome {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_OUTCOME_MISMATCH",
                format!(
                    "requested {} outcome conflicts with the formalization's typed claim polarity",
                    outcome.as_str()
                ),
                format!(
                    "Use outcome `{}` or create and reverify a new exact formalization with the intended polarity.",
                    bound_outcome.as_str()
                ),
            ));
        }
        let environment = self
            .store
            .get_environment(&formal_payload.environment_hash)?;
        let module = self.verify_artifact(&formal_payload.module_artifact_hash)?;
        if module.artifact.media_type != crate::domain::ArtifactMediaType::LeanSource {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_ARTIFACT_INVALID",
                "publication formalization does not resolve to verified Lean source bytes",
                "Restore the exact registered Lean module before preparing publication.",
            ));
        }

        let diagnostic = self.store.get_evidence(diagnostic_evidence_id)?;
        let diagnostic_job_id = validate_publication_evidence(
            &diagnostic,
            subject,
            EvidenceKind::LeanElaboration,
            &formal_payload,
        )?;
        let proof_closure = self.store.get_evidence(proof_closure_evidence_id)?;
        let proof_closure_job_id = validate_publication_evidence(
            &proof_closure,
            subject,
            EvidenceKind::ProofClosureScan,
            &formal_payload,
        )?;
        let axiom_audit = self.store.get_evidence(axiom_audit_evidence_id)?;
        let axiom_audit_job_id = validate_publication_evidence(
            &axiom_audit,
            subject,
            EvidenceKind::AxiomAudit,
            &formal_payload,
        )?;
        if proof_closure_job_id != axiom_audit_job_id
            || proof_closure.payload.artifact_hashes != axiom_audit.payload.artifact_hashes
            || proof_closure.payload.verifier_or_reviewer_identity
                != axiom_audit.payload.verifier_or_reviewer_identity
        {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_EVIDENCE_MISMATCH",
                "proof-closure and axiom-audit evidence do not form one exact audit pair",
                "Select the two accepted evidence records promoted from one terminal audit job.",
            ));
        }

        let diagnostic_report_hash = self.verify_evidence_artifacts_and_find_report(
            &diagnostic,
            &diagnostic_job_id,
            "verifier_report",
        )?;
        let diagnostic_job = self.store.get_verifier_job(&diagnostic_job_id)?;
        let diagnostic_report: VerifierExecutionReport = serde_json::from_slice(
            &self.artifacts.read(&diagnostic_report_hash)?,
        )
        .map_err(|error| {
            publication_preparation_error(
                "MCL_PUBLICATION_EVIDENCE_INVALID",
                format!("diagnostic verifier report is invalid: {error}"),
                "Quarantine the diagnostic evidence and rerun the exact verifier job.",
            )
        })?;
        diagnostic_report.validate().map_err(|error| {
            publication_preparation_error(
                "MCL_PUBLICATION_EVIDENCE_INVALID",
                error.message,
                "Quarantine the diagnostic evidence and rerun the exact verifier job.",
            )
        })?;
        if diagnostic_job.state != VerifierJobState::Succeeded
            || diagnostic_job.request.environment_hash != formal_payload.environment_hash
            || diagnostic_job.request.module_artifact_hash != formal_payload.module_artifact_hash
            || diagnostic_job.request.declaration_name != formal_payload.declaration_name
            || diagnostic_job.result_artifact_hash.as_deref()
                != Some(diagnostic_report_hash.as_str())
            || diagnostic_report.job_id != diagnostic_job.job_id
            || diagnostic_report.environment_hash != formal_payload.environment_hash
            || diagnostic_report.module_artifact_hash != formal_payload.module_artifact_hash
            || diagnostic_report.declaration_name != formal_payload.declaration_name
            || diagnostic_report.classification != VerifierExecutionClassification::Elaborated
            || diagnostic_report.trust_profile != environment.manifest.trust_profile
            || diagnostic_report.authoritative
            || diagnostic.payload.artifact_hashes
                != report_artifact_closure(
                    &formal_payload.module_artifact_hash,
                    &diagnostic_report_hash,
                    diagnostic_report.stdout_artifact_hash.as_deref(),
                    diagnostic_report.stderr_artifact_hash.as_deref(),
                )
        {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_EVIDENCE_MISMATCH",
                "diagnostic evidence does not reproduce one accepted exact verifier result",
                "Select current accepted elaboration evidence for the exact formalization.",
            ));
        }

        let audit_report_hash = self.verify_evidence_artifacts_and_find_report(
            &proof_closure,
            &proof_closure_job_id,
            "audit_report",
        )?;
        let audit_job = self.store.get_audit_job(&proof_closure_job_id)?;
        let audit_report: LeanAuditReport =
            serde_json::from_slice(&self.artifacts.read(&audit_report_hash)?).map_err(|error| {
                publication_preparation_error(
                    "MCL_PUBLICATION_EVIDENCE_INVALID",
                    format!("audit report is invalid: {error}"),
                    "Quarantine the audit evidence and rerun the exact audit job.",
                )
            })?;
        let audit_policy = crate::domain::audit::committed_audit_policy()?;
        let audit_policy_hash = audit_policy.policy_hash()?;
        audit_report
            .validate_against_policy(&audit_policy)
            .map_err(|error| {
                publication_preparation_error(
                    "MCL_PUBLICATION_EVIDENCE_INVALID",
                    error.message,
                    "Quarantine the audit evidence and rerun the exact audit job.",
                )
            })?;
        if audit_job.state != VerifierJobState::Succeeded
            || audit_job.request.subject != *subject
            || audit_job.request.diagnostic_evidence_id != diagnostic.evidence_id
            || audit_job.request.diagnostic_evidence_hash != diagnostic.evidence_hash
            || audit_job.request.environment_hash != formal_payload.environment_hash
            || audit_job.request.module_artifact_hash != formal_payload.module_artifact_hash
            || audit_job.request.declaration_name != formal_payload.declaration_name
            || audit_job.request.policy_hash != audit_policy_hash
            || audit_job.result_artifact_hash.as_deref() != Some(audit_report_hash.as_str())
            || audit_report.job_id != audit_job.job_id
            || audit_report.request_hash != audit_job.canonical_input_hash
            || audit_report.subject != *subject
            || audit_report.diagnostic_evidence_hash != diagnostic.evidence_hash
            || audit_report.environment_hash != formal_payload.environment_hash
            || audit_report.module_artifact_hash != formal_payload.module_artifact_hash
            || audit_report.declaration_name != formal_payload.declaration_name
            || audit_report.policy_hash != audit_job.request.policy_hash
            || audit_report.classification != LeanAuditClassification::Passed
            || audit_report.trust_profile != environment.manifest.trust_profile
            || audit_report.authoritative
            || proof_closure.payload.artifact_hashes
                != report_artifact_closure(
                    &formal_payload.module_artifact_hash,
                    &audit_report_hash,
                    audit_report.stdout_artifact_hash.as_deref(),
                    audit_report.stderr_artifact_hash.as_deref(),
                )
        {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_EVIDENCE_MISMATCH",
                "audit evidence does not reproduce one accepted audit of the selected diagnostic",
                "Select the accepted proof-closure and axiom-audit pair derived from that diagnostic.",
            ));
        }
        self.verify_evidence_artifacts_and_find_report(
            &axiom_audit,
            &axiom_audit_job_id,
            "audit_report",
        )?;

        let policy = crate::domain::publication::committed_publication_policy()?;
        let request = PublicationRequest {
            schema_version: crate::domain::publication::PUBLICATION_REQUEST_SCHEMA_VERSION
                .to_owned(),
            subject: subject.clone(),
            outcome,
            diagnostic_evidence_id: diagnostic.evidence_id,
            diagnostic_evidence_hash: diagnostic.evidence_hash,
            proof_closure_evidence_id: proof_closure.evidence_id,
            proof_closure_evidence_hash: proof_closure.evidence_hash,
            axiom_audit_evidence_id: axiom_audit.evidence_id,
            axiom_audit_evidence_hash: axiom_audit.evidence_hash,
            environment_hash: formal_payload.environment_hash,
            module_artifact_hash: formal_payload.module_artifact_hash,
            declaration_name: formal_payload.declaration_name,
            policy_hash: policy.policy_hash()?,
            source_commit_sha: source_commit_sha.to_owned(),
            source_tree_sha: source_tree_sha.to_owned(),
        };
        let proposed_request_hash = request.request_hash()?;
        let request_bytes = canonical_json(&serde_json::to_value(&request).map_err(|error| {
            publication_preparation_error(
                "MCL_PUBLICATION_REQUEST_INVALID",
                error.to_string(),
                "Report this deterministic publication request serialization defect.",
            )
        })?)?;
        let proposed_artifact_hash = format!("{:x}", Sha256::digest(&request_bytes));
        if proposed_artifact_hash != proposed_request_hash {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_REQUEST_INVALID",
                "publication request identities disagree across canonical encodings",
                "Report this deterministic publication request hashing defect.",
            ));
        }
        let metadata = ArtifactMetadata {
            schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: crate::domain::ArtifactMediaType::Json,
            creation_source: crate::domain::ArtifactCreationSource::Generated,
            license_expression: None,
            restriction: crate::domain::ArtifactRestriction::Private,
            semantic_metadata: BTreeMap::from([
                ("artifact_role".to_owned(), "publication_request".to_owned()),
                ("request_hash".to_owned(), proposed_request_hash.clone()),
                (
                    "formalization_object_id".to_owned(),
                    subject.object_id.clone(),
                ),
                (
                    "formalization_version_hash".to_owned(),
                    subject.version_hash.clone(),
                ),
                ("source_commit_sha".to_owned(), source_commit_sha.to_owned()),
                ("source_tree_sha".to_owned(), source_tree_sha.to_owned()),
            ]),
        };
        metadata.validate_bytes(&request_bytes)?;
        self.store.validate_artifact_registration(
            &proposed_artifact_hash,
            request_bytes.len() as u64,
            &metadata,
        )?;
        match self.store.get_artifact(&proposed_artifact_hash) {
            Ok(existing)
                if artifact_matches_metadata(&existing, &metadata, request_bytes.len()) =>
            {
                self.verify_artifact(&proposed_artifact_hash)?;
            }
            Ok(_) => {
                return Err(publication_preparation_error(
                    "MCL_ARTIFACT_METADATA_CONFLICT",
                    format!(
                        "existing publication request artifact {proposed_artifact_hash} has incompatible metadata"
                    ),
                    "Quarantine the conflicting artifact and inspect its provenance.",
                ));
            }
            Err(error) if error.code == "MCL_ARTIFACT_NOT_FOUND" => {}
            Err(error) => return Err(error),
        }
        let artifact = if dry_run {
            None
        } else {
            let stored_hash = self.artifacts.put(&request_bytes)?;
            if stored_hash != proposed_artifact_hash {
                return Err(publication_preparation_error(
                    "MCL_ARTIFACT_INTEGRITY_FAILED",
                    "content-addressed store returned an unexpected publication request identity",
                    "Quarantine the artifact store and restore a verified backup.",
                ));
            }
            Some(self.store.register_publication_request_artifact(
                &stored_hash,
                request_bytes.len() as u64,
                &metadata,
                &request,
                actor,
                &format!("{idempotency_key}:publication-request"),
            )?)
        };
        Ok(PublicationRequestPreparationOutcome {
            dry_run,
            proposed_request_hash,
            proposed_artifact_hash,
            request,
            artifact,
        })
    }

    pub fn validate_publication_candidate(
        &mut self,
        report_bytes: &[u8],
        retained_closure_bytes: &[u8],
        retained_root: &Path,
    ) -> Result<PublicationCandidateValidationOutcome, AppError> {
        let candidate =
            validate_publication_candidate_documents(report_bytes, retained_closure_bytes)?;
        let retained_files =
            read_retained_publication_files(retained_root, &candidate.retained_closure)?;
        validate_retained_publication_semantics(
            &candidate.report,
            &candidate.retained_closure,
            &retained_files,
        )?;
        let request = candidate.report.request.clone();
        let rederived = self.prepare_publication_request(
            &request.subject,
            request.outcome,
            &request.diagnostic_evidence_id,
            &request.proof_closure_evidence_id,
            &request.axiom_audit_evidence_id,
            &request.source_commit_sha,
            &request.source_tree_sha,
            "publication-candidate-validator",
            &format!("validate-publication-candidate:{}", candidate.request_hash),
            true,
        )?;
        if rederived.request != request
            || rederived.proposed_request_hash != candidate.request_hash
            || rederived.proposed_artifact_hash != candidate.request_hash
            || rederived.artifact.is_some()
        {
            return Err(publication_candidate_error(
                "MCL_PUBLICATION_CANDIDATE_REQUEST_MISMATCH",
                "publication candidate request does not reproduce the current canonical request",
                "Regenerate the candidate from the current formalization head and its exact accepted evidence closure.",
            ));
        }
        validate_retained_store_snapshots(&self.store, &request, &retained_files)?;
        self.verify_artifact(&candidate.request_hash)
            .map_err(|error| {
                publication_candidate_error(
                    "MCL_PUBLICATION_REQUEST_ARTIFACT_INVALID",
                    format!(
                        "registered publication request artifact {} is missing or invalid: {}",
                        candidate.request_hash, error.message
                    ),
                    "Restore and verify the exact registered request artifact before validating its workflow candidate.",
                )
            })?;

        Ok(PublicationCandidateValidationOutcome {
            request_hash: candidate.request_hash,
            report_content_hash: candidate.report_content_hash.clone(),
            report_artifact_hash: candidate.report_content_hash,
            retained_closure_hash: candidate.retained_closure_hash.clone(),
            retained_closure_artifact_hash: candidate.retained_closure_hash,
            authoritative: false,
        })
    }

    pub fn work_one_verifier_job(
        &mut self,
        worker: &str,
        lease_seconds: u64,
    ) -> Result<Option<VerifierWorkOutcome>, AppError> {
        let Some(leased) = self.store.lease_next_verifier_job(worker, lease_seconds)? else {
            return Ok(None);
        };
        let running = self
            .store
            .mark_verifier_job_running(&leased.job_id, worker)?;
        let environment = self
            .store
            .get_environment(&running.request.environment_hash)?;
        if lease_seconds < environment.manifest.resource_limits.timeout_seconds + 60 {
            return Err(AppError::new(
                "MCL_VERIFIER_LEASE_TOO_SHORT",
                "worker lease does not cover the environment timeout plus cleanup margin",
                true,
                "Use a lease at least 60 seconds longer than the registered verifier timeout.",
            ));
        }
        let module = self
            .store
            .get_artifact(&running.request.module_artifact_hash)?;
        if module.media_type != crate::domain::ArtifactMediaType::LeanSource {
            return Err(AppError::new(
                "MCL_VERIFIER_ARTIFACT_INVALID",
                "leased verifier job no longer resolves to Lean source metadata",
                false,
                "Quarantine the database and restore a verified backup.",
            ));
        }
        let source = self.artifacts.read(&module.artifact_hash)?;
        let started = std::time::Instant::now();
        let forbidden = crate::verifier::scan_forbidden_source_token(&source)?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;
        let mut observed_toolchain_version = None;
        let mut process_error = None;
        let classification = if forbidden.is_some() {
            VerifierExecutionClassification::UnsafeSource
        } else {
            let workspace = tempfile::Builder::new()
                .prefix("mcl-lean-")
                .tempdir_in(&self.workspace_root)
                .map_err(|error| AppError::io("create verifier temporary workspace", error))?;
            self.artifacts.materialize(
                &module.artifact_hash,
                workspace.path(),
                "Submission.lean",
            )?;
            let mut driver = source.clone();
            driver.extend_from_slice(
                format!("\n#check {}\n", running.request.declaration_name).as_bytes(),
            );
            let driver_path = workspace.path().join("Driver.lean");
            let mut driver_file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&driver_path)
                .map_err(|error| AppError::io("create verifier driver", error))?;
            use std::io::Write as _;
            driver_file
                .write_all(&driver)
                .map_err(|error| AppError::io("write verifier driver", error))?;
            driver_file
                .sync_all()
                .map_err(|error| AppError::io("sync verifier driver", error))?;
            drop(driver_file);
            match crate::verifier::execute_lean(
                &self.verifier_command,
                workspace.path(),
                "Driver.lean",
                &environment.manifest,
            ) {
                Ok(result) => {
                    exit_code = result.exit_code;
                    stdout = result.stdout;
                    stderr = result.stderr;
                    observed_toolchain_version = Some(result.observed_toolchain_version);
                    if result.timed_out {
                        VerifierExecutionClassification::TimedOut
                    } else if result.output_limit_exceeded {
                        VerifierExecutionClassification::OutputLimitExceeded
                    } else if result.exit_code == Some(0) {
                        VerifierExecutionClassification::Elaborated
                    } else {
                        VerifierExecutionClassification::Rejected
                    }
                }
                Err(error) => {
                    let classification = match error.code {
                        "MCL_VERIFIER_VERSION_MISMATCH" => {
                            VerifierExecutionClassification::ToolchainMismatch
                        }
                        _ => VerifierExecutionClassification::LaunchFailed,
                    };
                    process_error = Some(error);
                    classification
                }
            }
        };
        let stdout_artifact_hash =
            self.register_verifier_output(&running, "stdout", &stdout, worker)?;
        let stderr_artifact_hash =
            self.register_verifier_output(&running, "stderr", &stderr, worker)?;
        let report = VerifierExecutionReport {
            schema_version: crate::domain::verifier::VERIFIER_EXECUTION_REPORT_SCHEMA_VERSION
                .to_owned(),
            job_id: running.job_id.clone(),
            environment_hash: running.request.environment_hash.clone(),
            module_artifact_hash: running.request.module_artifact_hash.clone(),
            declaration_name: running.request.declaration_name.clone(),
            classification,
            exit_code,
            stdout_artifact_hash,
            stderr_artifact_hash,
            duration_milliseconds: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            observed_toolchain_version,
            forbidden_source_token: forbidden,
            trust_profile: environment.manifest.trust_profile,
            memory_limit_enforced: false,
            network_isolation_enforced: false,
            authoritative: false,
        };
        report.validate()?;
        let report_value = serde_json::to_value(&report).map_err(|error| {
            AppError::new(
                "MCL_VERIFIER_RESULT_INVALID",
                error.to_string(),
                false,
                "Report this deterministic verifier result serialization defect.",
            )
        })?;
        let report_bytes = canonical_json(&report_value)?;
        let report_metadata = ArtifactMetadata {
            schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: crate::domain::ArtifactMediaType::Json,
            creation_source: crate::domain::ArtifactCreationSource::Verifier,
            license_expression: None,
            restriction: crate::domain::ArtifactRestriction::Private,
            semantic_metadata: BTreeMap::from([
                ("job_id".to_owned(), running.job_id.clone()),
                ("artifact_role".to_owned(), "verifier_report".to_owned()),
            ]),
        };
        let report_snapshot = self.ensure_verifier_artifact(
            &report_bytes,
            &report_metadata,
            worker,
            &format!(
                "verifier-report-{}-{}",
                running.job_id, running.attempt_count
            ),
        )?;
        let operational_success = !matches!(
            classification,
            VerifierExecutionClassification::TimedOut
                | VerifierExecutionClassification::OutputLimitExceeded
                | VerifierExecutionClassification::ToolchainMismatch
                | VerifierExecutionClassification::LaunchFailed
        );
        let last_error = process_error.as_ref().map(|error| {
            json!({
                "code": error.code,
                "message": error.message,
                "retryable": error.retryable,
                "corrective_action": error.corrective_action,
            })
        });
        let job = self.store.finish_verifier_job(
            &running.job_id,
            worker,
            &report_snapshot.artifact_hash,
            operational_success,
            last_error.as_ref(),
        )?;
        Ok(Some(VerifierWorkOutcome { job, report }))
    }

    pub fn work_one_audit_job(
        &mut self,
        worker: &str,
        lease_seconds: u64,
    ) -> Result<Option<AuditWorkOutcome>, AppError> {
        let Some(leased) = self.store.lease_next_audit_job(worker, lease_seconds)? else {
            return Ok(None);
        };
        let running = self.store.mark_audit_job_running(&leased.job_id, worker)?;
        let environment = self
            .store
            .get_environment(&running.request.environment_hash)?;
        if lease_seconds < environment.manifest.resource_limits.timeout_seconds + 60 {
            return Err(AppError::new(
                "MCL_AUDIT_LEASE_TOO_SHORT",
                "audit worker lease does not cover the environment timeout plus cleanup margin",
                true,
                "Use a lease at least 60 seconds longer than the registered verifier timeout.",
            ));
        }
        let module = self
            .store
            .get_artifact(&running.request.module_artifact_hash)?;
        if module.media_type != crate::domain::ArtifactMediaType::LeanSource {
            return Err(AppError::new(
                "MCL_AUDIT_ARTIFACT_INVALID",
                "leased audit job no longer resolves to Lean source metadata",
                false,
                "Quarantine the database and restore a verified backup.",
            ));
        }
        let policy = crate::domain::audit::committed_audit_policy()?;
        if policy.policy_hash()? != running.request.policy_hash {
            return Err(AppError::new(
                "MCL_AUDIT_POLICY_MISMATCH",
                "leased audit job does not use the committed policy identity",
                false,
                "Quarantine the audit job and enqueue it under the committed policy.",
            ));
        }
        let source = self.artifacts.read(&module.artifact_hash)?;
        let source_forbidden_token = crate::verifier::scan_forbidden_source_token(&source)?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut observed_toolchain_version = None;
        let mut observed_axioms = Vec::new();
        let mut unexpected_axioms = Vec::new();
        let mut dependency_closure_complete = false;
        let mut process_error = None;
        let classification = if source_forbidden_token.is_some() {
            LeanAuditClassification::Rejected
        } else {
            let workspace = tempfile::Builder::new()
                .prefix("mcl-audit-")
                .tempdir_in(&self.workspace_root)
                .map_err(|error| AppError::io("create audit temporary workspace", error))?;
            self.artifacts.materialize(
                &module.artifact_hash,
                workspace.path(),
                "Submission.lean",
            )?;
            let mut driver = source.clone();
            driver.extend_from_slice(
                format!("\n#print axioms {}\n", running.request.declaration_name).as_bytes(),
            );
            let driver_path = workspace.path().join("Audit.lean");
            let mut driver_file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&driver_path)
                .map_err(|error| AppError::io("create audit driver", error))?;
            use std::io::Write as _;
            driver_file
                .write_all(&driver)
                .map_err(|error| AppError::io("write audit driver", error))?;
            driver_file
                .sync_all()
                .map_err(|error| AppError::io("sync audit driver", error))?;
            drop(driver_file);
            match crate::verifier::execute_lean(
                &self.verifier_command,
                workspace.path(),
                "Audit.lean",
                &environment.manifest,
            ) {
                Ok(result) => {
                    stdout = result.stdout;
                    stderr = result.stderr;
                    observed_toolchain_version = Some(result.observed_toolchain_version);
                    if result.timed_out {
                        process_error = Some(AppError::new(
                            "MCL_AUDIT_TIMED_OUT",
                            "Lean axiom audit exceeded its wall-clock bound",
                            true,
                            "Inspect the exact audit inputs before retrying with a reviewed policy change.",
                        ));
                        LeanAuditClassification::Failed
                    } else if result.output_limit_exceeded {
                        process_error = Some(AppError::new(
                            "MCL_AUDIT_OUTPUT_LIMIT",
                            "Lean axiom audit exceeded its retained-output bound",
                            false,
                            "Inspect the declaration and dependency output before changing the bound.",
                        ));
                        LeanAuditClassification::Failed
                    } else if result.exit_code != Some(0) {
                        process_error = Some(AppError::new(
                            "MCL_AUDIT_LEAN_REJECTED",
                            "Lean rejected the verifier-controlled axiom audit driver",
                            false,
                            "Inspect the retained audit diagnostics and exact declaration.",
                        ));
                        LeanAuditClassification::Failed
                    } else {
                        match crate::verifier::parse_axiom_dependencies(
                            &running.request.declaration_name,
                            &stdout,
                            &stderr,
                        ) {
                            Ok(axioms) => {
                                observed_axioms = axioms;
                                unexpected_axioms = observed_axioms
                                    .iter()
                                    .filter(|axiom| {
                                        policy.allowed_axioms.binary_search(axiom).is_err()
                                    })
                                    .cloned()
                                    .collect();
                                dependency_closure_complete = true;
                                if unexpected_axioms.is_empty() {
                                    LeanAuditClassification::Passed
                                } else {
                                    LeanAuditClassification::Rejected
                                }
                            }
                            Err(error) => {
                                process_error = Some(error);
                                LeanAuditClassification::Failed
                            }
                        }
                    }
                }
                Err(error) => {
                    process_error = Some(error);
                    LeanAuditClassification::Failed
                }
            }
        };
        let stdout_artifact_hash =
            self.register_audit_output(&running, "audit_stdout", &stdout, worker)?;
        let stderr_artifact_hash =
            self.register_audit_output(&running, "audit_stderr", &stderr, worker)?;
        let report = LeanAuditReport {
            schema_version: crate::domain::audit::AUDIT_REPORT_SCHEMA_VERSION.to_owned(),
            job_id: running.job_id.clone(),
            request_hash: running.canonical_input_hash.clone(),
            subject: running.request.subject.clone(),
            diagnostic_evidence_hash: running.request.diagnostic_evidence_hash.clone(),
            environment_hash: running.request.environment_hash.clone(),
            module_artifact_hash: running.request.module_artifact_hash.clone(),
            declaration_name: running.request.declaration_name.clone(),
            policy_hash: running.request.policy_hash.clone(),
            classification,
            source_forbidden_token,
            observed_axioms,
            unexpected_axioms,
            stdout_artifact_hash,
            stderr_artifact_hash,
            observed_toolchain_version,
            trust_profile: environment.manifest.trust_profile,
            dependency_closure_complete,
            memory_limit_enforced: false,
            network_isolation_enforced: false,
            authoritative: false,
        };
        report.validate_against_policy(&policy)?;
        let report_bytes = canonical_json(&serde_json::to_value(&report).map_err(|error| {
            AppError::new(
                "MCL_AUDIT_RESULT_INVALID",
                error.to_string(),
                false,
                "Report this deterministic audit result serialization defect.",
            )
        })?)?;
        let report_metadata = ArtifactMetadata {
            schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: crate::domain::ArtifactMediaType::Json,
            creation_source: crate::domain::ArtifactCreationSource::Verifier,
            license_expression: None,
            restriction: crate::domain::ArtifactRestriction::Private,
            semantic_metadata: BTreeMap::from([
                ("job_id".to_owned(), running.job_id.clone()),
                ("artifact_role".to_owned(), "audit_report".to_owned()),
            ]),
        };
        let report_snapshot = self.ensure_verifier_artifact(
            &report_bytes,
            &report_metadata,
            worker,
            &format!("audit-report-{}-{}", running.job_id, running.attempt_count),
        )?;
        let operational_success = classification != LeanAuditClassification::Failed;
        let last_error = process_error.as_ref().map(|error| {
            json!({
                "code": error.code,
                "message": error.message,
                "retryable": error.retryable,
                "corrective_action": error.corrective_action,
            })
        });
        let job = self.store.finish_audit_job(
            &running.job_id,
            worker,
            &report_snapshot.artifact_hash,
            operational_success,
            last_error.as_ref(),
        )?;
        Ok(Some(AuditWorkOutcome { job, report }))
    }

    fn register_audit_output(
        &mut self,
        job: &LeanAuditJobSnapshot,
        role: &str,
        bytes: &[u8],
        worker: &str,
    ) -> Result<Option<String>, AppError> {
        if bytes.is_empty() {
            return Ok(None);
        }
        let metadata = ArtifactMetadata {
            schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: crate::domain::ArtifactMediaType::OctetStream,
            creation_source: crate::domain::ArtifactCreationSource::Verifier,
            license_expression: None,
            restriction: crate::domain::ArtifactRestriction::Private,
            semantic_metadata: BTreeMap::from([
                ("job_id".to_owned(), job.job_id.clone()),
                ("artifact_role".to_owned(), role.to_owned()),
            ]),
        };
        Ok(Some(
            self.ensure_verifier_artifact(
                bytes,
                &metadata,
                worker,
                &format!("{role}-{}-{}", job.job_id, job.attempt_count),
            )?
            .artifact_hash,
        ))
    }

    fn register_verifier_output(
        &mut self,
        job: &VerifierJobSnapshot,
        stream: &str,
        bytes: &[u8],
        worker: &str,
    ) -> Result<Option<String>, AppError> {
        if bytes.is_empty() {
            return Ok(None);
        }
        let metadata = ArtifactMetadata {
            schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: crate::domain::ArtifactMediaType::OctetStream,
            creation_source: crate::domain::ArtifactCreationSource::Verifier,
            license_expression: None,
            restriction: crate::domain::ArtifactRestriction::Private,
            semantic_metadata: BTreeMap::from([
                ("job_id".to_owned(), job.job_id.clone()),
                ("artifact_role".to_owned(), stream.to_owned()),
            ]),
        };
        Ok(Some(
            self.ensure_verifier_artifact(
                bytes,
                &metadata,
                worker,
                &format!("verifier-{stream}-{}-{}", job.job_id, job.attempt_count),
            )?
            .artifact_hash,
        ))
    }

    fn ensure_verifier_artifact(
        &mut self,
        bytes: &[u8],
        metadata: &ArtifactMetadata,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<ArtifactSnapshot, AppError> {
        metadata.validate_bytes(bytes)?;
        let hash = self.artifacts.put(bytes)?;
        match self.store.get_artifact(&hash) {
            Ok(existing) if artifact_matches_metadata(&existing, metadata, bytes.len()) => {
                Ok(existing)
            }
            Ok(_) => Err(AppError::new(
                "MCL_ARTIFACT_METADATA_CONFLICT",
                format!("existing artifact {hash} has incompatible metadata"),
                false,
                "Quarantine the conflicting artifact and inspect its provenance.",
            )),
            Err(error) if error.code == "MCL_ARTIFACT_NOT_FOUND" => self.store.register_artifact(
                &hash,
                bytes.len() as u64,
                metadata,
                actor,
                idempotency_key,
            ),
            Err(error) => Err(error),
        }
    }

    fn verify_evidence_artifacts_and_find_report(
        &self,
        evidence: &EvidenceSnapshot,
        job_id: &str,
        artifact_role: &str,
    ) -> Result<String, AppError> {
        let mut reports = Vec::new();
        for hash in &evidence.payload.artifact_hashes {
            let verified = self.verify_artifact(hash)?;
            let artifact = verified.artifact;
            if artifact.media_type == crate::domain::ArtifactMediaType::Json
                && artifact.creation_source == crate::domain::ArtifactCreationSource::Verifier
                && artifact.restriction == crate::domain::ArtifactRestriction::Private
                && artifact
                    .semantic_metadata
                    .get("job_id")
                    .is_some_and(|value| value == job_id)
                && artifact
                    .semantic_metadata
                    .get("artifact_role")
                    .is_some_and(|value| value == artifact_role)
            {
                reports.push(hash.clone());
            }
        }
        let [report] = reports.as_slice() else {
            return Err(publication_preparation_error(
                "MCL_PUBLICATION_EVIDENCE_INVALID",
                format!(
                    "evidence {} does not retain exactly one controlled {artifact_role}",
                    evidence.evidence_id
                ),
                "Quarantine the evidence and reproduce it from one controlled terminal job.",
            ));
        };
        Ok(report.clone())
    }

    fn ensure_review_artifact(
        &mut self,
        bytes: &[u8],
        metadata: &ArtifactMetadata,
        actor: &str,
        idempotency_key: &str,
    ) -> Result<ArtifactSnapshot, AppError> {
        if metadata.creation_source != crate::domain::ArtifactCreationSource::HumanReview
            || metadata.restriction != crate::domain::ArtifactRestriction::Private
        {
            return Err(AppError::new(
                "MCL_FIDELITY_REPORT_INVALID",
                "review artifact must use controlled private human-review provenance",
                false,
                "Create fidelity reports only through the review application path.",
            ));
        }
        metadata.validate_bytes(bytes)?;
        let hash = self.artifacts.put(bytes)?;
        match self.store.get_artifact(&hash) {
            Ok(existing) if artifact_matches_metadata(&existing, metadata, bytes.len()) => {
                Ok(existing)
            }
            Ok(_) => Err(AppError::new(
                "MCL_ARTIFACT_METADATA_CONFLICT",
                format!("existing artifact {hash} has incompatible metadata"),
                false,
                "Quarantine the conflicting review artifact and inspect its provenance.",
            )),
            Err(error) if error.code == "MCL_ARTIFACT_NOT_FOUND" => self.store.register_artifact(
                &hash,
                bytes.len() as u64,
                metadata,
                actor,
                idempotency_key,
            ),
            Err(error) => Err(error),
        }
    }

    pub fn create_record(
        &mut self,
        draft: &RecordDraft,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<RecordMutationOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let (proposed_version_hash, record) = if dry_run {
            (self.store.validate_record_create(draft)?, None)
        } else {
            let record = self.store.create_record(draft, actor, idempotency_key)?;
            (record.version_hash.clone(), Some(record))
        };
        Ok(RecordMutationOutcome {
            dry_run,
            proposed_version_hash,
            record,
        })
    }

    pub fn version_record(
        &mut self,
        object_id: &str,
        expected_head: &str,
        draft: &RecordDraft,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<RecordMutationOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let (proposed_version_hash, record) = if dry_run {
            (
                self.store
                    .validate_record_version(object_id, expected_head, draft)?,
                None,
            )
        } else {
            let record = self.store.version_record(
                object_id,
                expected_head,
                draft,
                actor,
                idempotency_key,
            )?;
            (record.version_hash.clone(), Some(record))
        };
        Ok(RecordMutationOutcome {
            dry_run,
            proposed_version_hash,
            record,
        })
    }

    pub fn get_record(
        &self,
        object_id: &str,
        version_hash: Option<&str>,
    ) -> Result<RecordSnapshot, AppError> {
        match version_hash {
            Some(hash) => {
                let record = self.store.get_record_version(hash)?;
                if record.object_id != object_id {
                    return Err(AppError::new(
                        "MCL_RECORD_VERSION_MISMATCH",
                        format!("version {hash} does not belong to object {object_id}"),
                        false,
                        "Use an exact object and version pair returned by canonical lookup.",
                    ));
                }
                Ok(record)
            }
            None => self.store.get_record(object_id),
        }
    }

    pub fn search_records(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<RecordSnapshot>, AppError> {
        self.store.search_records(query, limit)
    }

    pub fn create_edge(
        &mut self,
        draft: &EdgeDraft,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<EdgeMutationOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let edge = if dry_run {
            self.store.validate_edge_create(draft)?;
            None
        } else {
            Some(self.store.create_edge(draft, actor, idempotency_key)?)
        };
        Ok(EdgeMutationOutcome { dry_run, edge })
    }

    pub fn get_edge(&self, edge_id: &str) -> Result<EdgeSnapshot, AppError> {
        self.store.get_edge(edge_id)
    }

    pub fn traverse_graph(
        &self,
        request: &GraphTraversalRequest,
    ) -> Result<Vec<GraphTraversalHit>, AppError> {
        self.store.traverse_graph(request)
    }

    pub fn create_run(
        &mut self,
        kind: RunKind,
        budget: &Value,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<RunMutationOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let run = if dry_run {
            self.store.validate_run_create(budget)?;
            None
        } else {
            Some(
                self.store
                    .create_run(kind, budget, actor, idempotency_key)?,
            )
        };
        Ok(RunMutationOutcome { dry_run, run })
    }

    pub fn get_run(&self, run_id: &str) -> Result<RunSnapshot, AppError> {
        self.store.get_run(run_id)
    }

    pub fn list_run_events(&self, run_id: &str) -> Result<Vec<RunEventSnapshot>, AppError> {
        self.store.list_run_events(run_id)
    }

    pub fn append_run_event(
        &mut self,
        run_id: &str,
        expected_head_hash: &str,
        draft: &RunEventDraft,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<RunEventMutationOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let event = if dry_run {
            self.store
                .validate_run_event_append(run_id, expected_head_hash, draft)?;
            None
        } else {
            Some(self.store.append_run_event(
                run_id,
                expected_head_hash,
                draft,
                actor,
                idempotency_key,
            )?)
        };
        Ok(RunEventMutationOutcome { dry_run, event })
    }

    pub fn verify_run_chain(&self, run_id: &str) -> Result<RunChainReport, AppError> {
        self.store.verify_run_chain(run_id)
    }
}

#[derive(Debug)]
struct ValidatedPublicationCandidateDocuments {
    report: PublicationReport,
    retained_closure: PublicationRetainedClosure,
    request_hash: String,
    report_content_hash: String,
    retained_closure_hash: String,
}

fn validate_publication_candidate_documents(
    report_bytes: &[u8],
    retained_closure_bytes: &[u8],
) -> Result<ValidatedPublicationCandidateDocuments, AppError> {
    let report: PublicationReport = decode_exact_canonical_publication_json(
        report_bytes,
        "publication report",
        "MCL_PUBLICATION_REPORT_JSON_INVALID",
        "MCL_PUBLICATION_REPORT_NONCANONICAL",
    )?;
    let retained_closure: PublicationRetainedClosure = decode_exact_canonical_publication_json(
        retained_closure_bytes,
        "publication retained closure",
        "MCL_PUBLICATION_RETAINED_CLOSURE_JSON_INVALID",
        "MCL_PUBLICATION_RETAINED_CLOSURE_NONCANONICAL",
    )?;
    let policy = crate::domain::publication::committed_publication_policy()?;
    report.validate_candidate(&policy)?;
    retained_closure.validate(&report.request)?;

    let request_hash = report.request.request_hash()?;
    let report_content_hash = report.report_hash(&policy)?;
    let retained_closure_hash = retained_closure.closure_hash(&report.request)?;
    let exact_report_hash = format!("{:x}", Sha256::digest(report_bytes));
    let exact_retained_closure_hash = format!("{:x}", Sha256::digest(retained_closure_bytes));
    if report_content_hash != exact_report_hash
        || retained_closure_hash != exact_retained_closure_hash
    {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_CANDIDATE_IDENTITY_MISMATCH",
            "canonical publication document identities disagree with their exact file bytes",
            "Regenerate both documents with the committed canonical JSON encoder.",
        ));
    }
    let expected_retained_artifact_hashes =
        retained_closure.report_retained_artifact_hashes(&report.request)?;
    if report.retained_artifact_hashes != expected_retained_artifact_hashes {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_ARTIFACTS_MISMATCH",
            "publication report retained artifact hashes do not equal the exact closure members plus its manifest artifact hash",
            "Rebuild the report from the exact canonical retained closure without adding, omitting, or substituting hashes.",
        ));
    }

    Ok(ValidatedPublicationCandidateDocuments {
        report,
        retained_closure,
        request_hash,
        report_content_hash,
        retained_closure_hash,
    })
}

fn decode_exact_canonical_publication_json<T>(
    bytes: &[u8],
    label: &str,
    invalid_code: &'static str,
    noncanonical_code: &'static str,
) -> Result<T, AppError>
where
    T: DeserializeOwned + Serialize,
{
    let document: T = serde_json::from_slice(bytes).map_err(|error| {
        publication_candidate_error(
            invalid_code,
            format!("{label} JSON is invalid: {error}"),
            "Supply one complete document matching the committed closed publication schema.",
        )
    })?;
    let value = serde_json::to_value(&document).map_err(|error| {
        publication_candidate_error(
            invalid_code,
            format!("{label} cannot be serialized canonically: {error}"),
            "Regenerate the document with the committed publication implementation.",
        )
    })?;
    let canonical = canonical_json(&value).map_err(|error| {
        publication_candidate_error(
            invalid_code,
            format!("{label} is not valid canonical I-JSON: {}", error.message),
            "Regenerate the document with only canonical I-JSON values.",
        )
    })?;
    if bytes != canonical {
        return Err(publication_candidate_error(
            noncanonical_code,
            format!("{label} file bytes are not the exact canonical JSON encoding"),
            "Use the exact canonical JSON bytes without whitespace, a byte-order mark, or a trailing newline.",
        ));
    }
    Ok(document)
}

#[derive(Debug)]
struct RetainedPublicationFiles {
    bytes_by_role: BTreeMap<PublicationRetainedArtifactRole, Vec<u8>>,
}

impl RetainedPublicationFiles {
    fn get(&self, role: PublicationRetainedArtifactRole) -> Result<&[u8], AppError> {
        self.bytes_by_role
            .get(&role)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                publication_candidate_error(
                    "MCL_PUBLICATION_RETAINED_MEMBER_MISSING",
                    format!("retained closure has no loaded {} member", role.as_str()),
                    "Restore the complete protected-workflow retained closure.",
                )
            })
    }
}

fn read_retained_publication_files(
    retained_root: &Path,
    closure: &PublicationRetainedClosure,
) -> Result<RetainedPublicationFiles, AppError> {
    let root_metadata = fs::symlink_metadata(retained_root)
        .map_err(|error| AppError::io("inspect publication retained root", error))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_ROOT_UNSAFE",
            "publication retained root is not a real directory",
            "Use the protected-workflow output directory without symbolic links.",
        ));
    }
    let retained_root = retained_root
        .canonicalize()
        .map_err(|error| AppError::io("canonicalize publication retained root", error))?;
    let mut bytes_by_role = BTreeMap::new();
    for entry in &closure.artifacts {
        let relative = Path::new(&entry.path);
        let mut candidate = retained_root.clone();
        for component in relative.components() {
            let std::path::Component::Normal(component) = component else {
                return Err(publication_candidate_error(
                    "MCL_PUBLICATION_RETAINED_PATH_UNSAFE",
                    format!(
                        "retained {} path is not strictly relative",
                        entry.role.as_str()
                    ),
                    "Use the exact fixed retained path declared for that role.",
                ));
            };
            candidate.push(component);
            let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
                publication_candidate_error(
                    "MCL_PUBLICATION_RETAINED_MEMBER_MISSING",
                    format!(
                        "cannot inspect retained {} member {}: {error}",
                        entry.role.as_str(),
                        candidate.display()
                    ),
                    "Restore the complete protected-workflow retained closure.",
                )
            })?;
            if metadata.file_type().is_symlink() {
                return Err(publication_candidate_error(
                    "MCL_PUBLICATION_RETAINED_PATH_UNSAFE",
                    format!(
                        "retained {} path contains a symbolic link",
                        entry.role.as_str()
                    ),
                    "Materialize every retained member as a real contained file.",
                ));
            }
        }
        let candidate = candidate.canonicalize().map_err(|error| {
            publication_candidate_error(
                "MCL_PUBLICATION_RETAINED_MEMBER_MISSING",
                format!(
                    "cannot resolve retained {} member: {error}",
                    entry.role.as_str()
                ),
                "Restore the complete protected-workflow retained closure.",
            )
        })?;
        if !candidate.starts_with(&retained_root) || !candidate.is_file() {
            return Err(publication_candidate_error(
                "MCL_PUBLICATION_RETAINED_PATH_UNSAFE",
                format!(
                    "retained {} member is not a regular file contained by the retained root",
                    entry.role.as_str()
                ),
                "Materialize every retained member as a real contained file.",
            ));
        }
        let limit = retained_role_byte_limit(entry.role);
        let file = fs::File::open(&candidate)
            .map_err(|error| AppError::io("open retained publication member", error))?;
        if !file
            .metadata()
            .map_err(|error| AppError::io("inspect retained publication member", error))?
            .is_file()
        {
            return Err(publication_candidate_error(
                "MCL_PUBLICATION_RETAINED_PATH_UNSAFE",
                format!(
                    "retained {} member is not a regular file",
                    entry.role.as_str()
                ),
                "Materialize every retained member as a real contained file.",
            ));
        }
        let mut bytes = Vec::new();
        file.take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| AppError::io("read retained publication member", error))?;
        if bytes.len() > limit {
            return Err(publication_candidate_error(
                "MCL_PUBLICATION_RETAINED_MEMBER_TOO_LARGE",
                format!(
                    "retained {} member exceeds its {} byte limit",
                    entry.role.as_str(),
                    limit
                ),
                "Reject the candidate and inspect the protected workflow output bound.",
            ));
        }
        let observed_hash = format!("{:x}", Sha256::digest(&bytes));
        if observed_hash != entry.artifact_hash {
            return Err(publication_candidate_error(
                "MCL_PUBLICATION_RETAINED_MEMBER_HASH_MISMATCH",
                format!(
                    "retained {} member bytes hash to {observed_hash}, expected {}",
                    entry.role.as_str(),
                    entry.artifact_hash
                ),
                "Reject the altered closure and retain the exact bytes named by the manifest.",
            ));
        }
        bytes_by_role.insert(entry.role, bytes);
    }
    if bytes_by_role.len() != PublicationRetainedArtifactRole::ALL.len() {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_MEMBER_MISSING",
            "retained closure did not load exactly one file for every required role",
            "Restore the complete protected-workflow retained closure.",
        ));
    }
    Ok(RetainedPublicationFiles { bytes_by_role })
}

const fn retained_role_byte_limit(role: PublicationRetainedArtifactRole) -> usize {
    match role {
        PublicationRetainedArtifactRole::LeanModule => MAX_RETAINED_LEAN_BYTES,
        PublicationRetainedArtifactRole::AuditStderr
        | PublicationRetainedArtifactRole::AuditStdout
        | PublicationRetainedArtifactRole::ProtectedAuditStderr
        | PublicationRetainedArtifactRole::ProtectedAuditStdout
        | PublicationRetainedArtifactRole::ProtectedDependencyStderr
        | PublicationRetainedArtifactRole::ProtectedDependencyStdout
        | PublicationRetainedArtifactRole::ProtectedStderr
        | PublicationRetainedArtifactRole::ProtectedStdout
        | PublicationRetainedArtifactRole::VerifierStderr
        | PublicationRetainedArtifactRole::VerifierStdout => MAX_RETAINED_LOG_BYTES,
        _ => MAX_RETAINED_JSON_BYTES,
    }
}

fn validate_retained_publication_semantics(
    report: &PublicationReport,
    closure: &PublicationRetainedClosure,
    files: &RetainedPublicationFiles,
) -> Result<(), AppError> {
    let request = &report.request;
    let retained_request: PublicationRequest =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::PublicationRequest)?;
    if retained_request != *request || retained_request.request_hash()? != report.request_hash {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::PublicationRequest,
            "retained request is not the exact request embedded in the candidate report",
        ));
    }

    let committed_policy = crate::domain::publication::committed_publication_policy()?;
    let retained_policy: crate::domain::PublicationPolicy =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::PublicationPolicy)?;
    retained_policy.validate()?;
    let publication_policy_hash = retained_policy.policy_hash()?;
    if retained_policy != committed_policy || publication_policy_hash != request.policy_hash {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::PublicationPolicy,
            "retained publication policy is not the exact committed request policy",
        ));
    }
    require_retained_identity(
        closure,
        PublicationRetainedArtifactRole::PublicationPolicy,
        &publication_policy_hash,
    )?;

    let committed_audit_policy = crate::domain::audit::committed_audit_policy()?;
    let retained_audit_policy: crate::domain::LeanAuditPolicy =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::AuditPolicy)?;
    retained_audit_policy.validate()?;
    let audit_policy_hash = retained_audit_policy.policy_hash()?;
    if retained_audit_policy != committed_audit_policy {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::AuditPolicy,
            "retained audit policy is not the exact committed audit policy",
        ));
    }
    require_retained_identity(
        closure,
        PublicationRetainedArtifactRole::AuditPolicy,
        &audit_policy_hash,
    )?;

    let environment: EnvironmentManifest =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::EnvironmentManifest)?;
    let environment_hash = environment.environment_hash()?;
    if environment_hash != request.environment_hash {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::EnvironmentManifest,
            "retained environment does not reproduce the publication request environment hash",
        ));
    }
    require_retained_identity(
        closure,
        PublicationRetainedArtifactRole::EnvironmentManifest,
        &environment_hash,
    )?;

    let module = files.get(PublicationRetainedArtifactRole::LeanModule)?;
    if crate::verifier::scan_forbidden_source_token(module)?.is_some() {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::LeanModule,
            "retained Lean module contains a forbidden source token",
        ));
    }
    require_retained_identity(
        closure,
        PublicationRetainedArtifactRole::LeanModule,
        &request.module_artifact_hash,
    )?;

    let formalization = validate_retained_record(
        files,
        closure,
        PublicationRetainedArtifactRole::FormalizationVersion,
        RecordKind::Formalization,
    )?;
    let claim = validate_retained_record(
        files,
        closure,
        PublicationRetainedArtifactRole::ClaimVersion,
        RecordKind::Claim,
    )?;
    let source = validate_retained_record(
        files,
        closure,
        PublicationRetainedArtifactRole::SourceVersion,
        RecordKind::Source,
    )?;
    if formalization.object_id != request.subject.object_id
        || formalization.version_hash != request.subject.version_hash
    {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::FormalizationVersion,
            "retained formalization is not the exact publication request subject",
        ));
    }
    let formalization_payload: FormalizationPayload =
        serde_json::from_value(formalization.payload.clone()).map_err(|error| {
            retained_semantic_error(
                PublicationRetainedArtifactRole::FormalizationVersion,
                format!("retained formalization payload is invalid: {error}"),
            )
        })?;
    let claim_payload: ClaimPayload =
        serde_json::from_value(claim.payload.clone()).map_err(|error| {
            retained_semantic_error(
                PublicationRetainedArtifactRole::ClaimVersion,
                format!("retained claim payload is invalid: {error}"),
            )
        })?;
    if formalization_payload.claim_version.object_id != claim.object_id
        || formalization_payload.claim_version.version_hash != claim.version_hash
        || claim_payload.source_reference.object_id != source.object_id
        || claim_payload.source_reference.version_hash != source.version_hash
        || formalization_payload.environment_hash != request.environment_hash
        || formalization_payload.module_artifact_hash != request.module_artifact_hash
        || formalization_payload.declaration_name != request.declaration_name
        || !environment.dependencies.is_empty()
        || !environment.import_manifest.is_empty()
        || formalization_payload.import_manifest != environment.import_manifest
    {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_RECORD_LINK_INVALID",
            "retained formalization, claim, source, environment, module, and declaration do not form one exact chain",
            "Regenerate the retained closure from the exact current canonical publication subject.",
        ));
    }

    let diagnostic: EvidenceSnapshot =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::DiagnosticEvidence)?;
    let proof_closure: EvidenceSnapshot =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::ProofClosureEvidence)?;
    let axiom_audit: EvidenceSnapshot =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::AxiomAuditEvidence)?;
    let diagnostic_job_id = validate_retained_evidence(
        &diagnostic,
        closure,
        PublicationRetainedArtifactRole::DiagnosticEvidence,
        &request.diagnostic_evidence_id,
        &request.diagnostic_evidence_hash,
        EvidenceKind::LeanElaboration,
        request,
        &formalization_payload,
    )?;
    let proof_closure_job_id = validate_retained_evidence(
        &proof_closure,
        closure,
        PublicationRetainedArtifactRole::ProofClosureEvidence,
        &request.proof_closure_evidence_id,
        &request.proof_closure_evidence_hash,
        EvidenceKind::ProofClosureScan,
        request,
        &formalization_payload,
    )?;
    let axiom_audit_job_id = validate_retained_evidence(
        &axiom_audit,
        closure,
        PublicationRetainedArtifactRole::AxiomAuditEvidence,
        &request.axiom_audit_evidence_id,
        &request.axiom_audit_evidence_hash,
        EvidenceKind::AxiomAudit,
        request,
        &formalization_payload,
    )?;
    if proof_closure_job_id != axiom_audit_job_id
        || proof_closure.payload.artifact_hashes != axiom_audit.payload.artifact_hashes
        || proof_closure.payload.verifier_or_reviewer_identity
            != axiom_audit.payload.verifier_or_reviewer_identity
    {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_EVIDENCE_MISMATCH",
            "retained proof-closure and axiom-audit evidence do not form one exact audit pair",
            "Retain both accepted evidence snapshots from the same terminal audit job.",
        ));
    }

    let verifier_job: VerifierJobSnapshot =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::VerifierJob)?;
    let verifier_report: VerifierExecutionReport =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::VerifierReport)?;
    validate_retained_verifier_chain(
        &verifier_job,
        &verifier_report,
        &diagnostic,
        &diagnostic_job_id,
        request,
        &environment,
        closure,
        files,
    )?;

    let audit_job: LeanAuditJobSnapshot =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::AuditJob)?;
    let audit_report: LeanAuditReport =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::AuditReport)?;
    validate_retained_audit_chain(
        &audit_job,
        &audit_report,
        &proof_closure,
        &axiom_audit,
        &proof_closure_job_id,
        request,
        &environment,
        &retained_audit_policy,
        closure,
        files,
    )?;

    for role in [
        PublicationRetainedArtifactRole::AuditStderr,
        PublicationRetainedArtifactRole::AuditStdout,
        PublicationRetainedArtifactRole::ProtectedAuditStderr,
        PublicationRetainedArtifactRole::ProtectedAuditStdout,
        PublicationRetainedArtifactRole::ProtectedDependencyStderr,
        PublicationRetainedArtifactRole::ProtectedDependencyStdout,
        PublicationRetainedArtifactRole::ProtectedStderr,
        PublicationRetainedArtifactRole::ProtectedStdout,
        PublicationRetainedArtifactRole::VerifierStderr,
        PublicationRetainedArtifactRole::VerifierStdout,
    ] {
        if files.get(role)?.len() as u64 > environment.resource_limits.max_output_bytes {
            return Err(retained_semantic_error(
                role,
                "retained output exceeds the request environment output bound",
            ));
        }
    }
    for role in [
        PublicationRetainedArtifactRole::ProtectedAuditStderr,
        PublicationRetainedArtifactRole::ProtectedAuditStdout,
        PublicationRetainedArtifactRole::ProtectedDependencyStderr,
        PublicationRetainedArtifactRole::ProtectedDependencyStdout,
        PublicationRetainedArtifactRole::ProtectedStderr,
        PublicationRetainedArtifactRole::ProtectedStdout,
    ] {
        let artifact_hash = retained_entry(closure, role)?.artifact_hash.clone();
        require_retained_identity(closure, role, &artifact_hash)?;
    }
    validate_protected_dependency_output(
        files.get(PublicationRetainedArtifactRole::ProtectedDependencyStdout)?,
        files.get(PublicationRetainedArtifactRole::ProtectedDependencyStderr)?,
    )?;
    let protected_axioms = crate::verifier::parse_axiom_dependencies(
        &request.declaration_name,
        files.get(PublicationRetainedArtifactRole::ProtectedAuditStdout)?,
        files.get(PublicationRetainedArtifactRole::ProtectedAuditStderr)?,
    )?;
    if protected_axioms != report.observed_axioms {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_PROTECTED_AUDIT_MISMATCH",
            "protected audit output does not reproduce the candidate report observed axioms",
            "Reject the candidate and rerun the protected audit from the exact retained module.",
        ));
    }

    Ok(())
}

fn validate_protected_dependency_output(stdout: &[u8], stderr: &[u8]) -> Result<(), AppError> {
    const PINNED_IMPLICIT_DEPENDENCY: &str = "/opt/lib/lean/Init.olean";
    if !stderr.is_empty() {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_PROTECTED_DEPENDENCY_MISMATCH",
            "protected Lean dependency discovery wrote unexpected stderr",
            "Reject the candidate and rerun pinned Lean dependency discovery in the protected sandbox.",
        ));
    }
    let output = std::str::from_utf8(stdout).map_err(|error| {
        publication_candidate_error(
            "MCL_PUBLICATION_PROTECTED_DEPENDENCY_MISMATCH",
            format!("protected Lean dependency output is not UTF-8: {error}"),
            "Reject the candidate and retain the exact pinned Lean dependency output.",
        )
    })?;
    let dependencies = output.split_terminator('\n').collect::<Vec<_>>();
    if dependencies.is_empty()
        || dependencies.len() > 8
        || dependencies
            .iter()
            .any(|dependency| *dependency != PINNED_IMPLICIT_DEPENDENCY)
    {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_PROTECTED_DEPENDENCY_MISMATCH",
            "protected Lean dependency output is not the closed implicit Init dependency",
            "Reject undeclared imports and regenerate the no-import candidate from the exact pinned toolchain.",
        ));
    }
    Ok(())
}

fn decode_closed_retained_json<T>(
    files: &RetainedPublicationFiles,
    role: PublicationRetainedArtifactRole,
) -> Result<T, AppError>
where
    T: DeserializeOwned + Serialize,
{
    let bytes = files.get(role)?;
    let source: Value = serde_json::from_slice(bytes).map_err(|error| {
        retained_semantic_error(role, format!("retained JSON is invalid: {error}"))
    })?;
    let canonical = canonical_json(&source).map_err(|error| {
        retained_semantic_error(
            role,
            format!("retained JSON is not canonical I-JSON: {}", error.message),
        )
    })?;
    if bytes != canonical {
        return Err(retained_semantic_error(
            role,
            "retained JSON bytes are not the exact canonical encoding",
        ));
    }
    let document: T = serde_json::from_value(source.clone()).map_err(|error| {
        retained_semantic_error(
            role,
            format!("retained JSON does not match its closed type: {error}"),
        )
    })?;
    let reproduced = serde_json::to_value(&document).map_err(|error| {
        retained_semantic_error(
            role,
            format!("retained typed value cannot be reproduced: {error}"),
        )
    })?;
    if source != reproduced {
        return Err(retained_semantic_error(
            role,
            "retained JSON contains unknown or non-reproducible fields",
        ));
    }
    Ok(document)
}

fn validate_retained_record(
    files: &RetainedPublicationFiles,
    closure: &PublicationRetainedClosure,
    role: PublicationRetainedArtifactRole,
    expected_kind: RecordKind,
) -> Result<RecordSnapshot, AppError> {
    let record: RecordSnapshot = decode_closed_retained_json(files, role)?;
    validate_record_payload(record.kind, &record.schema_version, &record.payload)
        .map_err(|error| retained_semantic_error(role, error.message))?;
    let reproduced = record_version_hash(&record.schema_version, &record.payload)?;
    if record.kind != expected_kind
        || uuid::Uuid::parse_str(&record.object_id).is_err()
        || reproduced != record.version_hash
        || record.created_by.trim().is_empty()
        || record
            .predecessor_hash
            .as_deref()
            .is_some_and(|hash| !is_lower_sha256(hash))
    {
        return Err(retained_semantic_error(
            role,
            "retained canonical record snapshot has an invalid kind, identity, or attribution",
        ));
    }
    require_retained_identity(closure, role, &record.version_hash)?;
    Ok(record)
}

#[allow(clippy::too_many_arguments)]
fn validate_retained_evidence(
    evidence: &EvidenceSnapshot,
    closure: &PublicationRetainedClosure,
    role: PublicationRetainedArtifactRole,
    expected_id: &str,
    expected_hash: &str,
    expected_kind: EvidenceKind,
    request: &PublicationRequest,
    formalization: &FormalizationPayload,
) -> Result<String, AppError> {
    let reproduced_hash = evidence.payload.evidence_hash()?;
    if evidence.evidence_id != expected_id
        || evidence.evidence_hash != expected_hash
        || evidence.evidence_hash != reproduced_hash
        || evidence.created_by.trim().is_empty()
    {
        return Err(retained_semantic_error(
            role,
            "retained evidence snapshot does not reproduce its request-bound identity",
        ));
    }
    require_retained_identity(closure, role, &reproduced_hash)?;
    validate_publication_evidence(evidence, &request.subject, expected_kind, formalization)
}

fn validate_retained_store_snapshots(
    store: &Store,
    request: &PublicationRequest,
    files: &RetainedPublicationFiles,
) -> Result<(), AppError> {
    for role in [
        PublicationRetainedArtifactRole::SourceVersion,
        PublicationRetainedArtifactRole::ClaimVersion,
        PublicationRetainedArtifactRole::FormalizationVersion,
    ] {
        let retained: RecordSnapshot = decode_closed_retained_json(files, role)?;
        if store.get_record_version(&retained.version_hash)? != retained {
            return Err(retained_semantic_error(
                role,
                "retained record snapshot differs from the exact registered snapshot",
            ));
        }
    }
    let retained_environment: EnvironmentManifest =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::EnvironmentManifest)?;
    if store.get_environment(&request.environment_hash)?.manifest != retained_environment {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::EnvironmentManifest,
            "retained environment differs from the exact registered manifest",
        ));
    }
    for (role, evidence_id) in [
        (
            PublicationRetainedArtifactRole::DiagnosticEvidence,
            request.diagnostic_evidence_id.as_str(),
        ),
        (
            PublicationRetainedArtifactRole::ProofClosureEvidence,
            request.proof_closure_evidence_id.as_str(),
        ),
        (
            PublicationRetainedArtifactRole::AxiomAuditEvidence,
            request.axiom_audit_evidence_id.as_str(),
        ),
    ] {
        let retained: EvidenceSnapshot = decode_closed_retained_json(files, role)?;
        if store.get_evidence(evidence_id)? != retained {
            return Err(retained_semantic_error(
                role,
                "retained evidence snapshot differs from the exact registered snapshot",
            ));
        }
    }
    let retained_verifier_job: VerifierJobSnapshot =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::VerifierJob)?;
    if store.get_verifier_job(&retained_verifier_job.job_id)? != retained_verifier_job {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::VerifierJob,
            "retained verifier job differs from the exact registered terminal snapshot",
        ));
    }
    let retained_audit_job: LeanAuditJobSnapshot =
        decode_closed_retained_json(files, PublicationRetainedArtifactRole::AuditJob)?;
    if store.get_audit_job(&retained_audit_job.job_id)? != retained_audit_job {
        return Err(retained_semantic_error(
            PublicationRetainedArtifactRole::AuditJob,
            "retained audit job differs from the exact registered terminal snapshot",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_retained_verifier_chain(
    job: &VerifierJobSnapshot,
    report: &VerifierExecutionReport,
    evidence: &EvidenceSnapshot,
    expected_job_id: &str,
    request: &PublicationRequest,
    environment: &EnvironmentManifest,
    closure: &PublicationRetainedClosure,
    files: &RetainedPublicationFiles,
) -> Result<(), AppError> {
    job.request.validate()?;
    report.validate()?;
    let input_hash =
        crate::canonical::value_hash(&serde_json::to_value(&job.request).map_err(|error| {
            retained_semantic_error(
                PublicationRetainedArtifactRole::VerifierJob,
                error.to_string(),
            )
        })?)?;
    let report_artifact_hash =
        retained_entry(closure, PublicationRetainedArtifactRole::VerifierReport)?
            .artifact_hash
            .as_str();
    if job.job_id != expected_job_id
        || uuid::Uuid::parse_str(&job.job_id).is_err()
        || job.canonical_input_hash != input_hash
        || job.state != VerifierJobState::Succeeded
        || job.request.environment_hash != request.environment_hash
        || job.request.module_artifact_hash != request.module_artifact_hash
        || job.request.declaration_name != request.declaration_name
        || job.result_artifact_hash.as_deref() != Some(report_artifact_hash)
        || report.job_id != job.job_id
        || report.environment_hash != request.environment_hash
        || report.module_artifact_hash != request.module_artifact_hash
        || report.declaration_name != request.declaration_name
        || report.classification != VerifierExecutionClassification::Elaborated
        || report.exit_code != Some(0)
        || report.forbidden_source_token.is_some()
        || report.observed_toolchain_version.is_none()
        || report.trust_profile != environment.trust_profile
        || evidence.payload.verifier_or_reviewer_identity
            != format!(
                "lean:{}",
                report
                    .observed_toolchain_version
                    .as_deref()
                    .unwrap_or("source-policy")
            )
    {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_VERIFIER_MISMATCH",
            "retained verifier job and report do not reproduce the accepted diagnostic execution",
            "Retain the exact terminal verifier snapshot, report, logs, module, and diagnostic evidence.",
        ));
    }
    require_retained_identity(
        closure,
        PublicationRetainedArtifactRole::VerifierJob,
        &input_hash,
    )?;
    require_retained_identity(
        closure,
        PublicationRetainedArtifactRole::VerifierReport,
        report_artifact_hash,
    )?;
    validate_optional_retained_log(
        report.stdout_artifact_hash.as_deref(),
        PublicationRetainedArtifactRole::VerifierStdout,
        closure,
        files,
    )?;
    validate_optional_retained_log(
        report.stderr_artifact_hash.as_deref(),
        PublicationRetainedArtifactRole::VerifierStderr,
        closure,
        files,
    )?;
    let expected_artifacts = report_artifact_closure(
        &request.module_artifact_hash,
        report_artifact_hash,
        report.stdout_artifact_hash.as_deref(),
        report.stderr_artifact_hash.as_deref(),
    );
    if evidence.payload.artifact_hashes != expected_artifacts {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_VERIFIER_MISMATCH",
            "retained diagnostic evidence artifact closure does not match the verifier report and logs",
            "Retain the exact accepted verifier artifact closure.",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_retained_audit_chain(
    job: &LeanAuditJobSnapshot,
    report: &LeanAuditReport,
    proof_closure: &EvidenceSnapshot,
    axiom_audit: &EvidenceSnapshot,
    expected_job_id: &str,
    request: &PublicationRequest,
    environment: &EnvironmentManifest,
    policy: &crate::domain::LeanAuditPolicy,
    closure: &PublicationRetainedClosure,
    files: &RetainedPublicationFiles,
) -> Result<(), AppError> {
    job.request.validate()?;
    report.validate_against_policy(policy)?;
    let input_hash = job.request.request_hash()?;
    let report_artifact_hash =
        retained_entry(closure, PublicationRetainedArtifactRole::AuditReport)?
            .artifact_hash
            .as_str();
    if job.job_id != expected_job_id
        || uuid::Uuid::parse_str(&job.job_id).is_err()
        || job.canonical_input_hash != input_hash
        || job.state != VerifierJobState::Succeeded
        || job.request.subject != request.subject
        || job.request.diagnostic_evidence_id != request.diagnostic_evidence_id
        || job.request.diagnostic_evidence_hash != request.diagnostic_evidence_hash
        || job.request.environment_hash != request.environment_hash
        || job.request.module_artifact_hash != request.module_artifact_hash
        || job.request.declaration_name != request.declaration_name
        || job.request.policy_hash != policy.policy_hash()?
        || job.result_artifact_hash.as_deref() != Some(report_artifact_hash)
        || report.job_id != job.job_id
        || report.request_hash != job.canonical_input_hash
        || report.subject != request.subject
        || report.diagnostic_evidence_hash != request.diagnostic_evidence_hash
        || report.environment_hash != request.environment_hash
        || report.module_artifact_hash != request.module_artifact_hash
        || report.declaration_name != request.declaration_name
        || report.classification != LeanAuditClassification::Passed
        || report.observed_toolchain_version.is_none()
        || report.trust_profile != environment.trust_profile
        || proof_closure.payload.verifier_or_reviewer_identity
            != format!(
                "lean-audit:{}",
                report
                    .observed_toolchain_version
                    .as_deref()
                    .unwrap_or("source-policy")
            )
    {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_AUDIT_MISMATCH",
            "retained audit job and report do not reproduce the accepted exact audit pair",
            "Retain the exact terminal audit snapshot, report, logs, policy, and evidence pair.",
        ));
    }
    require_retained_identity(
        closure,
        PublicationRetainedArtifactRole::AuditJob,
        &input_hash,
    )?;
    require_retained_identity(
        closure,
        PublicationRetainedArtifactRole::AuditReport,
        report_artifact_hash,
    )?;
    validate_optional_retained_log(
        report.stdout_artifact_hash.as_deref(),
        PublicationRetainedArtifactRole::AuditStdout,
        closure,
        files,
    )?;
    validate_optional_retained_log(
        report.stderr_artifact_hash.as_deref(),
        PublicationRetainedArtifactRole::AuditStderr,
        closure,
        files,
    )?;
    let expected_artifacts = report_artifact_closure(
        &request.module_artifact_hash,
        report_artifact_hash,
        report.stdout_artifact_hash.as_deref(),
        report.stderr_artifact_hash.as_deref(),
    );
    if proof_closure.payload.artifact_hashes != expected_artifacts
        || axiom_audit.payload.artifact_hashes != expected_artifacts
    {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_AUDIT_MISMATCH",
            "retained audit evidence artifact closure does not match the audit report and logs",
            "Retain the exact accepted audit artifact closure.",
        ));
    }
    let parsed_axioms = crate::verifier::parse_axiom_dependencies(
        &request.declaration_name,
        files.get(PublicationRetainedArtifactRole::AuditStdout)?,
        files.get(PublicationRetainedArtifactRole::AuditStderr)?,
    )?;
    if parsed_axioms != report.observed_axioms {
        return Err(publication_candidate_error(
            "MCL_PUBLICATION_RETAINED_AUDIT_MISMATCH",
            "retained local audit output does not reproduce its report observed axioms",
            "Reject the altered audit closure and reproduce the exact terminal audit.",
        ));
    }
    Ok(())
}

fn validate_optional_retained_log(
    reported_hash: Option<&str>,
    role: PublicationRetainedArtifactRole,
    closure: &PublicationRetainedClosure,
    files: &RetainedPublicationFiles,
) -> Result<(), AppError> {
    let entry = retained_entry(closure, role)?;
    let bytes = files.get(role)?;
    let matches = match reported_hash {
        Some(hash) => hash == entry.artifact_hash,
        None => bytes.is_empty(),
    };
    if !matches {
        return Err(retained_semantic_error(
            role,
            "retained log bytes do not match the report's optional output identity",
        ));
    }
    require_retained_identity(closure, role, &entry.artifact_hash)
}

fn retained_entry(
    closure: &PublicationRetainedClosure,
    role: PublicationRetainedArtifactRole,
) -> Result<&crate::domain::PublicationRetainedClosureEntry, AppError> {
    closure
        .artifacts
        .iter()
        .find(|entry| entry.role == role)
        .ok_or_else(|| {
            retained_semantic_error(role, "retained closure entry is unexpectedly missing")
        })
}

fn require_retained_identity(
    closure: &PublicationRetainedClosure,
    role: PublicationRetainedArtifactRole,
    expected_identity: &str,
) -> Result<(), AppError> {
    if retained_entry(closure, role)?.identity_hash != expected_identity {
        return Err(retained_semantic_error(
            role,
            format!("retained identity does not equal {expected_identity}"),
        ));
    }
    Ok(())
}

fn retained_semantic_error(
    role: PublicationRetainedArtifactRole,
    message: impl Into<String>,
) -> AppError {
    publication_candidate_error(
        "MCL_PUBLICATION_RETAINED_SEMANTIC_INVALID",
        format!(
            "retained {} member is invalid: {}",
            role.as_str(),
            message.into()
        ),
        "Reject the candidate and regenerate the exact retained closure in the protected workflow.",
    )
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_publication_evidence(
    evidence: &EvidenceSnapshot,
    subject: &ExactVersionReference,
    expected_kind: EvidenceKind,
    formalization: &FormalizationPayload,
) -> Result<String, AppError> {
    if evidence.payload.subject != *subject
        || evidence.payload.evidence_kind != expected_kind
        || evidence.payload.result != EvidenceResult::Accepted
        || evidence.payload.authority_class != EvidenceAuthorityClass::Diagnostic
        || evidence.payload.producing_run_id.is_some()
        || evidence.payload.producing_job_id.is_none()
        || evidence.payload.environment_hash.as_deref()
            != Some(formalization.environment_hash.as_str())
        || !evidence
            .payload
            .artifact_hashes
            .iter()
            .any(|hash| hash == &formalization.module_artifact_hash)
        || evidence.payload.stale
        || evidence.payload.stale_reason.is_some()
    {
        return Err(publication_preparation_error(
            "MCL_PUBLICATION_EVIDENCE_INVALID",
            format!(
                "evidence {} is not current accepted {} evidence for the exact formalization",
                evidence.evidence_id,
                expected_kind.as_str()
            ),
            "Select current accepted diagnostic evidence produced by the controlled verifier and audit paths.",
        ));
    }
    evidence.payload.producing_job_id.clone().ok_or_else(|| {
        publication_preparation_error(
            "MCL_PUBLICATION_EVIDENCE_INVALID",
            "publication evidence has no producing job identity",
            "Reproduce the evidence through a controlled verifier or audit job.",
        )
    })
}

fn report_artifact_closure(
    module_artifact_hash: &str,
    report_artifact_hash: &str,
    stdout_artifact_hash: Option<&str>,
    stderr_artifact_hash: Option<&str>,
) -> Vec<String> {
    let mut artifacts = vec![
        module_artifact_hash.to_owned(),
        report_artifact_hash.to_owned(),
    ];
    artifacts.extend(stdout_artifact_hash.map(str::to_owned));
    artifacts.extend(stderr_artifact_hash.map(str::to_owned));
    artifacts.sort();
    artifacts.dedup();
    artifacts
}

fn publication_preparation_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

fn publication_candidate_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

fn validate_attribution(actor: &str, idempotency_key: &str) -> Result<(), AppError> {
    if actor.trim().is_empty() || idempotency_key.trim().is_empty() {
        return Err(AppError::new(
            "MCL_ATTRIBUTION_REQUIRED",
            "actor and idempotency key must be nonempty",
            false,
            "Supply `--actor` and `--idempotency-key` with stable values.",
        ));
    }
    Ok(())
}

fn decode_review_payload<T: serde::de::DeserializeOwned>(
    value: Value,
    label: &str,
) -> Result<T, AppError> {
    serde_json::from_value(value).map_err(|error| {
        AppError::new(
            "MCL_FIDELITY_REFERENCE_INVALID",
            format!("stored {label} payload is invalid: {error}"),
            false,
            "Quarantine the invalid canonical record and restore a verified backup.",
        )
    })
}

fn fidelity_status_from_verdict(verdict: FidelityVerdict) -> FidelityStatus {
    match verdict {
        FidelityVerdict::Attested => FidelityStatus::Attested,
        FidelityVerdict::BenchmarkAligned => FidelityStatus::BenchmarkAligned,
        FidelityVerdict::Verified => FidelityStatus::Verified,
        FidelityVerdict::Rejected => FidelityStatus::Rejected,
    }
}

fn fidelity_integrity_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_FIDELITY_EVIDENCE_INTEGRITY_FAILED",
        message,
        false,
        "Quarantine the fidelity history and restore a verified database and artifact backup.",
    )
}

fn artifact_matches_metadata(
    artifact: &ArtifactSnapshot,
    metadata: &ArtifactMetadata,
    byte_size: usize,
) -> bool {
    artifact.byte_size == byte_size as u64
        && artifact.media_type == metadata.media_type
        && artifact.creation_source == metadata.creation_source
        && artifact.license_expression == metadata.license_expression
        && artifact.restriction == metadata.restriction
        && artifact.semantic_metadata == metadata.semantic_metadata
}

fn lean_version(command: &str) -> Result<String, String> {
    if command != "lean" && command != "lean.exe" {
        return Err(format!(
            "configured verifier executable `{command}` is not allowlisted"
        ));
    }
    let output = Command::new(command)
        .arg("--version")
        .output()
        .map_err(|error| format!("could not execute {command}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{command} --version exited with {}", output.status));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("Lean version output was not UTF-8: {error}"))
}

fn passed_check(name: &'static str, detail: String) -> Check {
    Check {
        name,
        healthy: true,
        detail,
        corrective_action: None,
    }
}

fn failed_check(
    name: &'static str,
    detail: impl Into<String>,
    corrective_action: impl Into<String>,
) -> Check {
    Check {
        name,
        healthy: false,
        detail: detail.into(),
        corrective_action: Some(corrective_action.into()),
    }
}

fn report(profile: &'static str, checks: Vec<Check>) -> DiagnosticReport {
    DiagnosticReport {
        healthy: checks.iter().all(|check| check.healthy),
        profile,
        checks,
    }
}

pub fn root_exists(path: &Path) -> bool {
    path.is_dir()
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        PublicationClassification, PublicationRetainedArtifactRole,
        PublicationRetainedClosureEntry, PublicationRunnerEnvironment,
    };

    use super::*;

    fn publication_request() -> PublicationRequest {
        let policy = crate::domain::publication::committed_publication_policy()
            .expect("committed publication policy");
        PublicationRequest {
            schema_version: crate::domain::publication::PUBLICATION_REQUEST_SCHEMA_VERSION
                .to_owned(),
            subject: ExactVersionReference {
                object_id: "018f0000-0000-7000-8000-000000000001".to_owned(),
                version_hash: "a".repeat(64),
            },
            outcome: PublicationOutcome::Proof,
            diagnostic_evidence_id: "018f0000-0000-7000-8000-000000000002".to_owned(),
            diagnostic_evidence_hash: "b".repeat(64),
            proof_closure_evidence_id: "018f0000-0000-7000-8000-000000000003".to_owned(),
            proof_closure_evidence_hash: "c".repeat(64),
            axiom_audit_evidence_id: "018f0000-0000-7000-8000-000000000004".to_owned(),
            axiom_audit_evidence_hash: "d".repeat(64),
            environment_hash: "e".repeat(64),
            module_artifact_hash: "f".repeat(64),
            declaration_name: "MathOS.Publication.truth".to_owned(),
            policy_hash: policy.policy_hash().expect("publication policy hash"),
            source_commit_sha: "1".repeat(40),
            source_tree_sha: "2".repeat(40),
        }
    }

    fn retained_closure(request: &PublicationRequest) -> PublicationRetainedClosure {
        let request_hash = request.request_hash().expect("publication request hash");
        let artifacts = PublicationRetainedArtifactRole::ALL
            .into_iter()
            .enumerate()
            .map(|(index, role)| PublicationRetainedClosureEntry {
                role,
                path: role.expected_path().to_owned(),
                identity_hash: format!("{:064x}", index + 16),
                artifact_hash: format!("{:064x}", index + 64),
            })
            .collect::<Vec<_>>();
        let mut closure = PublicationRetainedClosure {
            schema_version: crate::domain::publication::PUBLICATION_RETAINED_CLOSURE_SCHEMA_VERSION
                .to_owned(),
            subject: request.subject.clone(),
            request_hash: request_hash.clone(),
            artifacts,
        };
        let mut bind = |role, identity_hash: &str, artifact_hash: Option<&str>| {
            let entry = closure
                .artifacts
                .iter_mut()
                .find(|entry| entry.role == role)
                .expect("required retained role");
            entry.identity_hash = identity_hash.to_owned();
            if let Some(artifact_hash) = artifact_hash {
                entry.artifact_hash = artifact_hash.to_owned();
            }
        };
        bind(
            PublicationRetainedArtifactRole::PublicationRequest,
            &request_hash,
            Some(&request_hash),
        );
        bind(
            PublicationRetainedArtifactRole::FormalizationVersion,
            &request.subject.version_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::EnvironmentManifest,
            &request.environment_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::LeanModule,
            &request.module_artifact_hash,
            Some(&request.module_artifact_hash),
        );
        bind(
            PublicationRetainedArtifactRole::PublicationPolicy,
            &request.policy_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::DiagnosticEvidence,
            &request.diagnostic_evidence_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::ProofClosureEvidence,
            &request.proof_closure_evidence_hash,
            None,
        );
        bind(
            PublicationRetainedArtifactRole::AxiomAuditEvidence,
            &request.axiom_audit_evidence_hash,
            None,
        );
        closure
    }

    fn canonical_bytes<T: Serialize>(document: &T) -> Vec<u8> {
        canonical_json(&serde_json::to_value(document).expect("serializable publication document"))
            .expect("canonical publication document")
    }

    fn candidate_documents() -> (PublicationReport, PublicationRetainedClosure) {
        let policy = crate::domain::publication::committed_publication_policy()
            .expect("committed publication policy");
        let request = publication_request();
        let closure = retained_closure(&request);
        let retained_artifact_hashes = closure
            .report_retained_artifact_hashes(&request)
            .expect("report retained hashes");
        let report = PublicationReport {
            schema_version: crate::domain::publication::PUBLICATION_REPORT_SCHEMA_VERSION
                .to_owned(),
            request_hash: request.request_hash().expect("request hash"),
            request,
            classification: PublicationClassification::Passed,
            repository: policy.repository,
            workflow_path: policy.workflow_path,
            source_ref: policy.required_source_ref,
            workflow_run_id: 1,
            workflow_run_attempt: 1,
            runner_environment: PublicationRunnerEnvironment::GithubHosted,
            observed_lean_toolchain: policy.required_lean_toolchain,
            observed_axioms: Vec::new(),
            retained_artifact_hashes,
            clean_checkout: true,
            dependency_closure_complete: true,
            network_isolation_enforced: true,
            memory_limit_enforced: true,
            authoritative: false,
        };
        (report, closure)
    }

    #[test]
    fn publication_candidate_documents_bind_exact_canonical_hashes() {
        let (report, closure) = candidate_documents();
        let report_bytes = canonical_bytes(&report);
        let closure_bytes = canonical_bytes(&closure);
        let validated = validate_publication_candidate_documents(&report_bytes, &closure_bytes)
            .expect("closed publication candidate");

        assert_eq!(validated.request_hash, report.request_hash);
        assert_eq!(
            validated.report_content_hash,
            format!("{:x}", Sha256::digest(&report_bytes))
        );
        assert_eq!(
            validated.retained_closure_hash,
            format!("{:x}", Sha256::digest(&closure_bytes))
        );
    }

    #[test]
    fn publication_candidate_documents_reject_noncanonical_or_unknown_json() {
        let (report, closure) = candidate_documents();
        let report_bytes = canonical_bytes(&report);
        let closure_bytes = canonical_bytes(&closure);

        let mut noncanonical_report = report_bytes.clone();
        noncanonical_report.push(b'\n');
        assert_eq!(
            validate_publication_candidate_documents(&noncanonical_report, &closure_bytes)
                .expect_err("report newline must fail")
                .code,
            "MCL_PUBLICATION_REPORT_NONCANONICAL"
        );
        let mut noncanonical_closure = closure_bytes.clone();
        noncanonical_closure.push(b' ');
        assert_eq!(
            validate_publication_candidate_documents(&report_bytes, &noncanonical_closure)
                .expect_err("closure whitespace must fail")
                .code,
            "MCL_PUBLICATION_RETAINED_CLOSURE_NONCANONICAL"
        );

        let mut unknown_report: Value =
            serde_json::from_slice(&report_bytes).expect("report value");
        unknown_report["unexpected"] = Value::Bool(true);
        let unknown_report = canonical_json(&unknown_report).expect("canonical unknown report");
        assert_eq!(
            validate_publication_candidate_documents(&unknown_report, &closure_bytes)
                .expect_err("unknown report field must fail")
                .code,
            "MCL_PUBLICATION_REPORT_JSON_INVALID"
        );
    }

    #[test]
    fn publication_candidate_documents_reject_altered_report_or_closure() {
        let (mut report, closure) = candidate_documents();
        let closure_bytes = canonical_bytes(&closure);
        report.retained_artifact_hashes.pop();
        assert_eq!(
            validate_publication_candidate_documents(&canonical_bytes(&report), &closure_bytes)
                .expect_err("altered report retention must fail")
                .code,
            "MCL_PUBLICATION_RETAINED_ARTIFACTS_MISMATCH"
        );

        let (report, mut closure) = candidate_documents();
        closure
            .artifacts
            .iter_mut()
            .find(|entry| entry.role == PublicationRetainedArtifactRole::VerifierStdout)
            .expect("verifier stdout role")
            .artifact_hash = "9".repeat(64);
        assert_eq!(
            validate_publication_candidate_documents(
                &canonical_bytes(&report),
                &canonical_bytes(&closure),
            )
            .expect_err("altered retained closure must fail")
            .code,
            "MCL_PUBLICATION_RETAINED_ARTIFACTS_MISMATCH"
        );
    }

    #[test]
    fn retained_member_reader_hashes_every_fixed_role_file() {
        let request = publication_request();
        let mut closure = retained_closure(&request);
        let root = tempfile::TempDir::new().expect("retained root");
        for entry in &mut closure.artifacts {
            let bytes = format!("retained:{}", entry.role.as_str()).into_bytes();
            let path = root.path().join(&entry.path);
            fs::create_dir_all(path.parent().expect("retained parent"))
                .expect("retained directory");
            fs::write(&path, &bytes).expect("retained member");
            entry.artifact_hash = format!("{:x}", Sha256::digest(&bytes));
        }
        let loaded = read_retained_publication_files(root.path(), &closure)
            .expect("all exact retained files");
        assert_eq!(
            loaded.bytes_by_role.len(),
            PublicationRetainedArtifactRole::ALL.len()
        );

        let altered = root
            .path()
            .join(PublicationRetainedArtifactRole::ProtectedStdout.expected_path());
        fs::write(altered, b"altered").expect("alter retained member");
        assert_eq!(
            read_retained_publication_files(root.path(), &closure)
                .expect_err("altered retained member must fail")
                .code,
            "MCL_PUBLICATION_RETAINED_MEMBER_HASH_MISMATCH"
        );
    }

    #[test]
    fn protected_dependency_output_allows_only_the_pinned_implicit_init_closure() {
        validate_protected_dependency_output(
            b"/opt/lib/lean/Init.olean\n/opt/lib/lean/Init.olean\n",
            b"",
        )
        .expect("pinned implicit dependency closure");

        for (stdout, stderr) in [
            (b"".as_slice(), b"".as_slice()),
            (
                b"/opt/lib/lean/Init.olean\n/opt/lib/lean/Init/System/IO.olean\n".as_slice(),
                b"".as_slice(),
            ),
            (b"/tmp/Init.olean\n".as_slice(), b"".as_slice()),
            (
                b"/opt/lib/lean/Init.olean\n".as_slice(),
                b"dependency warning\n".as_slice(),
            ),
        ] {
            assert_eq!(
                validate_protected_dependency_output(stdout, stderr)
                    .expect_err("altered dependency output must fail")
                    .code,
                "MCL_PUBLICATION_PROTECTED_DEPENDENCY_MISMATCH"
            );
        }
    }
}
