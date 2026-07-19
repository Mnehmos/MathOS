use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::artifacts::ArtifactStore;
use crate::canonical::canonical_json;
use crate::config::ResolvedConfig;
use crate::domain::{
    ArtifactMetadata, ArtifactSnapshot, EdgeDraft, EdgeSnapshot, EnvironmentManifest,
    EnvironmentSnapshot, GraphTraversalHit, GraphTraversalRequest, RecordDraft, RecordSnapshot,
    RunChainReport, RunEventDraft, RunEventSnapshot, RunKind, RunSnapshot,
    VerifierExecutionClassification, VerifierExecutionReport, VerifierJobRequest,
    VerifierJobSnapshot,
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
            schema_version: "verifier_execution_report/1".to_owned(),
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
            Ok(existing) if existing.media_type == metadata.media_type => Ok(existing),
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
