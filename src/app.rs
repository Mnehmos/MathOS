use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

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
    EvidenceSnapshot, FidelityReviewHistoryEntry, FidelityReviewReport, FidelityStatus,
    FidelityStatusSnapshot, FidelityVerdict, GraphTraversalHit, GraphTraversalRequest,
    LeanAuditClassification, LeanAuditJobSnapshot, LeanAuditReport, LeanAuditRequest,
    PublicationAttestationVerification, PublicationAuthorityBinding, PublicationClassification,
    PublicationIngestionReceiptSnapshot, PublicationOutcome, PublicationReport, PublicationRequest,
    PublicationRetainedArtifactRole, PublicationRetainedClosure, PublicationStage,
    PublicationStageArtifact, PublicationStageSnapshot, RecordDraft, RecordKind, RecordSnapshot,
    RunChainReport, RunEventDraft, RunEventSnapshot, RunKind, RunSnapshot,
    VerifierExecutionClassification, VerifierExecutionReport, VerifierJobRequest,
    VerifierJobSnapshot, VerifierJobState,
};
use crate::error::AppError;
use crate::store::{ClaimStatusReadBasis, PublicationAuthorityCommit, Store};

const DOCTOR_CANARY: &[u8] = b"mcl doctor artifact integrity canary v1";
const MAX_RETAINED_JSON_BYTES: usize = 1_048_576;
const MAX_RETAINED_LEAN_BYTES: usize = 1_048_576;
const MAX_RETAINED_LOG_BYTES: usize = 16 * 1_048_576;
const MAX_ATTESTATION_BUNDLE_BYTES: usize = 512 * 1_024;

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
pub struct PublicationStageOutcome {
    pub dry_run: bool,
    pub proposed_stage_hash: String,
    pub report_artifact_hash: String,
    pub retained_closure_artifact_hash: String,
    pub attestation_bundle_artifact_hash: String,
    pub stage: Option<PublicationStageSnapshot>,
    pub authoritative: bool,
}

#[derive(Debug, Serialize)]
pub struct PublicationIngestionOutcome {
    pub dry_run: bool,
    pub proposed_receipt_hash: String,
    pub verification: PublicationAttestationVerification,
    pub receipt: Option<PublicationIngestionReceiptSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct PublicationAuthorityPromotionOutcome {
    pub dry_run: bool,
    pub publication_receipt_hash: String,
    pub proposed_evidence_hash: String,
    pub evidence_kind: EvidenceKind,
    pub evidence: Option<EvidenceSnapshot>,
}

struct RevalidatedPublicationAuthority {
    receipt_hash: String,
    commit: PublicationAuthorityCommit,
}

#[derive(Debug, Serialize)]
pub struct FidelityReviewOutcome {
    pub dry_run: bool,
    pub proposed_report_artifact_hash: String,
    pub proposed_evidence_hash: String,
    pub report: crate::domain::fidelity::VersionedFidelityReviewReport,
    pub evidence: Option<EvidenceSnapshot>,
}

pub struct Application {
    store: Store,
    artifacts: ArtifactStore,
    verifier_command: String,
    publication_verifier: crate::config::PublicationVerifierConfig,
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
            publication_verifier: config.publication_verifier.clone(),
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
                    .all_registered_cas_hashes()?
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
            publication_authority: None,
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
        request: &crate::domain::fidelity::VersionedFidelityReviewRequest,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<FidelityReviewOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        request.validate()?;
        if request.reviewer_identity() != actor {
            return Err(AppError::new(
                "MCL_FIDELITY_REVIEWER_MISMATCH",
                "reviewer identity must equal the attributed actor",
                false,
                "Submit the review under the reviewer's own actor identity.",
            ));
        }
        let source = self
            .store
            .get_record_version(&request.source().version_hash)?;
        let claim = self
            .store
            .get_record_version(&request.claim().version_hash)?;
        let formalization = self
            .store
            .get_record_version(&request.formalization().version_hash)?;
        if source.object_id != request.source().object_id
            || source.kind != RecordKind::Source
            || claim.object_id != request.claim().object_id
            || claim.kind != RecordKind::Claim
            || formalization.object_id != request.formalization().object_id
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
        if claim_payload.source_reference != *request.source()
            || formal_payload.claim_version != *request.claim()
        {
            return Err(AppError::new(
                "MCL_FIDELITY_LINEAGE_MISMATCH",
                "source, claim, and formalization do not form one exact lineage",
                false,
                "Review the exact source referenced by the claim and the exact claim referenced by the formalization.",
            ));
        }
        if !claim_payload.ambiguity_notes.is_empty()
            && request.ambiguity_disposition() == crate::domain::AmbiguityDisposition::NoAmbiguity
        {
            return Err(AppError::new(
                "MCL_FIDELITY_AMBIGUITY_INVALID",
                "claim ambiguity cannot be silently discarded by the review",
                false,
                "Preserve, resolve, or leave the recorded ambiguity explicitly unresolved.",
            ));
        }
        if (request.review_level() == crate::domain::FidelityReviewLevel::SourcePaperCorrespondence
            && source_payload.source_type != SourceType::Paper)
            || (request.review_level()
                == crate::domain::FidelityReviewLevel::BenchmarkHashAlignment
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
        if request.reviewed_source_relation().is_some_and(|relation| {
            !relation.matches_formalization_polarity(formal_payload.claim_polarity)
        }) {
            return Err(AppError::new(
                "MCL_FIDELITY_RELATION_MISMATCH",
                "reviewed source relation does not match the immutable formalization polarity",
                false,
                "Review the claim for claim polarity or its logical negation for negation polarity.",
            ));
        }
        let current = self.fidelity_status(request.formalization())?;
        let may_be_exact_retry = current
            .history
            .iter()
            .any(|entry| entry.report.request() == *request);
        if request.supersedes_evidence_id() != current.head_evidence_id.as_deref()
            && !may_be_exact_retry
        {
            return Err(AppError::new(
                "MCL_FIDELITY_REVIEW_CONFLICT",
                "fidelity review does not supersede the current exact evidence head",
                true,
                "Reload fidelity status and retry against the current evidence head.",
            ));
        }
        self.store.get_run(request.producing_run_id())?;
        for hash in request.supporting_artifact_hashes() {
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
        let report = match request {
            crate::domain::fidelity::VersionedFidelityReviewRequest::V1(request) => {
                crate::domain::fidelity::VersionedFidelityReviewReport::V1(FidelityReviewReport {
                    schema_version: crate::domain::fidelity::FIDELITY_REVIEW_REPORT_SCHEMA_VERSION
                        .to_owned(),
                    request_hash: request.request_hash()?,
                    request: request.clone(),
                    formalization_author: formalization.created_by.clone(),
                    exact_theorem_type: formal_payload.exact_theorem_type.clone(),
                    declaration_hash: formal_payload.declaration_hash.clone(),
                })
            }
            crate::domain::fidelity::VersionedFidelityReviewRequest::V2(request) => {
                crate::domain::fidelity::VersionedFidelityReviewReport::V2(
                    crate::domain::fidelity::FidelityReviewReportV2 {
                        schema_version:
                            crate::domain::fidelity::FIDELITY_REVIEW_REPORT_V2_SCHEMA_VERSION
                                .to_owned(),
                        request_hash: request.request_hash()?,
                        request: request.clone(),
                        reviewed_source_relation: request.reviewed_source_relation,
                        formalization_author: formalization.created_by.clone(),
                        exact_theorem_type: formal_payload.exact_theorem_type.clone(),
                        declaration_hash: formal_payload.declaration_hash.clone(),
                    },
                )
            }
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
        let mut artifact_hashes = request.supporting_artifact_hashes().to_vec();
        artifact_hashes.push(proposed_report_artifact_hash.clone());
        artifact_hashes.sort();
        artifact_hashes.dedup();
        let payload = EvidencePayload {
            schema_version: crate::domain::evidence::EVIDENCE_SCHEMA_VERSION.to_owned(),
            subject: request.formalization().clone(),
            evidence_kind: EvidenceKind::StatementFidelityReview,
            result: if request.verdict() == FidelityVerdict::Rejected {
                EvidenceResult::Rejected
            } else {
                EvidenceResult::Accepted
            },
            authority_class: EvidenceAuthorityClass::Reviewed,
            producing_run_id: Some(request.producing_run_id().to_owned()),
            producing_job_id: None,
            artifact_hashes,
            verifier_or_reviewer_identity: actor.to_owned(),
            environment_hash: None,
            supersedes_evidence_id: request.supersedes_evidence_id().map(str::to_owned),
            stale: false,
            stale_reason: None,
            publication_authority: None,
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
                    ("request_hash".to_owned(), report.request_hash().to_owned()),
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
        self.fidelity_status_from_evidence(formalization, evidence)
    }

    fn fidelity_status_from_evidence(
        &self,
        formalization: &ExactVersionReference,
        mut evidence: Vec<EvidenceSnapshot>,
    ) -> Result<FidelityStatusSnapshot, AppError> {
        evidence.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.evidence_id.cmp(&right.evidence_id))
        });
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
                let status = fidelity_status_from_verdict(report.request().verdict());
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

    pub fn claim_research_status(
        &mut self,
        claim: &ExactVersionReference,
    ) -> Result<crate::domain::ClaimResearchStatusSnapshot, AppError> {
        use crate::domain::{
            AmbiguityDisposition, ClaimResearchStatusNonqualification,
            ClaimResearchStatusNonqualificationReason, ClaimResearchStatusSnapshot,
            ClaimResearchStatusWitness, ClaimResearchStatusWitnessKind, ResearchStatus,
            ReviewedSourceRelation, VersionedFidelityReviewReport,
        };

        let claim_record =
            self.read_validated_exact_record(claim, RecordKind::Claim, "claim-status claim")?;
        let claim_payload: ClaimPayload =
            decode_review_payload(claim_record.payload, "claim-status claim")?;
        self.read_validated_exact_record(
            &claim_payload.source_reference,
            RecordKind::Source,
            "claim-status source",
        )?;
        let basis = self.store.capture_claim_status_read_basis(claim)?;
        let empty_snapshot = |status| ClaimResearchStatusSnapshot {
            schema_version: crate::domain::research_status::CLAIM_RESEARCH_STATUS_SCHEMA_VERSION
                .to_owned(),
            claim: claim.clone(),
            status,
            witnesses: Vec::new(),
            nonqualifications: Vec::new(),
        };

        if basis.current_claim_head_version_hash.as_deref() != Some(&claim.version_hash) {
            return self
                .finish_claim_research_status(&basis, empty_snapshot(ResearchStatus::Superseded));
        }
        if basis.formalizations.is_empty() {
            return self
                .finish_claim_research_status(&basis, empty_snapshot(ResearchStatus::NotStarted));
        }

        let mut witnesses = Vec::new();
        let mut nonqualifications = Vec::new();
        for formalization_basis in &basis.formalizations {
            let formalization = formalization_basis.formalization.clone();
            let formalization_record = self.read_validated_exact_record(
                &formalization,
                RecordKind::Formalization,
                "claim-status formalization",
            )?;
            let formal_payload: FormalizationPayload =
                decode_review_payload(formalization_record.payload, "claim-status formalization")?;
            if formal_payload.claim_version != *claim {
                return Err(claim_status_integrity_error(
                    "current formalization does not reference the requested exact claim",
                ));
            }

            // Read and validate the entire fidelity history, not only the row
            // that might qualify as its current head.
            let mut fidelity_evidence =
                Vec::with_capacity(formalization_basis.fidelity_evidence.len());
            for identity in &formalization_basis.fidelity_evidence {
                let evidence = self.store.get_evidence(&identity.evidence_id)?;
                if evidence.evidence_hash != identity.evidence_hash {
                    return Err(claim_status_integrity_error(
                        "captured fidelity evidence identity changed during replay",
                    ));
                }
                fidelity_evidence.push(evidence);
            }
            let fidelity = self.fidelity_status_from_evidence(&formalization, fidelity_evidence)?;

            // Likewise, replay every authority row before deciding whether
            // one deterministic witness can qualify. A corrupt unused row is
            // still a corrupt relevant truth input and must fail the read.
            let mut validated_authorities =
                Vec::with_capacity(formalization_basis.authoritative_evidence.len());
            for identity in &formalization_basis.authoritative_evidence {
                let evidence = self.store.get_evidence(&identity.evidence_id)?;
                if evidence.evidence_hash != identity.evidence_hash {
                    return Err(claim_status_integrity_error(
                        "captured authority evidence identity changed during replay",
                    ));
                }
                let commit = self.revalidate_publication_authority_evidence(&evidence)?;
                let kind = match commit.outcome {
                    PublicationOutcome::Proof => EvidenceKind::LeanKernelProof,
                    PublicationOutcome::Refutation => EvidenceKind::LeanKernelRefutation,
                };
                validated_authorities.push((evidence, kind));
            }
            let has_authority_proof = validated_authorities
                .iter()
                .any(|(_, kind)| *kind == EvidenceKind::LeanKernelProof);
            let has_authority_refutation = validated_authorities
                .iter()
                .any(|(_, kind)| *kind == EvidenceKind::LeanKernelRefutation);
            if has_authority_proof && has_authority_refutation {
                return Err(claim_status_integrity_error(
                    "one exact formalization has both proof and refutation authority",
                ));
            }

            let head = fidelity.head_evidence_id.as_deref().and_then(|head_id| {
                fidelity
                    .history
                    .iter()
                    .find(|entry| entry.evidence.evidence_id == head_id)
            });
            let verified_head = head.filter(|entry| {
                entry.status == FidelityStatus::Verified
                    && !entry.evidence.payload.stale
                    && entry.evidence.payload.result == EvidenceResult::Accepted
                    && entry.evidence.payload.authority_class == EvidenceAuthorityClass::Reviewed
            });
            let Some(fidelity_head) = verified_head else {
                let reason = if claim_payload.ambiguity_notes.is_empty() {
                    ClaimResearchStatusNonqualificationReason::NoCurrentVerifiedFidelity
                } else {
                    ClaimResearchStatusNonqualificationReason::SourceAmbiguityUnresolved
                };
                nonqualifications.push(ClaimResearchStatusNonqualification {
                    formalization: formalization.clone(),
                    reason,
                    fidelity_evidence_id: head.map(|entry| entry.evidence.evidence_id.clone()),
                    authority_evidence_id: None,
                });
                continue;
            };

            let request = fidelity_head.report.request();
            let ambiguity_reason = match request.ambiguity_disposition() {
                AmbiguityDisposition::Unresolved => {
                    Some(ClaimResearchStatusNonqualificationReason::SourceAmbiguityUnresolved)
                }
                AmbiguityDisposition::PreservedVariants => {
                    Some(ClaimResearchStatusNonqualificationReason::SourceAmbiguityPreserved)
                }
                AmbiguityDisposition::NoAmbiguity | AmbiguityDisposition::ResolvedFromSource => {
                    None
                }
            };
            if let Some(reason) = ambiguity_reason {
                nonqualifications.push(ClaimResearchStatusNonqualification {
                    formalization: formalization.clone(),
                    reason,
                    fidelity_evidence_id: Some(fidelity_head.evidence.evidence_id.clone()),
                    authority_evidence_id: None,
                });
                continue;
            }

            let expected = match formal_payload.claim_polarity {
                Some(FormalizationClaimPolarity::Claim) => Some((
                    ClaimResearchStatusWitnessKind::Proof,
                    ReviewedSourceRelation::Claim,
                    EvidenceKind::LeanKernelProof,
                )),
                Some(FormalizationClaimPolarity::Negation) => Some((
                    ClaimResearchStatusWitnessKind::Refutation,
                    ReviewedSourceRelation::LogicalNegation,
                    EvidenceKind::LeanKernelRefutation,
                )),
                None => None,
            };
            let Some((witness_kind, expected_relation, expected_authority_kind)) = expected else {
                nonqualifications.push(ClaimResearchStatusNonqualification {
                    formalization: formalization.clone(),
                    reason: ClaimResearchStatusNonqualificationReason::FidelityRelationUnbound,
                    fidelity_evidence_id: Some(fidelity_head.evidence.evidence_id.clone()),
                    authority_evidence_id: None,
                });
                continue;
            };
            let (reviewed_relation, v1_relation_unbound) = match &fidelity_head.report {
                VersionedFidelityReviewReport::V1(_) => (
                    ReviewedSourceRelation::Claim,
                    witness_kind == ClaimResearchStatusWitnessKind::Refutation,
                ),
                VersionedFidelityReviewReport::V2(report) => {
                    (report.reviewed_source_relation, false)
                }
            };
            if v1_relation_unbound {
                nonqualifications.push(ClaimResearchStatusNonqualification {
                    formalization: formalization.clone(),
                    reason: ClaimResearchStatusNonqualificationReason::FidelityRelationUnbound,
                    fidelity_evidence_id: Some(fidelity_head.evidence.evidence_id.clone()),
                    authority_evidence_id: None,
                });
                continue;
            }
            if reviewed_relation != expected_relation {
                nonqualifications.push(ClaimResearchStatusNonqualification {
                    formalization: formalization.clone(),
                    reason: ClaimResearchStatusNonqualificationReason::FidelityRelationMismatch,
                    fidelity_evidence_id: Some(fidelity_head.evidence.evidence_id.clone()),
                    authority_evidence_id: None,
                });
                continue;
            }

            let Some((authority, _)) = validated_authorities
                .iter()
                .find(|(_, kind)| *kind == expected_authority_kind)
            else {
                let (reason, authority_evidence_id) =
                    if let Some((authority, _)) = validated_authorities.first() {
                        (
                            ClaimResearchStatusNonqualificationReason::AuthorityKindMismatch,
                            Some(authority.evidence_id.clone()),
                        )
                    } else {
                        (
                        ClaimResearchStatusNonqualificationReason::NoCurrentAuthoritativeEvidence,
                        None,
                    )
                    };
                nonqualifications.push(ClaimResearchStatusNonqualification {
                    formalization: formalization.clone(),
                    reason,
                    fidelity_evidence_id: Some(fidelity_head.evidence.evidence_id.clone()),
                    authority_evidence_id,
                });
                continue;
            };
            let publication_receipt_hash = authority
                .payload
                .publication_authority
                .as_ref()
                .map(|binding| binding.ingestion_receipt_hash.clone())
                .ok_or_else(|| {
                    claim_status_integrity_error(
                        "revalidated authority evidence has no publication receipt binding",
                    )
                })?;
            witnesses.push(ClaimResearchStatusWitness {
                formalization: formalization.clone(),
                kind: witness_kind,
                reviewed_source_relation: reviewed_relation,
                fidelity_request_schema_version: request.schema_version().to_owned(),
                fidelity_evidence_id: fidelity_head.evidence.evidence_id.clone(),
                fidelity_evidence_hash: fidelity_head.evidence.evidence_hash.clone(),
                fidelity_report_artifact_hash: fidelity_head.report_artifact_hash.clone(),
                authority_evidence_id: authority.evidence_id.clone(),
                authority_evidence_hash: authority.evidence_hash.clone(),
                publication_receipt_hash,
            });
        }

        let has_proof = witnesses
            .iter()
            .any(|witness| witness.kind == ClaimResearchStatusWitnessKind::Proof);
        let has_refutation = witnesses
            .iter()
            .any(|witness| witness.kind == ClaimResearchStatusWitnessKind::Refutation);
        let has_source_ambiguity = nonqualifications.iter().any(|item| {
            matches!(
                item.reason,
                ClaimResearchStatusNonqualificationReason::SourceAmbiguityUnresolved
                    | ClaimResearchStatusNonqualificationReason::SourceAmbiguityPreserved
            )
        });
        let status = if has_source_ambiguity || (has_proof && has_refutation) {
            ResearchStatus::Ambiguous
        } else if has_proof {
            ResearchStatus::Proved
        } else if has_refutation {
            ResearchStatus::Disproved
        } else {
            ResearchStatus::Open
        };
        self.finish_claim_research_status(
            &basis,
            ClaimResearchStatusSnapshot {
                schema_version:
                    crate::domain::research_status::CLAIM_RESEARCH_STATUS_SCHEMA_VERSION.to_owned(),
                claim: claim.clone(),
                status,
                witnesses,
                nonqualifications,
            },
        )
    }

    fn finish_claim_research_status(
        &self,
        basis: &ClaimStatusReadBasis,
        mut snapshot: crate::domain::ClaimResearchStatusSnapshot,
    ) -> Result<crate::domain::ClaimResearchStatusSnapshot, AppError> {
        snapshot.sort_components();
        snapshot.validate()?;
        self.store.recheck_claim_status_read_basis(basis)?;
        Ok(snapshot)
    }

    fn read_fidelity_report(
        &self,
        evidence: &EvidenceSnapshot,
    ) -> Result<
        (
            String,
            crate::domain::fidelity::VersionedFidelityReviewReport,
        ),
        AppError,
    > {
        let mut reports = Vec::new();
        for hash in &evidence.payload.artifact_hashes {
            let artifact = self.store.get_artifact(hash)?;
            let bytes = self.artifacts.read(hash)?;
            if artifact.artifact_hash != *hash || artifact.byte_size != bytes.len() as u64 {
                return Err(fidelity_integrity_error(format!(
                    "fidelity artifact {hash} metadata disagrees with its verified CAS bytes"
                )));
            }
            if artifact.media_type == crate::domain::ArtifactMediaType::Json
                && artifact.creation_source == crate::domain::ArtifactCreationSource::HumanReview
                && artifact.restriction == crate::domain::ArtifactRestriction::Private
                && artifact
                    .semantic_metadata
                    .get("artifact_role")
                    .is_some_and(|role| role == "fidelity_review_report")
            {
                let report: crate::domain::fidelity::VersionedFidelityReviewReport =
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
                let canonical =
                    canonical_json(&serde_json::to_value(&report).map_err(|error| {
                        fidelity_integrity_error(format!(
                            "stored fidelity report cannot be serialized: {error}"
                        ))
                    })?)?;
                if canonical != bytes {
                    return Err(fidelity_integrity_error(
                        "stored fidelity report is not canonical JSON",
                    ));
                }
                reports.push((hash.clone(), artifact, report));
            }
        }
        let [(hash, artifact, report)] = reports.as_slice() else {
            return Err(fidelity_integrity_error(
                "fidelity evidence does not resolve to exactly one controlled review report",
            ));
        };
        let request = report.request();
        let expected_result = if request.verdict() == FidelityVerdict::Rejected {
            EvidenceResult::Rejected
        } else {
            EvidenceResult::Accepted
        };
        let mut expected_artifact_hashes = request.supporting_artifact_hashes().to_vec();
        expected_artifact_hashes.push(hash.clone());
        expected_artifact_hashes.sort();
        expected_artifact_hashes.dedup();
        let expected_metadata = BTreeMap::from([
            (
                "artifact_role".to_owned(),
                "fidelity_review_report".to_owned(),
            ),
            (
                "reviewer_identity".to_owned(),
                request.reviewer_identity().to_owned(),
            ),
            ("request_hash".to_owned(), report.request_hash().to_owned()),
        ]);
        if evidence.payload.schema_version != crate::domain::evidence::EVIDENCE_SCHEMA_VERSION
            || evidence.payload.subject != *request.formalization()
            || evidence.payload.evidence_kind != EvidenceKind::StatementFidelityReview
            || evidence.payload.result != expected_result
            || evidence.payload.authority_class != EvidenceAuthorityClass::Reviewed
            || evidence.payload.producing_run_id.as_deref() != Some(request.producing_run_id())
            || evidence.payload.producing_job_id.is_some()
            || evidence.payload.artifact_hashes != expected_artifact_hashes
            || evidence.payload.verifier_or_reviewer_identity != request.reviewer_identity()
            || evidence.payload.environment_hash.is_some()
            || evidence.payload.supersedes_evidence_id.as_deref()
                != request.supersedes_evidence_id()
            || evidence.payload.publication_authority.is_some()
            || evidence.created_by != request.reviewer_identity()
            || artifact.created_by != request.reviewer_identity()
            || artifact.semantic_metadata != expected_metadata
        {
            return Err(fidelity_integrity_error(
                "fidelity report, evidence, and artifact provenance disagree",
            ));
        }

        let source = self.read_validated_exact_record(
            request.source(),
            RecordKind::Source,
            "fidelity source",
        )?;
        let claim =
            self.read_validated_exact_record(request.claim(), RecordKind::Claim, "fidelity claim")?;
        let formalization = self.read_validated_exact_record(
            request.formalization(),
            RecordKind::Formalization,
            "fidelity formalization",
        )?;
        let source_payload: SourcePayload =
            decode_review_payload(source.payload, "fidelity source")?;
        let claim_payload: ClaimPayload = decode_review_payload(claim.payload, "fidelity claim")?;
        let formal_payload: FormalizationPayload =
            decode_review_payload(formalization.payload, "fidelity formalization")?;
        if claim_payload.source_reference != *request.source()
            || formal_payload.claim_version != *request.claim()
            || report.formalization_author() != formalization.created_by
            || report.exact_theorem_type() != formal_payload.exact_theorem_type
            || report.declaration_hash() != formal_payload.declaration_hash
            || (!claim_payload.ambiguity_notes.is_empty()
                && request.ambiguity_disposition()
                    == crate::domain::AmbiguityDisposition::NoAmbiguity)
            || (request.review_level()
                == crate::domain::FidelityReviewLevel::SourcePaperCorrespondence
                && source_payload.source_type != SourceType::Paper)
            || (request.review_level()
                == crate::domain::FidelityReviewLevel::BenchmarkHashAlignment
                && (source_payload.source_type != SourceType::Benchmark
                    || source_payload.content_hash.is_none()))
        {
            return Err(fidelity_integrity_error(
                "fidelity report does not reproduce its exact canonical lineage",
            ));
        }
        self.store.get_run(request.producing_run_id())?;
        Ok((hash.clone(), report.clone()))
    }

    fn read_validated_exact_record(
        &self,
        reference: &ExactVersionReference,
        expected_kind: RecordKind,
        label: &str,
    ) -> Result<RecordSnapshot, AppError> {
        let record = self.store.get_record_version(&reference.version_hash)?;
        if record.object_id != reference.object_id || record.kind != expected_kind {
            return Err(claim_status_integrity_error(format!(
                "{label} does not resolve to the requested exact canonical record"
            )));
        }
        validate_record_payload(record.kind, &record.schema_version, &record.payload).map_err(
            |error| {
                claim_status_integrity_error(format!(
                    "{label} failed schema validation: {}",
                    error.message
                ))
            },
        )?;
        let rehashed = record_version_hash(&record.schema_version, &record.payload)?;
        if rehashed != record.version_hash || rehashed != reference.version_hash {
            return Err(claim_status_integrity_error(format!(
                "{label} failed canonical version rehashing"
            )));
        }
        Ok(record)
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
            publication_authority: None,
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
        self.validate_publication_candidate_against_current_store(&candidate, &retained_files)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stage_publication_candidate(
        &mut self,
        report_bytes: &[u8],
        retained_closure_bytes: &[u8],
        retained_root: &Path,
        attestation_bundle_bytes: &[u8],
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<PublicationStageOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        if attestation_bundle_bytes.is_empty()
            || attestation_bundle_bytes.len() > MAX_ATTESTATION_BUNDLE_BYTES
        {
            return Err(publication_ingestion_error(
                "MCL_PUBLICATION_BUNDLE_INVALID",
                "attestation bundle is empty or exceeds the 512 KiB staging bound",
                "Stage one bounded Sigstore JSON bundle emitted for the exact candidate report.",
            ));
        }
        let bundle_value: Value =
            serde_json::from_slice(attestation_bundle_bytes).map_err(|error| {
                publication_ingestion_error(
                    "MCL_PUBLICATION_BUNDLE_INVALID",
                    format!("attestation bundle is not valid JSON: {error}"),
                    "Stage the exact Sigstore JSON bundle emitted by the protected workflow.",
                )
            })?;
        validate_sigstore_bundle_shape(&bundle_value)?;

        let candidate =
            validate_publication_candidate_documents(report_bytes, retained_closure_bytes)?;
        let retained_files =
            read_retained_publication_files(retained_root, &candidate.retained_closure)?;
        validate_retained_publication_semantics(
            &candidate.report,
            &candidate.retained_closure,
            &retained_files,
        )?;

        let attestation_bundle_artifact_hash =
            format!("{:x}", Sha256::digest(attestation_bundle_bytes));
        let retained_artifacts = candidate
            .retained_closure
            .artifacts
            .iter()
            .map(|entry| {
                Ok(PublicationStageArtifact {
                    role: entry.role,
                    path: entry.path.clone(),
                    identity_hash: entry.identity_hash.clone(),
                    artifact_hash: entry.artifact_hash.clone(),
                    byte_size: retained_files.get(entry.role)?.len() as u64,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let stage = PublicationStage {
            schema_version: crate::domain::publication::PUBLICATION_STAGE_SCHEMA_VERSION.to_owned(),
            report_artifact_hash: candidate.report_content_hash.clone(),
            report_byte_size: report_bytes.len() as u64,
            retained_closure_artifact_hash: candidate.retained_closure_hash.clone(),
            retained_closure_byte_size: retained_closure_bytes.len() as u64,
            attestation_bundle_artifact_hash: attestation_bundle_artifact_hash.clone(),
            attestation_bundle_byte_size: attestation_bundle_bytes.len() as u64,
            retained_artifacts,
            authoritative: false,
        };
        stage.validate()?;
        let proposed_stage_hash = stage.stage_hash()?;
        let snapshot = if dry_run {
            None
        } else {
            ensure_staged_hash(
                &self.artifacts,
                report_bytes,
                &stage.report_artifact_hash,
                "publication report",
            )?;
            ensure_staged_hash(
                &self.artifacts,
                retained_closure_bytes,
                &stage.retained_closure_artifact_hash,
                "publication retained closure",
            )?;
            ensure_staged_hash(
                &self.artifacts,
                attestation_bundle_bytes,
                &stage.attestation_bundle_artifact_hash,
                "publication attestation bundle",
            )?;
            for entry in &stage.retained_artifacts {
                ensure_staged_hash(
                    &self.artifacts,
                    retained_files.get(entry.role)?,
                    &entry.artifact_hash,
                    entry.role.as_str(),
                )?;
            }
            Some(
                self.store
                    .register_publication_stage(&stage, actor, idempotency_key)?,
            )
        };
        Ok(PublicationStageOutcome {
            dry_run,
            proposed_stage_hash,
            report_artifact_hash: stage.report_artifact_hash,
            retained_closure_artifact_hash: stage.retained_closure_artifact_hash,
            attestation_bundle_artifact_hash,
            stage: snapshot,
            authoritative: false,
        })
    }

    pub fn ingest_publication(
        &mut self,
        report_artifact_hash: &str,
        attestation_bundle_artifact_hash: &str,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<PublicationIngestionOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        let stage = self
            .store
            .get_publication_stage(report_artifact_hash, attestation_bundle_artifact_hash)?;
        let report_bytes = self.read_staged_publication_bytes(
            &stage.stage.report_artifact_hash,
            stage.stage.report_byte_size,
            "publication report",
        )?;
        let retained_closure_bytes = self.read_staged_publication_bytes(
            &stage.stage.retained_closure_artifact_hash,
            stage.stage.retained_closure_byte_size,
            "publication retained closure",
        )?;
        let attestation_bundle_bytes = self.read_staged_publication_bytes(
            &stage.stage.attestation_bundle_artifact_hash,
            stage.stage.attestation_bundle_byte_size,
            "publication attestation bundle",
        )?;
        let bundle_value: Value =
            serde_json::from_slice(&attestation_bundle_bytes).map_err(|error| {
                publication_ingestion_error(
                    "MCL_PUBLICATION_BUNDLE_INVALID",
                    format!("staged attestation bundle is not valid JSON: {error}"),
                    "Quarantine the stage and restage the exact protected-workflow bundle.",
                )
            })?;
        validate_sigstore_bundle_shape(&bundle_value)?;
        let candidate =
            validate_publication_candidate_documents(&report_bytes, &retained_closure_bytes)?;
        if candidate.report_content_hash != stage.stage.report_artifact_hash
            || candidate.retained_closure_hash != stage.stage.retained_closure_artifact_hash
        {
            return Err(publication_ingestion_error(
                "MCL_PUBLICATION_STAGE_MISMATCH",
                "staged report or retained closure no longer matches its registered identity",
                "Quarantine the altered stage and restage the exact protected candidate.",
            ));
        }
        let retained_files =
            self.read_staged_publication_members(&stage, &candidate.retained_closure)?;
        validate_retained_publication_semantics(
            &candidate.report,
            &candidate.retained_closure,
            &retained_files,
        )?;
        self.validate_publication_candidate_against_current_store(&candidate, &retained_files)?;

        let policy = crate::domain::publication::committed_publication_policy()?;
        if !dry_run {
            let idempotent = self.store.publication_ingestion_idempotency_result(
                &stage.stage_hash,
                actor,
                idempotency_key,
            )?;
            let existing = match idempotent {
                Some(existing) => Some(existing),
                None => match self
                    .store
                    .get_publication_ingestion_receipt_for_stage(&stage.stage_hash)
                {
                    Ok(existing) => Some(existing),
                    Err(error) if error.code == "MCL_PUBLICATION_RECEIPT_NOT_FOUND" => None,
                    Err(error) => return Err(error),
                },
            };
            if let Some(existing) = existing {
                let raw = self.read_staged_publication_bytes(
                    &existing.verification.raw_verification_hash,
                    existing.raw_verification_byte_size,
                    "raw publication attestation verification",
                )?;
                let stored_receipt = self.read_staged_publication_bytes(
                    &existing.receipt_hash,
                    existing.receipt_byte_size,
                    "publication attestation receipt",
                )?;
                validate_persisted_publication_receipt(
                    &existing,
                    &raw,
                    &stored_receipt,
                    &bundle_value,
                    &candidate.report,
                    &policy,
                )?;
                let registered = self.store.register_publication_ingestion_receipt(
                    &stage.stage_hash,
                    &candidate.report.request.subject,
                    &existing.verification,
                    existing.raw_verification_byte_size,
                    existing.receipt_byte_size,
                    actor,
                    idempotency_key,
                )?;
                return Ok(PublicationIngestionOutcome {
                    dry_run: false,
                    proposed_receipt_hash: registered.receipt_hash.clone(),
                    verification: registered.verification.clone(),
                    receipt: Some(registered),
                });
            }
        }
        let resolved_verifier_path =
            resolve_publication_verifier(&self.publication_verifier.gh_command)?;
        let verifier_binary_sha256 = sha256_file(&resolved_verifier_path)?;
        if verifier_binary_sha256 != policy.attestation_verifier_binary_sha256 {
            return Err(publication_ingestion_error(
                "MCL_PUBLICATION_VERIFIER_PIN_MISMATCH",
                "resolved gh verifier binary does not match the committed SHA-256 pin",
                "Install the exact publication-policy gh binary before ingesting an attestation.",
            ));
        }
        let workspace = tempfile::Builder::new()
            .prefix("mcl-publication-attestation-")
            .tempdir_in(&self.workspace_root)
            .map_err(|error| AppError::io("create publication attestation workspace", error))?;
        let verifier_path = workspace
            .path()
            .join(if cfg!(windows) { "gh.exe" } else { "gh" });
        fs::copy(&resolved_verifier_path, &verifier_path)
            .map_err(|error| AppError::io("isolate pinned publication verifier", error))?;
        if sha256_file(&verifier_path)? != verifier_binary_sha256 {
            return Err(publication_ingestion_error(
                "MCL_PUBLICATION_VERIFIER_PIN_MISMATCH",
                "gh verifier changed while it was copied into the private execution workspace",
                "Quarantine the verifier host and rerun on a clean protected runner.",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&verifier_path, fs::Permissions::from_mode(0o500))
                .map_err(|error| AppError::io("lock isolated publication verifier", error))?;
        }
        #[cfg(windows)]
        {
            let mut permissions = fs::metadata(&verifier_path)
                .map_err(|error| AppError::io("inspect isolated publication verifier", error))?
                .permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&verifier_path, permissions)
                .map_err(|error| AppError::io("lock isolated publication verifier", error))?;
        }
        let verifier_version = verify_publication_gh_version(
            &verifier_path,
            &policy.attestation_verifier_version,
            workspace.path(),
        )?;
        let report_path = self.artifacts.materialize(
            &stage.stage.report_artifact_hash,
            workspace.path(),
            "publication-report.json",
        )?;
        let bundle_path = self.artifacts.materialize(
            &stage.stage.attestation_bundle_artifact_hash,
            workspace.path(),
            "attestation.json",
        )?;
        let arguments = publication_attestation_arguments(
            &report_path,
            &bundle_path,
            &candidate.report,
            &policy,
        );
        let capture = crate::verifier::run_bounded_external(
            &verifier_path,
            &arguments,
            workspace.path(),
            Duration::from_secs(self.publication_verifier.timeout_seconds),
            self.publication_verifier.max_output_bytes as u64,
            &[],
            "MCL_PUBLICATION_VERIFIER_LAUNCH_FAILED",
            "pinned GitHub attestation verifier",
        )?;
        if capture.timed_out
            || capture.output_limit_exceeded
            || capture.exit_code != Some(0)
            || !capture.stderr.is_empty()
        {
            return Err(publication_ingestion_error(
                "MCL_PUBLICATION_ATTESTATION_REJECTED",
                format!(
                    "pinned gh verification failed (exit={:?}, timed_out={}, output_limit_exceeded={}, stderr_bytes={})",
                    capture.exit_code,
                    capture.timed_out,
                    capture.output_limit_exceeded,
                    capture.stderr.len()
                ),
                "Inspect the staged hashes and protected provenance; never bypass failed attestation verification.",
            ));
        }
        if sha256_file(&verifier_path)? != verifier_binary_sha256 {
            return Err(publication_ingestion_error(
                "MCL_PUBLICATION_VERIFIER_PIN_MISMATCH",
                "gh verifier binary changed during attestation verification",
                "Quarantine the verifier host and rerun on a clean protected runner.",
            ));
        }
        let parsed = crate::publication_attestation::validate_gh_attestation_output(
            &capture.stdout,
            &bundle_value,
            &candidate.report,
            &policy,
        )?;
        self.validate_publication_candidate_against_current_store(&candidate, &retained_files)?;
        let raw_verification_hash = format!("{:x}", Sha256::digest(&capture.stdout));
        let verification = PublicationAttestationVerification {
            schema_version:
                crate::domain::publication::PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION
                    .to_owned(),
            report_content_hash: candidate.report_content_hash,
            report_artifact_hash: stage.stage.report_artifact_hash.clone(),
            attestation_bundle_hash: stage.stage.attestation_bundle_artifact_hash.clone(),
            raw_verification_hash: raw_verification_hash.clone(),
            verifier_name: "gh".to_owned(),
            verifier_version,
            verifier_binary_sha256,
            repository: policy.repository.clone(),
            signer_workflow: format!("{}/{}", policy.repository, policy.workflow_path),
            certificate_identity: format!(
                "https://github.com/{}/{}@{}",
                policy.repository, policy.workflow_path, policy.required_source_ref
            ),
            source_ref: policy.required_source_ref.clone(),
            source_commit_sha: candidate.report.request.source_commit_sha.clone(),
            predicate_type: policy.attestation_predicate_type.clone(),
            self_hosted_runners_denied: true,
            verified_attestation_count: parsed.verified_attestation_count,
            verified_timestamp_count: parsed.verified_timestamp_count,
            authoritative: false,
        };
        verification.validate(&candidate.report, &policy)?;
        let receipt_bytes =
            canonical_json(&serde_json::to_value(&verification).map_err(|error| {
                publication_ingestion_error(
                    "MCL_PUBLICATION_RECEIPT_INVALID",
                    error.to_string(),
                    "Report this deterministic publication receipt serialization defect.",
                )
            })?)?;
        let proposed_receipt_hash = format!("{:x}", Sha256::digest(&receipt_bytes));
        let receipt = if dry_run {
            None
        } else {
            ensure_staged_hash(
                &self.artifacts,
                &capture.stdout,
                &raw_verification_hash,
                "raw publication attestation verification",
            )?;
            ensure_staged_hash(
                &self.artifacts,
                &receipt_bytes,
                &proposed_receipt_hash,
                "publication attestation receipt",
            )?;
            Some(self.store.register_publication_ingestion_receipt(
                &stage.stage_hash,
                &candidate.report.request.subject,
                &verification,
                capture.stdout.len() as u64,
                receipt_bytes.len() as u64,
                actor,
                idempotency_key,
            )?)
        };
        Ok(PublicationIngestionOutcome {
            dry_run,
            proposed_receipt_hash,
            verification,
            receipt,
        })
    }

    pub fn promote_publication_authority(
        &mut self,
        publication_receipt_hash: &str,
        actor: &str,
        idempotency_key: &str,
        dry_run: bool,
    ) -> Result<PublicationAuthorityPromotionOutcome, AppError> {
        validate_attribution(actor, idempotency_key)?;
        if actor.chars().count() > 256 {
            return Err(publication_authority_error(
                "MCL_ATTRIBUTION_INVALID",
                "publication authority actor attribution exceeds 256 characters",
                "Use a short stable actor identity.",
            ));
        }
        let validated = self.revalidate_publication_authority_receipt(publication_receipt_hash)?;
        let proposed_payload = validated.commit.evidence_payload()?;
        let proposed_evidence_hash = proposed_payload.evidence_hash()?;
        let evidence_kind = proposed_payload.evidence_kind;
        let evidence = if dry_run {
            None
        } else {
            Some(self.store.create_publication_authority_evidence(
                &validated.commit,
                actor,
                idempotency_key,
            )?)
        };
        Ok(PublicationAuthorityPromotionOutcome {
            dry_run,
            publication_receipt_hash: validated.receipt_hash,
            proposed_evidence_hash,
            evidence_kind,
            evidence,
        })
    }

    fn revalidate_publication_authority_receipt(
        &mut self,
        publication_receipt_hash: &str,
    ) -> Result<RevalidatedPublicationAuthority, AppError> {
        let receipt = self
            .store
            .get_publication_ingestion_receipt(publication_receipt_hash)?;
        let stage = self
            .store
            .get_publication_stage_by_hash(&receipt.stage_hash)?;
        let report_bytes = self.read_staged_publication_bytes(
            &stage.stage.report_artifact_hash,
            stage.stage.report_byte_size,
            "publication report",
        )?;
        let retained_closure_bytes = self.read_staged_publication_bytes(
            &stage.stage.retained_closure_artifact_hash,
            stage.stage.retained_closure_byte_size,
            "publication retained closure",
        )?;
        let attestation_bundle_bytes = self.read_staged_publication_bytes(
            &stage.stage.attestation_bundle_artifact_hash,
            stage.stage.attestation_bundle_byte_size,
            "publication attestation bundle",
        )?;
        let bundle_value: Value =
            serde_json::from_slice(&attestation_bundle_bytes).map_err(|error| {
                publication_ingestion_error(
                    "MCL_PUBLICATION_BUNDLE_INVALID",
                    format!("staged attestation bundle is not valid JSON: {error}"),
                    "Quarantine the stage and restage the exact protected-workflow bundle.",
                )
            })?;
        validate_sigstore_bundle_shape(&bundle_value)?;

        let candidate =
            validate_publication_candidate_documents(&report_bytes, &retained_closure_bytes)?;
        if candidate.report_content_hash != stage.stage.report_artifact_hash
            || candidate.retained_closure_hash != stage.stage.retained_closure_artifact_hash
        {
            return Err(publication_authority_error(
                "MCL_PUBLICATION_AUTHORITY_INTEGRITY_FAILED",
                "publication receipt resolves to a stage whose candidate identities disagree",
                "Quarantine the receipt and restore the exact protected publication closure.",
            ));
        }
        let retained_files =
            self.read_staged_publication_members(&stage, &candidate.retained_closure)?;
        validate_retained_publication_semantics(
            &candidate.report,
            &candidate.retained_closure,
            &retained_files,
        )?;
        self.validate_publication_candidate_against_current_store(&candidate, &retained_files)?;

        let raw_verification = self.read_staged_publication_bytes(
            &receipt.verification.raw_verification_hash,
            receipt.raw_verification_byte_size,
            "raw publication attestation verification",
        )?;
        let canonical_receipt = self.read_staged_publication_bytes(
            &receipt.receipt_hash,
            receipt.receipt_byte_size,
            "publication attestation receipt",
        )?;
        let policy = crate::domain::publication::committed_publication_policy()?;
        validate_persisted_publication_receipt(
            &receipt,
            &raw_verification,
            &canonical_receipt,
            &bundle_value,
            &candidate.report,
            &policy,
        )?;
        validate_publication_authority_report(&candidate.report)?;

        let binding = PublicationAuthorityBinding {
            schema_version: crate::domain::evidence::PUBLICATION_AUTHORITY_BINDING_SCHEMA_VERSION
                .to_owned(),
            ingestion_receipt_hash: receipt.receipt_hash.clone(),
            stage_hash: stage.stage_hash.clone(),
            report_artifact_hash: stage.stage.report_artifact_hash.clone(),
            retained_closure_artifact_hash: stage.stage.retained_closure_artifact_hash.clone(),
            attestation_bundle_artifact_hash: stage.stage.attestation_bundle_artifact_hash.clone(),
            raw_verification_hash: receipt.verification.raw_verification_hash.clone(),
            publication_request_hash: candidate.request_hash.clone(),
            publication_policy_hash: candidate.report.request.policy_hash.clone(),
        };
        let mut artifact_hashes = candidate.report.retained_artifact_hashes.clone();
        artifact_hashes.extend([
            stage.stage.report_artifact_hash.clone(),
            stage.stage.attestation_bundle_artifact_hash.clone(),
            receipt.verification.raw_verification_hash.clone(),
            receipt.receipt_hash.clone(),
        ]);
        artifact_hashes.sort_unstable();
        artifact_hashes.dedup();
        let commit = PublicationAuthorityCommit {
            subject: candidate.report.request.subject.clone(),
            outcome: candidate.report.request.outcome,
            environment_hash: candidate.report.request.environment_hash.clone(),
            binding,
            artifact_hashes,
        };
        Ok(RevalidatedPublicationAuthority {
            receipt_hash: receipt.receipt_hash,
            commit,
        })
    }

    fn revalidate_publication_authority_evidence(
        &mut self,
        evidence: &EvidenceSnapshot,
    ) -> Result<PublicationAuthorityCommit, AppError> {
        let receipt_hash = evidence
            .payload
            .publication_authority
            .as_ref()
            .map(|binding| binding.ingestion_receipt_hash.as_str())
            .ok_or_else(|| {
                publication_authority_error(
                    "MCL_PUBLICATION_AUTHORITY_INTEGRITY_FAILED",
                    "stored authority evidence has no receipt-bound publication authority",
                    "Quarantine the evidence and restore a verified receipt-bound backup.",
                )
            })?;
        let validated = self.revalidate_publication_authority_receipt(receipt_hash)?;
        let expected_payload = validated.commit.evidence_payload()?;
        let expected_hash = expected_payload.evidence_hash()?;
        if validated.receipt_hash != receipt_hash
            || evidence.payload != expected_payload
            || evidence.evidence_hash != expected_hash
        {
            return Err(publication_authority_error(
                "MCL_PUBLICATION_AUTHORITY_INTEGRITY_FAILED",
                "stored authority evidence differs from the fully revalidated publication closure",
                "Quarantine the evidence and restore the exact receipt-bound publication closure.",
            ));
        }
        Ok(validated.commit)
    }

    fn validate_publication_candidate_against_current_store(
        &mut self,
        candidate: &ValidatedPublicationCandidateDocuments,
        retained_files: &RetainedPublicationFiles,
    ) -> Result<PublicationCandidateValidationOutcome, AppError> {
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
        validate_retained_store_snapshots(&self.store, &request, retained_files)?;
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
            request_hash: candidate.request_hash.clone(),
            report_content_hash: candidate.report_content_hash.clone(),
            report_artifact_hash: candidate.report_content_hash.clone(),
            retained_closure_hash: candidate.retained_closure_hash.clone(),
            retained_closure_artifact_hash: candidate.retained_closure_hash.clone(),
            authoritative: false,
        })
    }

    fn read_staged_publication_bytes(
        &self,
        hash: &str,
        expected_size: u64,
        label: &str,
    ) -> Result<Vec<u8>, AppError> {
        let bytes = self.artifacts.read(hash).map_err(|error| {
            if error.code == "MCL_IO_ERROR" {
                publication_ingestion_error(
                    "MCL_PUBLICATION_STAGE_MEMBER_MISSING",
                    format!("staged {label} {hash} is unavailable: {}", error.message),
                    "Restore the exact staged CAS object or restage the protected candidate.",
                )
            } else {
                publication_ingestion_error(
                    "MCL_PUBLICATION_STAGE_MEMBER_MISMATCH",
                    format!(
                        "staged {label} {hash} failed CAS validation: {}",
                        error.message
                    ),
                    "Quarantine the altered stage and restage the exact protected candidate.",
                )
            }
        })?;
        if bytes.len() as u64 != expected_size {
            return Err(publication_ingestion_error(
                "MCL_PUBLICATION_STAGE_MEMBER_MISMATCH",
                format!("staged {label} size does not match its immutable registration"),
                "Quarantine the altered stage and restage the protected candidate.",
            ));
        }
        Ok(bytes)
    }

    fn read_staged_publication_members(
        &self,
        stage: &PublicationStageSnapshot,
        closure: &PublicationRetainedClosure,
    ) -> Result<RetainedPublicationFiles, AppError> {
        if stage.stage.retained_artifacts.len() != closure.artifacts.len() {
            return Err(publication_ingestion_error(
                "MCL_PUBLICATION_STAGE_MISMATCH",
                "staged member registration does not match the retained closure",
                "Quarantine the stage and restage the exact protected candidate.",
            ));
        }
        let mut bytes_by_role = BTreeMap::new();
        for (staged, retained) in stage
            .stage
            .retained_artifacts
            .iter()
            .zip(&closure.artifacts)
        {
            if staged.role != retained.role
                || staged.path != retained.path
                || staged.identity_hash != retained.identity_hash
                || staged.artifact_hash != retained.artifact_hash
            {
                return Err(publication_ingestion_error(
                    "MCL_PUBLICATION_STAGE_MISMATCH",
                    "staged member registration differs from the canonical retained closure",
                    "Quarantine the stage and restage the exact protected candidate.",
                ));
            }
            let bytes = self.read_staged_publication_bytes(
                &staged.artifact_hash,
                staged.byte_size,
                staged.role.as_str(),
            )?;
            bytes_by_role.insert(staged.role, bytes);
        }
        Ok(RetainedPublicationFiles { bytes_by_role })
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

fn publication_ingestion_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

fn publication_authority_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

fn validate_publication_authority_report(report: &PublicationReport) -> Result<(), AppError> {
    if report.classification != PublicationClassification::Passed
        || !report.clean_checkout
        || !report.dependency_closure_complete
        || !report.network_isolation_enforced
        || !report.memory_limit_enforced
        || report.authoritative
    {
        return Err(publication_authority_error(
            "MCL_PUBLICATION_AUTHORITY_REPORT_NOT_PASSED",
            "only an exact passed protected publication report can create authority",
            "Reproduce and attest a passed candidate under the committed publication policy.",
        ));
    }
    Ok(())
}

fn ensure_staged_hash(
    artifacts: &ArtifactStore,
    bytes: &[u8],
    expected_hash: &str,
    label: &str,
) -> Result<(), AppError> {
    let observed = artifacts.put(bytes)?;
    if observed != expected_hash {
        return Err(publication_ingestion_error(
            "MCL_PUBLICATION_STAGE_HASH_MISMATCH",
            format!("staged {label} hashed to {observed}, expected {expected_hash}"),
            "Quarantine the candidate and restage the exact retained bytes.",
        ));
    }
    Ok(())
}

fn validate_sigstore_bundle_shape(bundle: &Value) -> Result<(), AppError> {
    let Some(object) = bundle.as_object() else {
        return Err(publication_ingestion_error(
            "MCL_PUBLICATION_BUNDLE_INVALID",
            "Sigstore bundle must be one JSON object",
            "Stage one v0.3 Sigstore JSON bundle for the exact report.",
        ));
    };
    let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = ["dsseEnvelope", "mediaType", "verificationMaterial"]
        .into_iter()
        .collect::<BTreeSet<_>>();
    if keys != expected
        || object.get("mediaType").and_then(Value::as_str)
            != Some("application/vnd.dev.sigstore.bundle.v0.3+json")
        || object
            .get("verificationMaterial")
            .is_none_or(Value::is_null)
        || object.get("dsseEnvelope").is_none_or(Value::is_null)
    {
        return Err(publication_ingestion_error(
            "MCL_PUBLICATION_BUNDLE_INVALID",
            "Sigstore bundle does not match the closed v0.3 JSON envelope",
            "Stage the exact JSON bundle emitted by actions/attest for the candidate report.",
        ));
    }
    Ok(())
}

fn publication_attestation_arguments(
    report_path: &Path,
    bundle_path: &Path,
    report: &PublicationReport,
    policy: &crate::domain::PublicationPolicy,
) -> Vec<OsString> {
    let certificate_identity = format!(
        "https://github.com/{}/{}@{}",
        policy.repository, policy.workflow_path, policy.required_source_ref
    );
    vec![
        OsString::from("attestation"),
        OsString::from("verify"),
        report_path.as_os_str().to_owned(),
        OsString::from("--repo"),
        OsString::from(&policy.repository),
        OsString::from("--bundle"),
        bundle_path.as_os_str().to_owned(),
        OsString::from("--cert-identity"),
        OsString::from(certificate_identity),
        OsString::from("--source-ref"),
        OsString::from(&policy.required_source_ref),
        OsString::from("--source-digest"),
        OsString::from(&report.request.source_commit_sha),
        OsString::from("--signer-digest"),
        OsString::from(&report.request.source_commit_sha),
        OsString::from("--predicate-type"),
        OsString::from(&policy.attestation_predicate_type),
        OsString::from("--deny-self-hosted-runners"),
        OsString::from("--format"),
        OsString::from("json"),
    ]
}

fn validate_persisted_publication_receipt(
    receipt: &PublicationIngestionReceiptSnapshot,
    raw_verification: &[u8],
    stored_receipt: &[u8],
    bundle: &Value,
    report: &PublicationReport,
    policy: &crate::domain::PublicationPolicy,
) -> Result<(), AppError> {
    receipt.verification.validate(report, policy)?;
    let parsed = crate::publication_attestation::validate_gh_attestation_output(
        raw_verification,
        bundle,
        report,
        policy,
    )?;
    let receipt_bytes = canonical_json(&serde_json::to_value(&receipt.verification).map_err(
        |error| {
            publication_ingestion_error(
                "MCL_PUBLICATION_RECEIPT_INVALID",
                error.to_string(),
                "Quarantine the stored receipt and restore verified publication state.",
            )
        },
    )?)?;
    if parsed.verified_attestation_count != receipt.verification.verified_attestation_count
        || parsed.verified_timestamp_count != receipt.verification.verified_timestamp_count
        || raw_verification.len() as u64 != receipt.raw_verification_byte_size
        || stored_receipt != receipt_bytes
        || format!("{:x}", Sha256::digest(&receipt_bytes)) != receipt.receipt_hash
    {
        return Err(publication_ingestion_error(
            "MCL_PUBLICATION_RECEIPT_INVALID",
            "persisted publication receipt CAS closure is incomplete, altered, or disagrees with the closed verifier output",
            "Quarantine the receipt and restore the exact raw verification and canonical receipt bytes.",
        ));
    }
    Ok(())
}

fn resolve_publication_verifier(command: &str) -> Result<PathBuf, AppError> {
    if !matches!(command, "gh" | "gh.exe") {
        return Err(publication_ingestion_error(
            "MCL_PUBLICATION_VERIFIER_REJECTED",
            "publication attestation verifier command is not allowlisted",
            "Configure only gh or gh.exe; the resolved binary must also match the policy hash.",
        ));
    }
    let path = std::env::var_os("PATH").ok_or_else(|| {
        publication_ingestion_error(
            "MCL_PUBLICATION_VERIFIER_UNAVAILABLE",
            "PATH is unavailable while resolving the pinned gh verifier",
            "Install the exact pinned gh binary on the protected runner PATH.",
        )
    })?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(command);
        let Ok(metadata) = fs::metadata(&candidate) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let resolved = candidate
            .canonicalize()
            .map_err(|error| AppError::io("canonicalize publication verifier", error))?;
        if fs::metadata(&resolved).is_ok_and(|metadata| metadata.is_file()) {
            return Ok(resolved);
        }
    }
    Err(publication_ingestion_error(
        "MCL_PUBLICATION_VERIFIER_UNAVAILABLE",
        format!("allowlisted publication verifier `{command}` was not found on PATH"),
        "Install the exact policy-pinned gh binary on the protected runner PATH.",
    ))
}

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = fs::File::open(path)
        .map_err(|error| AppError::io("open publication verifier binary", error))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| AppError::io("hash publication verifier binary", error))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn verify_publication_gh_version(
    verifier_path: &Path,
    expected_version: &str,
    workspace: &Path,
) -> Result<String, AppError> {
    let capture = crate::verifier::run_bounded_external(
        verifier_path,
        &[OsString::from("--version")],
        workspace,
        Duration::from_secs(30),
        8_192,
        &[],
        "MCL_PUBLICATION_VERIFIER_LAUNCH_FAILED",
        "pinned GitHub attestation verifier",
    )?;
    let stdout = String::from_utf8(capture.stdout).map_err(|error| {
        publication_ingestion_error(
            "MCL_PUBLICATION_VERIFIER_VERSION_MISMATCH",
            format!("gh version output is not UTF-8: {error}"),
            "Install the exact policy-pinned gh release.",
        )
    })?;
    let expected_prefix = format!("gh version {expected_version} (");
    if capture.timed_out
        || capture.output_limit_exceeded
        || capture.exit_code != Some(0)
        || !capture.stderr.is_empty()
        || !stdout
            .lines()
            .next()
            .is_some_and(|line| line.starts_with(&expected_prefix))
    {
        return Err(publication_ingestion_error(
            "MCL_PUBLICATION_VERIFIER_VERSION_MISMATCH",
            format!("resolved gh verifier does not report exact version {expected_version}"),
            "Install the exact policy-pinned gh release before ingesting publication evidence.",
        ));
    }
    Ok(expected_version.to_owned())
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

fn claim_status_integrity_error(message: impl Into<String>) -> AppError {
    AppError::new(
        "MCL_CLAIM_STATUS_INTEGRITY_FAILED",
        message,
        false,
        "Quarantine the inconsistent claim-status inputs and restore verified canonical, evidence, and artifact backups.",
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
        ArtifactCreationSource, ArtifactMediaType, ArtifactRestriction, PublicationClassification,
        PublicationPolicy, PublicationRetainedArtifactRole, PublicationRetainedClosureEntry,
        PublicationRunnerEnvironment, TrustProfile,
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

    struct LocalPublicationAuthorityFixture {
        root: tempfile::TempDir,
        application: Application,
        receipt: PublicationIngestionReceiptSnapshot,
        stage: PublicationStageSnapshot,
        request: PublicationRequest,
    }

    fn verifier_report_metadata(job_id: &str, role: &str) -> ArtifactMetadata {
        ArtifactMetadata {
            schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION.to_owned(),
            media_type: ArtifactMediaType::Json,
            creation_source: ArtifactCreationSource::Verifier,
            license_expression: None,
            restriction: ArtifactRestriction::Private,
            semantic_metadata: BTreeMap::from([
                ("job_id".to_owned(), job_id.to_owned()),
                ("artifact_role".to_owned(), role.to_owned()),
            ]),
        }
    }

    fn valid_publication_attestation_output(
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
            "attestation": {"bundle": bundle, "bundle_url": "", "initiator": ""},
            "verificationResult": {
                "mediaType": "application/vnd.dev.sigstore.verificationresult+json;version=0.1",
                "signature": {"certificate": {
                    "certificateIssuer": "CN=sigstore-intermediate,O=sigstore.dev",
                    "subjectAlternativeName": identity,
                    "issuer": "https://token.actions.githubusercontent.com",
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
                    "sourceRepositoryIdentifier": policy.repository_id.to_string(),
                    "sourceRepositoryOwnerURI": format!("https://github.com/{owner}"),
                    "sourceRepositoryOwnerIdentifier": policy.repository_owner_id.to_string(),
                    "buildConfigURI": identity,
                    "buildConfigDigest": report.request.source_commit_sha,
                    "buildTrigger": "push",
                    "runInvocationURI": run_uri,
                    "sourceRepositoryVisibilityAtSigning": "public"
                }},
                "verifiedTimestamps": [{
                    "type": "Tlog",
                    "uri": "https://rekor.sigstore.dev",
                    "timestamp": "2026-07-20T04:22:41Z"
                }],
                "verifiedIdentity": {
                    "subjectAlternativeName": {"subjectAlternativeName": identity},
                    "issuer": {"issuer": "", "regexp": ".*"},
                    "runnerEnvironment": "github-hosted"
                },
                "statement": {
                    "_type": "https://in-toto.io/Statement/v1",
                    "subject": [{
                        "name": "publication-report.json",
                        "digest": {"sha256": report.report_hash(policy).expect("report hash")}
                    }],
                    "predicateType": policy.attestation_predicate_type,
                    "predicate": {
                        "buildDefinition": {
                            "buildType": "https://actions.github.io/buildtypes/workflow/v1",
                            "externalParameters": {"workflow": {
                                "path": policy.workflow_path,
                                "ref": policy.required_source_ref,
                                "repository": repository_url
                            }},
                            "internalParameters": {"github": {
                                "event_name": "push",
                                "repository_id": policy.repository_id.to_string(),
                                "repository_owner_id": policy.repository_owner_id.to_string(),
                                "runner_environment": "github-hosted"
                            }},
                            "resolvedDependencies": [{
                                "digest": {"gitCommit": report.request.source_commit_sha},
                                "uri": format!(
                                    "git+{repository_url}@{}",
                                    policy.required_source_ref
                                )
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

    fn local_publication_authority_fixture() -> LocalPublicationAuthorityFixture {
        local_publication_authority_fixture_for(PublicationOutcome::Proof)
    }

    fn local_publication_authority_fixture_for(
        outcome: PublicationOutcome,
    ) -> LocalPublicationAuthorityFixture {
        let root = tempfile::TempDir::new().expect("publication authority root");
        let config = ResolvedConfig::load(root.path(), None).expect("publication test config");
        Application::initialize(
            &config,
            "publication-authority-test",
            "publication-authority-init",
            false,
        )
        .expect("application initializes");
        let mut application = Application::open(&config).expect("application opens");

        let environment: EnvironmentManifest = serde_json::from_str(include_str!(
            "../fixtures/environment/lean-4.32-no-imports-local.json"
        ))
        .expect("no-import environment fixture");
        let environment = application
            .register_environment(
                &environment,
                "publication-authority-test",
                "publication-authority-environment",
                false,
            )
            .expect("environment registers")
            .environment
            .expect("persisted environment");

        let (declaration_name, module_bytes, informal_statement, logical_shape, polarity, theorem_type) =
            match outcome {
                PublicationOutcome::Proof => (
                    "MathOS.Publication.authorityFixture",
                    b"namespace MathOS.Publication\ntheorem authorityFixture : True := by trivial\nend MathOS.Publication\n".as_slice(),
                    "True is inhabited.",
                    "True",
                    "claim",
                    "True",
                ),
                PublicationOutcome::Refutation => (
                    "MathOS.Publication.refutationFixture",
                    b"namespace MathOS.Publication\ntheorem refutationFixture : Not False := by intro h; exact False.elim h\nend MathOS.Publication\n".as_slice(),
                    "False is inhabited.",
                    "False",
                    "negation",
                    "Not False",
                ),
            };
        let module = application
            .ingest_artifact(
                module_bytes,
                &ArtifactMetadata {
                    schema_version: crate::domain::artifact::ARTIFACT_METADATA_SCHEMA_VERSION
                        .to_owned(),
                    media_type: ArtifactMediaType::LeanSource,
                    creation_source: ArtifactCreationSource::UserIngest,
                    license_expression: None,
                    restriction: ArtifactRestriction::Private,
                    semantic_metadata: BTreeMap::from([(
                        "declaration_name".to_owned(),
                        declaration_name.to_owned(),
                    )]),
                },
                "publication-authority-test",
                "publication-authority-module",
                false,
            )
            .expect("module ingests")
            .artifact
            .expect("persisted module");
        let source = application
            .create_record(
                &RecordDraft {
                    kind: RecordKind::Source,
                    schema_version: "source/1".to_owned(),
                    payload: json!({
                        "source_type": "user_statement",
                        "title_or_label": "Publication authority fixture",
                        "authors_or_origin": ["MathOS tests"],
                        "canonical_locator": "local:publication-authority-fixture",
                        "acquisition_date": "2026-07-20",
                        "license_expression": null,
                        "redistribution_status": "unknown",
                        "content_hash": module.artifact_hash,
                        "citation_metadata": {},
                        "redaction_class": "private",
                        "provenance_notes": "local authority promotion fixture",
                        "original_text": informal_statement
                    }),
                    searchable_text: "Publication authority fixture".to_owned(),
                },
                "publication-authority-test",
                "publication-authority-source",
                false,
            )
            .expect("source creates")
            .record
            .expect("persisted source");
        let claim = application
            .create_record(
                &RecordDraft {
                    kind: RecordKind::Claim,
                    schema_version: "claim/1".to_owned(),
                    payload: json!({
                        "source_reference": {
                            "object_id": source.object_id,
                            "version_hash": source.version_hash
                        },
                        "normalized_informal_statement": informal_statement,
                        "claim_kind": "existential",
                        "logical_shape": logical_shape,
                        "assumptions": [],
                        "variables": [],
                        "concept_links": [],
                        "source_citations": [],
                        "ambiguity_notes": []
                    }),
                    searchable_text: informal_statement.to_owned(),
                },
                "publication-authority-test",
                "publication-authority-claim",
                false,
            )
            .expect("claim creates")
            .record
            .expect("persisted claim");
        let formalization = application
            .create_record(
                &RecordDraft {
                    kind: RecordKind::Formalization,
                    schema_version: "formalization/1".to_owned(),
                    payload: json!({
                        "claim_version": {
                            "object_id": claim.object_id,
                            "version_hash": claim.version_hash
                        },
                        "formal_system": "lean4",
                        "claim_polarity": polarity,
                        "environment_hash": environment.environment_hash,
                        "module_artifact_hash": module.artifact_hash,
                        "declaration_name": declaration_name,
                        "exact_theorem_type": theorem_type,
                        "declaration_hash": "d".repeat(64),
                        "import_manifest": [],
                        "formalization_notes": "local authority promotion fixture",
                        "fidelity_evidence_references": [],
                        "verification_evidence_references": []
                    }),
                    searchable_text: declaration_name.to_owned(),
                },
                "publication-authority-test",
                "publication-authority-formalization",
                false,
            )
            .expect("formalization creates")
            .record
            .expect("persisted formalization");
        let subject = ExactVersionReference {
            object_id: formalization.object_id.clone(),
            version_hash: formalization.version_hash.clone(),
        };

        let verifier_job = application
            .enqueue_verifier_job(
                &VerifierJobRequest {
                    schema_version: crate::domain::verifier::VERIFIER_REQUEST_SCHEMA_VERSION
                        .to_owned(),
                    environment_hash: environment.environment_hash.clone(),
                    module_artifact_hash: module.artifact_hash.clone(),
                    declaration_name: declaration_name.to_owned(),
                },
                0,
                "publication-authority-test",
                "publication-authority-verifier-job",
                false,
            )
            .expect("verifier job enqueues")
            .job
            .expect("persisted verifier job");
        let verifier_worker = "publication-authority-verifier-worker";
        application
            .store
            .lease_next_verifier_job(verifier_worker, 60)
            .expect("verifier job leases")
            .expect("leased verifier job");
        let running_verifier = application
            .store
            .mark_verifier_job_running(&verifier_job.job_id, verifier_worker)
            .expect("verifier job runs");
        let verifier_report = VerifierExecutionReport {
            schema_version: crate::domain::verifier::VERIFIER_EXECUTION_REPORT_SCHEMA_VERSION
                .to_owned(),
            job_id: running_verifier.job_id.clone(),
            environment_hash: environment.environment_hash.clone(),
            module_artifact_hash: module.artifact_hash.clone(),
            declaration_name: declaration_name.to_owned(),
            classification: VerifierExecutionClassification::Elaborated,
            exit_code: Some(0),
            stdout_artifact_hash: None,
            stderr_artifact_hash: None,
            duration_milliseconds: 1,
            observed_toolchain_version: Some("Lean 4.32.0".to_owned()),
            forbidden_source_token: None,
            trust_profile: TrustProfile::Local,
            memory_limit_enforced: false,
            network_isolation_enforced: false,
            authoritative: false,
        };
        let verifier_report_bytes = canonical_bytes(&verifier_report);
        let verifier_report_artifact = application
            .ensure_verifier_artifact(
                &verifier_report_bytes,
                &verifier_report_metadata(&verifier_job.job_id, "verifier_report"),
                verifier_worker,
                "publication-authority-verifier-report",
            )
            .expect("verifier report stores");
        let verifier_job = application
            .store
            .finish_verifier_job(
                &verifier_job.job_id,
                verifier_worker,
                &verifier_report_artifact.artifact_hash,
                true,
                None,
            )
            .expect("verifier job finishes");
        let diagnostic = application
            .promote_verifier_diagnostic(
                &subject.object_id,
                &subject.version_hash,
                &verifier_job.job_id,
                "publication-authority-test",
                "publication-authority-diagnostic",
                false,
            )
            .expect("diagnostic promotes")
            .evidence
            .expect("persisted diagnostic");

        let audit_job = application
            .enqueue_audit_job(
                &subject,
                &diagnostic.evidence_id,
                0,
                "publication-authority-test",
                "publication-authority-audit-job",
                false,
            )
            .expect("audit job enqueues")
            .job
            .expect("persisted audit job");
        let audit_worker = "publication-authority-audit-worker";
        application
            .store
            .lease_next_audit_job(audit_worker, 60)
            .expect("audit job leases")
            .expect("leased audit job");
        let running_audit = application
            .store
            .mark_audit_job_running(&audit_job.job_id, audit_worker)
            .expect("audit job runs");
        let audit_stdout =
            format!("'{declaration_name}' does not depend on any axioms\n").into_bytes();
        let audit_stdout_hash = application
            .register_audit_output(&running_audit, "audit_stdout", &audit_stdout, audit_worker)
            .expect("audit stdout stores")
            .expect("nonempty audit stdout");
        let audit_policy =
            crate::domain::audit::committed_audit_policy().expect("committed audit policy");
        let audit_report = LeanAuditReport {
            schema_version: crate::domain::audit::AUDIT_REPORT_SCHEMA_VERSION.to_owned(),
            job_id: running_audit.job_id.clone(),
            request_hash: running_audit.canonical_input_hash.clone(),
            subject: subject.clone(),
            diagnostic_evidence_hash: diagnostic.evidence_hash.clone(),
            environment_hash: environment.environment_hash.clone(),
            module_artifact_hash: module.artifact_hash.clone(),
            declaration_name: declaration_name.to_owned(),
            policy_hash: audit_policy.policy_hash().expect("audit policy hash"),
            classification: LeanAuditClassification::Passed,
            source_forbidden_token: None,
            observed_axioms: Vec::new(),
            unexpected_axioms: Vec::new(),
            stdout_artifact_hash: Some(audit_stdout_hash),
            stderr_artifact_hash: None,
            observed_toolchain_version: Some("Lean 4.32.0".to_owned()),
            trust_profile: TrustProfile::Local,
            dependency_closure_complete: true,
            memory_limit_enforced: false,
            network_isolation_enforced: false,
            authoritative: false,
        };
        let audit_report_bytes = canonical_bytes(&audit_report);
        let audit_report_artifact = application
            .ensure_verifier_artifact(
                &audit_report_bytes,
                &verifier_report_metadata(&audit_job.job_id, "audit_report"),
                audit_worker,
                "publication-authority-audit-report",
            )
            .expect("audit report stores");
        let audit_job = application
            .store
            .finish_audit_job(
                &audit_job.job_id,
                audit_worker,
                &audit_report_artifact.artifact_hash,
                true,
                None,
            )
            .expect("audit job finishes");
        let audit_evidence = application
            .promote_audit_evidence(
                &subject.object_id,
                &subject.version_hash,
                &audit_job.job_id,
                "publication-authority-test",
                "publication-authority-audit-evidence",
                false,
            )
            .expect("audit evidence promotes")
            .evidence
            .expect("persisted audit evidence");
        let proof_closure = audit_evidence
            .iter()
            .find(|evidence| evidence.payload.evidence_kind == EvidenceKind::ProofClosureScan)
            .expect("proof closure evidence")
            .clone();
        let axiom_audit = audit_evidence
            .iter()
            .find(|evidence| evidence.payload.evidence_kind == EvidenceKind::AxiomAudit)
            .expect("axiom audit evidence")
            .clone();

        let preparation = application
            .prepare_publication_request(
                &subject,
                outcome,
                &diagnostic.evidence_id,
                &proof_closure.evidence_id,
                &axiom_audit.evidence_id,
                &"a".repeat(40),
                &"b".repeat(40),
                "publication-authority-test",
                "publication-authority-request",
                false,
            )
            .expect("publication request prepares");
        let request = preparation.request;
        let request_hash = request.request_hash().expect("request hash");
        let request_bytes = canonical_bytes(&request);
        assert_eq!(preparation.proposed_request_hash, request_hash);

        let publication_policy = crate::domain::publication::committed_publication_policy()
            .expect("committed publication policy");
        let publication_policy_hash = publication_policy
            .policy_hash()
            .expect("publication policy hash");
        let empty = Vec::new();
        let protected_dependency_stdout = b"/opt/lib/lean/Init.olean\n".to_vec();
        let mut retained = BTreeMap::from([
            (
                PublicationRetainedArtifactRole::AuditJob,
                canonical_bytes(&audit_job),
            ),
            (
                PublicationRetainedArtifactRole::AuditPolicy,
                canonical_bytes(&audit_policy),
            ),
            (
                PublicationRetainedArtifactRole::AuditReport,
                audit_report_bytes.clone(),
            ),
            (PublicationRetainedArtifactRole::AuditStderr, empty.clone()),
            (
                PublicationRetainedArtifactRole::AuditStdout,
                audit_stdout.clone(),
            ),
            (
                PublicationRetainedArtifactRole::AxiomAuditEvidence,
                canonical_bytes(&axiom_audit),
            ),
            (
                PublicationRetainedArtifactRole::ClaimVersion,
                canonical_bytes(&claim),
            ),
            (
                PublicationRetainedArtifactRole::DiagnosticEvidence,
                canonical_bytes(&diagnostic),
            ),
            (
                PublicationRetainedArtifactRole::EnvironmentManifest,
                canonical_bytes(&environment.manifest),
            ),
            (
                PublicationRetainedArtifactRole::FormalizationVersion,
                canonical_bytes(&formalization),
            ),
            (
                PublicationRetainedArtifactRole::LeanModule,
                module_bytes.to_vec(),
            ),
            (
                PublicationRetainedArtifactRole::ProofClosureEvidence,
                canonical_bytes(&proof_closure),
            ),
            (
                PublicationRetainedArtifactRole::ProtectedAuditStderr,
                empty.clone(),
            ),
            (
                PublicationRetainedArtifactRole::ProtectedAuditStdout,
                audit_stdout.clone(),
            ),
            (
                PublicationRetainedArtifactRole::ProtectedDependencyStderr,
                empty.clone(),
            ),
            (
                PublicationRetainedArtifactRole::ProtectedDependencyStdout,
                protected_dependency_stdout,
            ),
            (
                PublicationRetainedArtifactRole::ProtectedStderr,
                empty.clone(),
            ),
            (
                PublicationRetainedArtifactRole::ProtectedStdout,
                empty.clone(),
            ),
            (
                PublicationRetainedArtifactRole::PublicationPolicy,
                canonical_bytes(&publication_policy),
            ),
            (
                PublicationRetainedArtifactRole::PublicationRequest,
                request_bytes,
            ),
            (
                PublicationRetainedArtifactRole::SourceVersion,
                canonical_bytes(&source),
            ),
            (
                PublicationRetainedArtifactRole::VerifierJob,
                canonical_bytes(&verifier_job),
            ),
            (
                PublicationRetainedArtifactRole::VerifierReport,
                verifier_report_bytes.clone(),
            ),
            (
                PublicationRetainedArtifactRole::VerifierStderr,
                empty.clone(),
            ),
            (PublicationRetainedArtifactRole::VerifierStdout, empty),
        ]);
        let entries = PublicationRetainedArtifactRole::ALL
            .into_iter()
            .map(|role| {
                let bytes = retained.get(&role).expect("retained role");
                let artifact_hash = format!("{:x}", Sha256::digest(bytes));
                let identity_hash = match role {
                    PublicationRetainedArtifactRole::AuditJob => {
                        audit_job.canonical_input_hash.clone()
                    }
                    PublicationRetainedArtifactRole::AuditPolicy => {
                        audit_policy.policy_hash().expect("audit policy hash")
                    }
                    PublicationRetainedArtifactRole::AxiomAuditEvidence => {
                        axiom_audit.evidence_hash.clone()
                    }
                    PublicationRetainedArtifactRole::ClaimVersion => claim.version_hash.clone(),
                    PublicationRetainedArtifactRole::DiagnosticEvidence => {
                        diagnostic.evidence_hash.clone()
                    }
                    PublicationRetainedArtifactRole::EnvironmentManifest => {
                        environment.environment_hash.clone()
                    }
                    PublicationRetainedArtifactRole::FormalizationVersion => {
                        formalization.version_hash.clone()
                    }
                    PublicationRetainedArtifactRole::LeanModule => module.artifact_hash.clone(),
                    PublicationRetainedArtifactRole::ProofClosureEvidence => {
                        proof_closure.evidence_hash.clone()
                    }
                    PublicationRetainedArtifactRole::PublicationPolicy => {
                        publication_policy_hash.clone()
                    }
                    PublicationRetainedArtifactRole::PublicationRequest => request_hash.clone(),
                    PublicationRetainedArtifactRole::SourceVersion => source.version_hash.clone(),
                    PublicationRetainedArtifactRole::VerifierJob => {
                        verifier_job.canonical_input_hash.clone()
                    }
                    _ => artifact_hash.clone(),
                };
                PublicationRetainedClosureEntry {
                    role,
                    path: role.expected_path().to_owned(),
                    identity_hash,
                    artifact_hash,
                }
            })
            .collect::<Vec<_>>();
        let closure = PublicationRetainedClosure {
            schema_version: crate::domain::publication::PUBLICATION_RETAINED_CLOSURE_SCHEMA_VERSION
                .to_owned(),
            subject: subject.clone(),
            request_hash: request_hash.clone(),
            artifacts: entries,
        };
        for entry in &closure.artifacts {
            let path = root.path().join(&entry.path);
            fs::create_dir_all(path.parent().expect("retained parent"))
                .expect("retained parent creates");
            fs::write(
                path,
                retained.remove(&entry.role).expect("retained member bytes"),
            )
            .expect("retained member writes");
        }
        let closure_bytes = canonical_bytes(&closure);
        let report = PublicationReport {
            schema_version: crate::domain::publication::PUBLICATION_REPORT_SCHEMA_VERSION
                .to_owned(),
            request_hash,
            request: request.clone(),
            classification: PublicationClassification::Passed,
            repository: publication_policy.repository.clone(),
            workflow_path: publication_policy.workflow_path.clone(),
            source_ref: publication_policy.required_source_ref.clone(),
            workflow_run_id: 1,
            workflow_run_attempt: 1,
            runner_environment: PublicationRunnerEnvironment::GithubHosted,
            observed_lean_toolchain: publication_policy.required_lean_toolchain.clone(),
            observed_axioms: Vec::new(),
            retained_artifact_hashes: closure
                .report_retained_artifact_hashes(&request)
                .expect("report retained hashes"),
            clean_checkout: true,
            dependency_closure_complete: true,
            network_isolation_enforced: true,
            memory_limit_enforced: true,
            authoritative: false,
        };
        let report_bytes = canonical_bytes(&report);
        let bundle = json!({
            "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json",
            "verificationMaterial": {},
            "dsseEnvelope": {}
        });
        let bundle_bytes = canonical_json(&bundle).expect("bundle canonicalizes");
        let stage = application
            .stage_publication_candidate(
                &report_bytes,
                &closure_bytes,
                root.path(),
                &bundle_bytes,
                "publication-authority-test",
                "publication-authority-stage",
                false,
            )
            .expect("publication candidate stages")
            .stage
            .expect("persisted publication stage");

        let raw_verification = canonical_json(&valid_publication_attestation_output(
            &report,
            &publication_policy,
            &bundle,
        ))
        .expect("raw attestation canonicalizes");
        let parsed = crate::publication_attestation::validate_gh_attestation_output(
            &raw_verification,
            &bundle,
            &report,
            &publication_policy,
        )
        .expect("raw attestation validates");
        let raw_verification_hash = application
            .artifacts
            .put(&raw_verification)
            .expect("raw attestation stores");
        let verification = PublicationAttestationVerification {
            schema_version:
                crate::domain::publication::PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION
                    .to_owned(),
            report_content_hash: stage.stage.report_artifact_hash.clone(),
            report_artifact_hash: stage.stage.report_artifact_hash.clone(),
            attestation_bundle_hash: stage.stage.attestation_bundle_artifact_hash.clone(),
            raw_verification_hash,
            verifier_name: "gh".to_owned(),
            verifier_version: publication_policy.attestation_verifier_version.clone(),
            verifier_binary_sha256: publication_policy
                .attestation_verifier_binary_sha256
                .clone(),
            repository: publication_policy.repository.clone(),
            signer_workflow: format!(
                "{}/{}",
                publication_policy.repository, publication_policy.workflow_path
            ),
            certificate_identity: format!(
                "https://github.com/{}/{}@{}",
                publication_policy.repository,
                publication_policy.workflow_path,
                publication_policy.required_source_ref
            ),
            source_ref: publication_policy.required_source_ref.clone(),
            source_commit_sha: request.source_commit_sha.clone(),
            predicate_type: publication_policy.attestation_predicate_type.clone(),
            self_hosted_runners_denied: true,
            verified_attestation_count: parsed.verified_attestation_count,
            verified_timestamp_count: parsed.verified_timestamp_count,
            authoritative: false,
        };
        let receipt_bytes = canonical_bytes(&verification);
        let receipt_hash = application
            .artifacts
            .put(&receipt_bytes)
            .expect("attestation receipt stores");
        let receipt = application
            .store
            .register_publication_ingestion_receipt(
                &stage.stage_hash,
                &subject,
                &verification,
                raw_verification.len() as u64,
                receipt_bytes.len() as u64,
                "publication-authority-test",
                "publication-authority-receipt",
            )
            .expect("publication receipt registers");
        assert_eq!(receipt.receipt_hash, receipt_hash);

        LocalPublicationAuthorityFixture {
            root,
            application,
            receipt,
            stage,
            request,
        }
    }

    fn fixture_cas_path(root: &Path, hash: &str) -> PathBuf {
        root.join(".mcl")
            .join("artifacts")
            .join("sha256")
            .join(&hash[..2])
            .join(&hash[2..4])
            .join(hash)
    }

    fn fixture_fidelity_lineage(
        fixture: &LocalPublicationAuthorityFixture,
    ) -> (
        ExactVersionReference,
        ExactVersionReference,
        FormalizationPayload,
    ) {
        let formalization = fixture
            .application
            .store
            .get_record_version(&fixture.request.subject.version_hash)
            .expect("fixture formalization reads");
        let formal_payload: FormalizationPayload =
            serde_json::from_value(formalization.payload).expect("fixture formalization decodes");
        let claim = fixture
            .application
            .store
            .get_record_version(&formal_payload.claim_version.version_hash)
            .expect("fixture claim reads");
        let claim_payload: ClaimPayload =
            serde_json::from_value(claim.payload).expect("fixture claim decodes");
        (
            claim_payload.source_reference,
            formal_payload.claim_version.clone(),
            formal_payload,
        )
    }

    fn add_fixture_verified_fidelity(
        fixture: &mut LocalPublicationAuthorityFixture,
        relation: Option<crate::domain::ReviewedSourceRelation>,
        key_suffix: &str,
    ) -> Result<FidelityReviewOutcome, AppError> {
        let (source, claim, formal_payload) = fixture_fidelity_lineage(fixture);
        let reviewer = "independent-fidelity-reviewer";
        let run = fixture
            .application
            .create_run(
                RunKind::LiteratureReview,
                &json!({}),
                reviewer,
                &format!("claim-status-fidelity-run-{key_suffix}"),
                false,
            )?
            .run
            .expect("persisted fidelity run");
        let supersedes_evidence_id = fixture
            .application
            .fidelity_status(&fixture.request.subject)?
            .head_evidence_id;
        let request = match relation {
            None => crate::domain::VersionedFidelityReviewRequest::V1(
                crate::domain::FidelityReviewRequest {
                    schema_version: crate::domain::fidelity::FIDELITY_REVIEW_REQUEST_SCHEMA_VERSION
                        .to_owned(),
                    source,
                    claim,
                    formalization: fixture.request.subject.clone(),
                    review_level: crate::domain::FidelityReviewLevel::MathematicalStatement,
                    verdict: FidelityVerdict::Verified,
                    reviewer_identity: reviewer.to_owned(),
                    findings: vec![
                        "The exact Lean declaration states the reviewed source proposition."
                            .to_owned(),
                    ],
                    ambiguity_disposition: crate::domain::AmbiguityDisposition::NoAmbiguity,
                    definition_mappings: Vec::new(),
                    supporting_artifact_hashes: vec![formal_payload.module_artifact_hash],
                    producing_run_id: run.run_id,
                    supersedes_evidence_id,
                },
            ),
            Some(reviewed_source_relation) => crate::domain::VersionedFidelityReviewRequest::V2(
                crate::domain::FidelityReviewRequestV2 {
                    schema_version:
                        crate::domain::fidelity::FIDELITY_REVIEW_REQUEST_V2_SCHEMA_VERSION
                            .to_owned(),
                    source,
                    claim,
                    formalization: fixture.request.subject.clone(),
                    reviewed_source_relation,
                    review_level: crate::domain::FidelityReviewLevel::MathematicalStatement,
                    verdict: FidelityVerdict::Verified,
                    reviewer_identity: reviewer.to_owned(),
                    findings: vec![
                        "The exact Lean declaration states the reviewed source relation."
                            .to_owned(),
                    ],
                    ambiguity_disposition: crate::domain::AmbiguityDisposition::NoAmbiguity,
                    definition_mappings: Vec::new(),
                    supporting_artifact_hashes: vec![formal_payload.module_artifact_hash],
                    producing_run_id: run.run_id,
                    supersedes_evidence_id,
                },
            ),
        };
        fixture.application.review_fidelity(
            &request,
            reviewer,
            &format!("claim-status-fidelity-review-{key_suffix}"),
            false,
        )
    }

    #[test]
    fn claim_status_requires_both_revalidated_fidelity_and_authority_and_survives_restart()
    -> Result<(), AppError> {
        use crate::domain::{
            ClaimResearchStatusNonqualificationReason, ClaimResearchStatusWitnessKind,
            ResearchStatus,
        };

        let mut fixture = local_publication_authority_fixture();
        let (_, claim, _) = fixture_fidelity_lineage(&fixture);
        let authority = fixture
            .application
            .promote_publication_authority(
                &fixture.receipt.receipt_hash,
                "publication-authority-test",
                "claim-status-proof-authority",
                false,
            )?
            .evidence
            .expect("persisted proof authority");

        let before_fidelity = fixture.application.claim_research_status(&claim)?;
        assert_eq!(before_fidelity.status, ResearchStatus::Open);
        assert!(before_fidelity.witnesses.is_empty());
        assert_eq!(before_fidelity.nonqualifications.len(), 1);
        assert_eq!(
            before_fidelity.nonqualifications[0].reason,
            ClaimResearchStatusNonqualificationReason::NoCurrentVerifiedFidelity
        );

        let fidelity = add_fixture_verified_fidelity(&mut fixture, None, "proof-v1")?;
        let fidelity_evidence = fidelity.evidence.expect("persisted fidelity evidence");
        let proved = fixture.application.claim_research_status(&claim)?;
        assert_eq!(proved.status, ResearchStatus::Proved);
        assert!(proved.nonqualifications.is_empty());
        let [witness] = proved.witnesses.as_slice() else {
            panic!("one proof witness expected");
        };
        assert_eq!(witness.kind, ClaimResearchStatusWitnessKind::Proof);
        assert_eq!(witness.fidelity_evidence_id, fidelity_evidence.evidence_id);
        assert_eq!(witness.authority_evidence_id, authority.evidence_id);
        assert_eq!(
            witness.publication_receipt_hash,
            fixture.receipt.receipt_hash
        );

        let config = ResolvedConfig::load(fixture.root.path(), None)?;
        drop(fixture.application);
        let mut reopened = Application::open(&config)?;
        assert_eq!(reopened.claim_research_status(&claim)?, proved);

        let current_formalization = reopened.get_record(
            &fixture.request.subject.object_id,
            Some(&fixture.request.subject.version_hash),
        )?;
        let mut successor_formalization_payload = current_formalization.payload.clone();
        successor_formalization_payload["formalization_notes"] =
            Value::String("new exact formalization version".to_owned());
        reopened.version_record(
            &current_formalization.object_id,
            &current_formalization.version_hash,
            &RecordDraft {
                kind: RecordKind::Formalization,
                schema_version: current_formalization.schema_version,
                payload: successor_formalization_payload,
                searchable_text: "successor authority fixture".to_owned(),
            },
            "publication-authority-test",
            "claim-status-successor-formalization",
            false,
        )?;
        let changed_formalization = reopened.claim_research_status(&claim)?;
        assert_eq!(changed_formalization.status, ResearchStatus::Open);
        assert!(changed_formalization.witnesses.is_empty());
        assert_eq!(
            changed_formalization.nonqualifications[0].reason,
            ClaimResearchStatusNonqualificationReason::NoCurrentVerifiedFidelity
        );

        let current_claim = reopened.get_record(&claim.object_id, Some(&claim.version_hash))?;
        let mut successor_claim_payload = current_claim.payload.clone();
        successor_claim_payload["normalized_informal_statement"] =
            Value::String("True remains inhabited under a revised exact claim.".to_owned());
        reopened.version_record(
            &current_claim.object_id,
            &current_claim.version_hash,
            &RecordDraft {
                kind: RecordKind::Claim,
                schema_version: current_claim.schema_version,
                payload: successor_claim_payload,
                searchable_text: "revised exact claim".to_owned(),
            },
            "publication-authority-test",
            "claim-status-successor-claim",
            false,
        )?;
        let superseded = reopened.claim_research_status(&claim)?;
        assert_eq!(superseded.status, ResearchStatus::Superseded);
        assert!(superseded.witnesses.is_empty());
        assert!(superseded.nonqualifications.is_empty());
        Ok(())
    }

    #[test]
    fn claim_status_needs_v2_logical_negation_fidelity_before_disproof() -> Result<(), AppError> {
        use crate::domain::{
            ClaimResearchStatusNonqualificationReason, ClaimResearchStatusWitnessKind,
            ResearchStatus, ReviewedSourceRelation,
        };

        let mut fixture = local_publication_authority_fixture_for(PublicationOutcome::Refutation);
        let (_, claim, _) = fixture_fidelity_lineage(&fixture);
        let authority = fixture
            .application
            .promote_publication_authority(
                &fixture.receipt.receipt_hash,
                "publication-authority-test",
                "claim-status-refutation-authority",
                false,
            )?
            .evidence
            .expect("persisted refutation authority");
        assert_eq!(
            authority.payload.evidence_kind,
            EvidenceKind::LeanKernelRefutation
        );

        let v1 = add_fixture_verified_fidelity(&mut fixture, None, "refutation-v1")?;
        let v1_evidence = v1.evidence.expect("persisted v1 fidelity");
        let unbound = fixture.application.claim_research_status(&claim)?;
        assert_eq!(unbound.status, ResearchStatus::Open);
        assert!(unbound.witnesses.is_empty());
        assert_eq!(unbound.nonqualifications.len(), 1);
        assert_eq!(
            unbound.nonqualifications[0].reason,
            ClaimResearchStatusNonqualificationReason::FidelityRelationUnbound
        );
        assert_eq!(
            unbound.nonqualifications[0].fidelity_evidence_id.as_deref(),
            Some(v1_evidence.evidence_id.as_str())
        );

        let mismatch = add_fixture_verified_fidelity(
            &mut fixture,
            Some(ReviewedSourceRelation::Claim),
            "refutation-v2-mismatch",
        )
        .expect_err("claim relation cannot review a negation formalization");
        assert_eq!(mismatch.code, "MCL_FIDELITY_RELATION_MISMATCH");

        let v2 = add_fixture_verified_fidelity(
            &mut fixture,
            Some(ReviewedSourceRelation::LogicalNegation),
            "refutation-v2",
        )?;
        let v2_evidence = v2.evidence.expect("persisted v2 fidelity");
        let disproved = fixture.application.claim_research_status(&claim)?;
        assert_eq!(disproved.status, ResearchStatus::Disproved);
        assert!(disproved.nonqualifications.is_empty());
        let [witness] = disproved.witnesses.as_slice() else {
            panic!("one refutation witness expected");
        };
        assert_eq!(witness.kind, ClaimResearchStatusWitnessKind::Refutation);
        assert_eq!(
            witness.reviewed_source_relation,
            ReviewedSourceRelation::LogicalNegation
        );
        assert_eq!(witness.fidelity_evidence_id, v2_evidence.evidence_id);
        assert_eq!(witness.authority_evidence_id, authority.evidence_id);
        Ok(())
    }

    #[test]
    fn claim_status_fails_closed_when_a_current_fidelity_cas_artifact_is_missing()
    -> Result<(), AppError> {
        use crate::domain::ResearchStatus;

        let mut fixture = local_publication_authority_fixture();
        let (_, claim, _) = fixture_fidelity_lineage(&fixture);
        fixture.application.promote_publication_authority(
            &fixture.receipt.receipt_hash,
            "publication-authority-test",
            "claim-status-missing-cas-authority",
            false,
        )?;
        let fidelity = add_fixture_verified_fidelity(&mut fixture, None, "missing-cas-proof-v1")?;
        assert_eq!(
            fixture.application.claim_research_status(&claim)?.status,
            ResearchStatus::Proved
        );

        let report_path =
            fixture_cas_path(fixture.root.path(), &fidelity.proposed_report_artifact_hash);
        fs::remove_file(report_path).expect("test removes exact fidelity report CAS object");
        let error = fixture
            .application
            .claim_research_status(&claim)
            .expect_err("missing current fidelity report must fail closed");
        assert_ne!(error.code, "MCL_CLAIM_STATUS_READ_CONFLICT");
        assert!(!error.retryable);
        Ok(())
    }

    #[test]
    fn publication_authority_promotion_is_dry_run_idempotent_and_fails_closed() {
        let mut fixture = local_publication_authority_fixture();
        let receipt_hash = fixture.receipt.receipt_hash.clone();
        let dry_run = fixture
            .application
            .promote_publication_authority(
                &receipt_hash,
                "publication-authority-test",
                "publication-authority-promotion",
                true,
            )
            .expect("publication authority dry run");
        assert!(dry_run.dry_run);
        assert!(dry_run.evidence.is_none());
        assert_eq!(dry_run.evidence_kind, EvidenceKind::LeanKernelProof);

        let persisted = fixture
            .application
            .promote_publication_authority(
                &receipt_hash,
                "publication-authority-test",
                "publication-authority-promotion",
                false,
            )
            .expect("publication authority persists");
        assert!(!persisted.dry_run);
        assert_eq!(
            persisted.proposed_evidence_hash,
            dry_run.proposed_evidence_hash
        );
        let evidence = persisted.evidence.as_ref().expect("authoritative evidence");
        assert_eq!(evidence.evidence_hash, dry_run.proposed_evidence_hash);
        assert_eq!(
            evidence.payload.schema_version,
            crate::domain::evidence::AUTHORITATIVE_EVIDENCE_SCHEMA_VERSION
        );
        assert_eq!(
            evidence.payload.evidence_kind,
            EvidenceKind::LeanKernelProof
        );
        assert_eq!(evidence.payload.result, EvidenceResult::Accepted);
        assert_eq!(
            evidence.payload.authority_class,
            EvidenceAuthorityClass::Authoritative
        );
        assert!(evidence.payload.producing_run_id.is_none());
        assert!(evidence.payload.producing_job_id.is_none());
        let binding = evidence
            .payload
            .publication_authority
            .as_ref()
            .expect("receipt-bound authority");
        assert_eq!(
            binding.schema_version,
            crate::domain::evidence::PUBLICATION_AUTHORITY_BINDING_SCHEMA_VERSION
        );
        assert_eq!(binding.ingestion_receipt_hash, receipt_hash);
        assert_eq!(binding.stage_hash, fixture.stage.stage_hash);
        assert_eq!(
            binding.report_artifact_hash,
            fixture.stage.stage.report_artifact_hash
        );
        assert_eq!(
            binding.retained_closure_artifact_hash,
            fixture.stage.stage.retained_closure_artifact_hash
        );
        assert_eq!(
            binding.attestation_bundle_artifact_hash,
            fixture.stage.stage.attestation_bundle_artifact_hash
        );
        assert_eq!(
            binding.raw_verification_hash,
            fixture.receipt.verification.raw_verification_hash
        );
        assert_eq!(
            binding.publication_request_hash,
            fixture.request.request_hash().expect("request hash")
        );
        assert_eq!(binding.publication_policy_hash, fixture.request.policy_hash);

        let retry = fixture
            .application
            .promote_publication_authority(
                &receipt_hash,
                "publication-authority-test",
                "publication-authority-promotion",
                false,
            )
            .expect("identical authority retry");
        assert_eq!(
            serde_json::to_value(&retry).expect("retry serializes"),
            serde_json::to_value(&persisted).expect("promotion serializes")
        );

        let missing = fixture.stage.stage.attestation_bundle_artifact_hash.clone();
        fs::remove_file(fixture_cas_path(fixture.root.path(), &missing))
            .expect("staged CAS member removes");
        assert_eq!(
            fixture
                .application
                .promote_publication_authority(
                    &receipt_hash,
                    "publication-authority-test",
                    "publication-authority-missing-cas",
                    false,
                )
                .expect_err("missing staged CAS member must fail")
                .code,
            "MCL_PUBLICATION_STAGE_MEMBER_MISSING"
        );
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
    fn publication_authority_requires_an_explicit_passed_report() {
        let (report, _) = candidate_documents();
        validate_publication_authority_report(&report).expect("passed protected report");

        let mut rejected_reports = Vec::new();
        for classification in [
            PublicationClassification::Rejected,
            PublicationClassification::Failed,
        ] {
            let mut rejected = report.clone();
            rejected.classification = classification;
            rejected_reports.push(("classification", rejected));
        }

        let mut rejected = report.clone();
        rejected.clean_checkout = false;
        rejected_reports.push(("clean checkout", rejected));

        let mut rejected = report.clone();
        rejected.dependency_closure_complete = false;
        rejected_reports.push(("dependency closure", rejected));

        let mut rejected = report.clone();
        rejected.network_isolation_enforced = false;
        rejected_reports.push(("network isolation", rejected));

        let mut rejected = report.clone();
        rejected.memory_limit_enforced = false;
        rejected_reports.push(("memory limit", rejected));

        let mut rejected = report.clone();
        rejected.authoritative = true;
        rejected_reports.push(("pre-existing authority", rejected));

        for (failed_condition, rejected) in rejected_reports {
            assert_eq!(
                validate_publication_authority_report(&rejected)
                    .expect_err("failed condition cannot cross the authority gate")
                    .code,
                "MCL_PUBLICATION_AUTHORITY_REPORT_NOT_PASSED",
                "{failed_condition} must fail closed"
            );
        }
    }

    #[test]
    fn publication_attestation_arguments_are_closed_and_request_bound() {
        let (report, _) = candidate_documents();
        let policy = crate::domain::publication::committed_publication_policy()
            .expect("committed publication policy");
        let report_path = Path::new("isolated/publication-report.json");
        let bundle_path = Path::new("isolated/attestation.json");

        assert_eq!(
            publication_attestation_arguments(report_path, bundle_path, &report, &policy),
            vec![
                OsString::from("attestation"),
                OsString::from("verify"),
                report_path.as_os_str().to_owned(),
                OsString::from("--repo"),
                OsString::from("Mnehmos/MathOS"),
                OsString::from("--bundle"),
                bundle_path.as_os_str().to_owned(),
                OsString::from("--cert-identity"),
                OsString::from(
                    "https://github.com/Mnehmos/MathOS/.github/workflows/publication.yml@refs/heads/main"
                ),
                OsString::from("--source-ref"),
                OsString::from("refs/heads/main"),
                OsString::from("--source-digest"),
                OsString::from("1".repeat(40)),
                OsString::from("--signer-digest"),
                OsString::from("1".repeat(40)),
                OsString::from("--predicate-type"),
                OsString::from("https://slsa.dev/provenance/v1"),
                OsString::from("--deny-self-hosted-runners"),
                OsString::from("--format"),
                OsString::from("json"),
            ]
        );
    }

    #[test]
    fn persisted_publication_receipt_retry_reparses_raw_verifier_output() {
        let (report, _) = candidate_documents();
        let policy = crate::domain::publication::committed_publication_policy()
            .expect("committed publication policy");
        let bundle = json!({
            "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json",
            "verificationMaterial": {},
            "dsseEnvelope": {},
        });
        let bundle_bytes = canonical_json(&bundle).expect("bundle canonicalizes");
        let raw_verification = b"{}";
        let report_hash = report.report_hash(&policy).expect("report hash");
        let verification = PublicationAttestationVerification {
            schema_version:
                crate::domain::publication::PUBLICATION_ATTESTATION_VERIFICATION_SCHEMA_VERSION
                    .to_owned(),
            report_content_hash: report_hash.clone(),
            report_artifact_hash: report_hash,
            attestation_bundle_hash: format!("{:x}", Sha256::digest(&bundle_bytes)),
            raw_verification_hash: format!("{:x}", Sha256::digest(raw_verification)),
            verifier_name: "gh".to_owned(),
            verifier_version: policy.attestation_verifier_version.clone(),
            verifier_binary_sha256: policy.attestation_verifier_binary_sha256.clone(),
            repository: policy.repository.clone(),
            signer_workflow: format!("{}/{}", policy.repository, policy.workflow_path),
            certificate_identity: format!(
                "https://github.com/{}/{}@{}",
                policy.repository, policy.workflow_path, policy.required_source_ref
            ),
            source_ref: policy.required_source_ref.clone(),
            source_commit_sha: report.request.source_commit_sha.clone(),
            predicate_type: policy.attestation_predicate_type.clone(),
            self_hosted_runners_denied: true,
            verified_attestation_count: 1,
            verified_timestamp_count: 1,
            authoritative: false,
        };
        verification
            .validate(&report, &policy)
            .expect("synthetic receipt shape is valid before raw replay");
        let receipt_bytes =
            canonical_json(&serde_json::to_value(&verification).expect("verification serializes"))
                .expect("verification canonicalizes");
        let receipt = PublicationIngestionReceiptSnapshot {
            receipt_hash: format!("{:x}", Sha256::digest(&receipt_bytes)),
            stage_hash: "9".repeat(64),
            verification,
            raw_verification_byte_size: raw_verification.len() as u64,
            receipt_byte_size: receipt_bytes.len() as u64,
            created_at: 1,
            created_by: "publication-test".to_owned(),
        };

        assert_eq!(
            validate_persisted_publication_receipt(
                &receipt,
                raw_verification,
                &receipt_bytes,
                &bundle,
                &report,
                &policy,
            )
            .expect_err("retry must not trust the shaped receipt without replaying raw output")
            .code,
            "MCL_PUBLICATION_ATTESTATION_INVALID"
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
