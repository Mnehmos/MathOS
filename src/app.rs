use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
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
    LeanAuditRequest, PublicationOutcome, PublicationRequest, RecordDraft, RecordKind,
    RecordSnapshot, RunChainReport, RunEventDraft, RunEventSnapshot, RunKind, RunSnapshot,
    VerifierExecutionClassification, VerifierExecutionReport, VerifierJobRequest,
    VerifierJobSnapshot, VerifierJobState,
};
use crate::error::AppError;
use crate::store::Store;

const DOCTOR_CANARY: &[u8] = b"mcl doctor artifact integrity canary v1";

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
