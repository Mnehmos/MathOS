use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::canonical::{canonical_json, record_version_hash};
use crate::domain::schemas::{
    ExactVersionReference, FormalizationPayload, validate_record_payload,
};
use crate::domain::{
    COMPARATOR_FORMALIZATION_SCHEMA_VERSION, COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION,
    ComparatorFileBinding, ComparatorPackagePlan, ComparatorPackageStatus,
    ComparatorPackageVerification, ComparatorPublicationStatus, ComparatorSourceMemberBinding,
    EnvironmentSnapshot, MAX_COMPARATOR_FILE_BYTES, RecordKind, RecordSnapshot, ReleaseMember,
};
use crate::error::AppError;
use crate::release::{ReleaseIntegrity, verify_release_bundle_integrity};

const PLAN_MAX_BYTES: u64 = 2 * 1_048_576;
const MAX_PACKAGE_TREE_ENTRIES: usize = 5;
const PACKAGE_PATHS: [&str; 5] = [
    "Challenge.lean",
    "Solution.lean",
    "config.json",
    "formalization.yaml",
    "verification.json",
];

#[derive(Clone, Debug)]
pub struct ComparatorExportRequest<'a> {
    pub plan_path: &'a Path,
    pub bundle_dir: &'a Path,
    pub expected_release_manifest_hash: &'a str,
    pub output_dir: &'a Path,
    pub dry_run: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComparatorExportOutcome {
    pub dry_run: bool,
    pub package_path: PathBuf,
    pub verification_hash: String,
    pub input_fingerprint: String,
    pub source_release_manifest_hash: String,
    pub source_formalization: ExactVersionReference,
    pub status: ComparatorPackageStatus,
    pub comparator_verified: bool,
    pub authoritative: bool,
    pub member_count: usize,
    pub total_member_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComparatorPackageVerificationReport {
    pub verification_hash: String,
    pub input_fingerprint: String,
    pub source_release_manifest_hash: String,
    pub source_formalization: ExactVersionReference,
    pub status: ComparatorPackageStatus,
    pub comparator_verified: bool,
    pub authoritative: bool,
    pub member_count: usize,
    pub total_member_bytes: u64,
    pub database_independent: bool,
    pub inventory_verified: bool,
    pub hashes_verified: bool,
    pub bindings_verified: bool,
    pub deterministic_reprojection: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ComparatorConfig {
    challenge_module: String,
    solution_module: String,
    theorem_names: Vec<String>,
    permitted_axioms: Vec<String>,
    enable_nanoda: bool,
}

#[derive(Clone, Debug)]
struct ComparatorProjection {
    files: BTreeMap<String, Vec<u8>>,
    verification: ComparatorPackageVerification,
    verification_hash: String,
}

#[derive(Clone, Debug)]
struct ComparatorPackageIntegrity {
    files: BTreeMap<String, Vec<u8>>,
    verification: ComparatorPackageVerification,
    verification_hash: String,
}

pub fn export_comparator(
    request: ComparatorExportRequest<'_>,
) -> Result<ComparatorExportOutcome, AppError> {
    let (plan, plan_hash) = read_plan(request.plan_path)?;
    let release = load_verified_release(
        request.bundle_dir,
        request.expected_release_manifest_hash,
        &plan,
    )?;
    let projection = project_comparator(&release, &plan, &plan_hash)?;
    let (parent, destination) = resolve_new_output(request.output_dir)?;
    let total_member_bytes = projection
        .files
        .values()
        .map(|bytes| bytes.len() as u64)
        .sum();
    let outcome = ComparatorExportOutcome {
        dry_run: request.dry_run,
        package_path: destination.clone(),
        verification_hash: projection.verification_hash.clone(),
        input_fingerprint: projection.verification.input_fingerprint.clone(),
        source_release_manifest_hash: release.manifest_hash.clone(),
        source_formalization: projection.verification.source_formalization.clone(),
        status: ComparatorPackageStatus::Ready,
        comparator_verified: false,
        authoritative: false,
        member_count: projection.files.len(),
        total_member_bytes,
    };
    if request.dry_run {
        return Ok(outcome);
    }

    let temporary = tempfile::Builder::new()
        .prefix(".mcl-comparator-")
        .tempdir_in(&parent)
        .map_err(|error| AppError::io("create Comparator package staging directory", error))?;
    for (path, bytes) in &projection.files {
        write_new_member(temporary.path(), path, bytes)?;
    }
    let staged = verify_package_integrity(temporary.path())?;
    if staged.verification_hash != projection.verification_hash || staged.files != projection.files
    {
        return Err(comparator_error(
            "MCL_COMPARATOR_PACKAGE_MEMBER_MISMATCH",
            "staged Comparator package changed before publication",
            "Quarantine the staging directory and retry from unchanged canonical inputs.",
        ));
    }
    fs::rename(temporary.path(), &destination)
        .map_err(|error| AppError::io("atomically publish Comparator package", error))?;
    Ok(outcome)
}

pub fn verify_comparator_package(
    package_dir: &Path,
    expected_verification_hash: &str,
    plan_path: &Path,
    bundle_dir: &Path,
    expected_release_manifest_hash: &str,
) -> Result<ComparatorPackageVerificationReport, AppError> {
    require_hash(
        expected_verification_hash,
        "expected Comparator verification",
        "MCL_COMPARATOR_EXPECTED_HASH_INVALID",
    )?;
    let observed = verify_package_integrity(package_dir)?;
    if observed.verification_hash != expected_verification_hash {
        return Err(comparator_error(
            "MCL_COMPARATOR_VERIFICATION_HASH_MISMATCH",
            format!(
                "Comparator verification hash {} differs from trusted expected hash {expected_verification_hash}",
                observed.verification_hash
            ),
            "Use the exact verification hash emitted by the trusted package publication channel.",
        ));
    }

    let (plan, plan_hash) = read_plan(plan_path)?;
    let release = load_verified_release(bundle_dir, expected_release_manifest_hash, &plan)?;
    let expected = project_comparator(&release, &plan, &plan_hash)?;
    if expected.verification_hash != observed.verification_hash
        || expected.verification != observed.verification
        || expected.files != observed.files
    {
        return Err(comparator_error(
            "MCL_COMPARATOR_REPROJECTION_MISMATCH",
            "Comparator package does not reproduce byte-for-byte from the canonical plan and frozen release",
            "Quarantine the package and rebuild it from the exact plan and release.",
        ));
    }

    Ok(ComparatorPackageVerificationReport {
        verification_hash: observed.verification_hash,
        input_fingerprint: observed.verification.input_fingerprint,
        source_release_manifest_hash: observed.verification.source_release_manifest_hash,
        source_formalization: observed.verification.source_formalization,
        status: ComparatorPackageStatus::Ready,
        comparator_verified: false,
        authoritative: false,
        member_count: observed.files.len(),
        total_member_bytes: observed
            .files
            .values()
            .map(|bytes| bytes.len() as u64)
            .sum(),
        database_independent: true,
        inventory_verified: true,
        hashes_verified: true,
        bindings_verified: true,
        deterministic_reprojection: true,
    })
}

fn load_verified_release(
    bundle_dir: &Path,
    expected_manifest_hash: &str,
    plan: &ComparatorPackagePlan,
) -> Result<ReleaseIntegrity, AppError> {
    require_hash(
        expected_manifest_hash,
        "expected source release manifest",
        "MCL_COMPARATOR_EXPECTED_HASH_INVALID",
    )?;
    if plan.source_release_manifest_hash != expected_manifest_hash {
        return Err(binding_error(
            "Comparator plan source release differs from the trusted expected release identity",
        ));
    }
    let release = verify_release_bundle_integrity(bundle_dir)?;
    if release.manifest_hash != expected_manifest_hash {
        return Err(comparator_error(
            "MCL_COMPARATOR_SOURCE_RELEASE_HASH_MISMATCH",
            format!(
                "source release manifest {} differs from trusted expected hash {expected_manifest_hash}",
                release.manifest_hash
            ),
            "Use the exact frozen release selected by the trusted publication channel.",
        ));
    }
    Ok(release)
}

fn project_comparator(
    release: &ReleaseIntegrity,
    plan: &ComparatorPackagePlan,
    plan_hash: &str,
) -> Result<ComparatorProjection, AppError> {
    plan.validate()?;
    if release.manifest_hash != plan.source_release_manifest_hash
        || release.manifest.publication.subject != plan.formalization
    {
        return Err(binding_error(
            "Comparator plan does not select the frozen release headline formalization",
        ));
    }

    let formalization_path = format!(
        "objects/formalization/{}@{}.json",
        plan.formalization.object_id, plan.formalization.version_hash
    );
    let formalization_bytes = required_source(release, &formalization_path)?;
    let formalization_member = required_member(release, &formalization_path)?;
    let formalization_record: RecordSnapshot =
        decode_canonical(formalization_bytes, "source formalization record")?;
    if formalization_record.kind != RecordKind::Formalization
        || formalization_record.object_id != plan.formalization.object_id
        || formalization_record.version_hash != plan.formalization.version_hash
        || record_version_hash(
            &formalization_record.schema_version,
            &formalization_record.payload,
        )? != formalization_record.version_hash
    {
        return Err(binding_error(
            "selected formalization member does not reproduce its exact canonical identity",
        ));
    }
    validate_record_payload(
        RecordKind::Formalization,
        &formalization_record.schema_version,
        &formalization_record.payload,
    )?;
    let formalization: FormalizationPayload =
        serde_json::from_value(formalization_record.payload.clone()).map_err(|error| {
            comparator_error(
                "MCL_COMPARATOR_SOURCE_FORMALIZATION_INVALID",
                format!("selected formalization cannot be decoded: {error}"),
                "Restore the exact canonical formalization member.",
            )
        })?;

    let publication = &release.manifest.publication;
    if formalization.environment_hash != publication.environment_hash
        || formalization.module_artifact_hash != publication.module_artifact_hash
        || formalization.declaration_name != publication.declaration_name
        || release.manifest.replay.declaration_name != formalization.declaration_name
        || plan.theorem_names != [formalization.declaration_name.clone()]
    {
        return Err(binding_error(
            "Comparator plan, formalization, replay, and publication headline do not identify one exact theorem",
        ));
    }

    let solution_path = release.manifest.replay.module_path.clone();
    let solution = required_source(release, &solution_path)?.to_vec();
    let solution_member = required_member(release, &solution_path)?;
    if solution_member.content_hash != publication.module_artifact_hash
        || sha256(&solution) != publication.module_artifact_hash
    {
        return Err(binding_error(
            "frozen replay theorem source differs from the publication module artifact",
        ));
    }

    let dependency_path = release.manifest.replay.environment_path.clone();
    let dependency_bytes = required_source(release, &dependency_path)?;
    let dependency_member = required_member(release, &dependency_path)?;
    let environment: EnvironmentSnapshot =
        decode_canonical(dependency_bytes, "source dependency manifest")?;
    environment.manifest.validate()?;
    if environment.environment_hash != publication.environment_hash
        || formalization.import_manifest != environment.manifest.import_manifest
    {
        return Err(binding_error(
            "frozen dependency manifest differs from the selected formalization environment",
        ));
    }

    let config = ComparatorConfig {
        challenge_module: "Challenge".to_owned(),
        solution_module: "Solution".to_owned(),
        theorem_names: plan.theorem_names.clone(),
        permitted_axioms: plan.permitted_axioms.clone(),
        enable_nanoda: plan.enable_nanoda,
    };
    config.validate()?;
    let config_bytes = canonical_bytes(&config, "Comparator config")?;
    let challenge = plan.challenge_source.as_bytes().to_vec();
    let formalization_yaml =
        render_formalization_yaml(plan, plan_hash, release, &formalization, &environment)?;

    let challenge_binding = file_binding("Challenge.lean", &challenge);
    let solution_binding = file_binding("Solution.lean", &solution);
    let config_binding = file_binding("config.json", &config_bytes);
    let formalization_binding = file_binding("formalization.yaml", &formalization_yaml);
    let source_formalization_binding = source_binding(formalization_member);
    let theorem_source_binding = source_binding(solution_member);
    let dependency_binding = source_binding(dependency_member);

    let input_fingerprint = sha256(&canonical_json(&json!({
        "source_release_manifest_hash": release.manifest_hash,
        "source_formalization": plan.formalization,
        "source_formalization_member": source_formalization_binding,
        "theorem_source": theorem_source_binding,
        "dependency_manifest": dependency_binding,
        "challenge": challenge_binding,
        "solution": solution_binding,
        "config": config_binding,
        "formalization": formalization_binding,
        "tool_pins": plan.tool_pins,
        "plan_hash": plan_hash
    }))?);

    let verification = ComparatorPackageVerification {
        schema_version: COMPARATOR_PACKAGE_VERIFICATION_SCHEMA_VERSION.to_owned(),
        status: ComparatorPackageStatus::Ready,
        comparator_verified: false,
        authoritative: false,
        source_release_manifest_hash: release.manifest_hash.clone(),
        source_formalization: plan.formalization.clone(),
        source_formalization_member: source_formalization_binding,
        theorem_source: theorem_source_binding,
        dependency_manifest: dependency_binding,
        challenge: challenge_binding,
        solution: solution_binding,
        config: config_binding,
        formalization: formalization_binding,
        declaration_name: formalization.declaration_name,
        lean_toolchain: environment.manifest.lean_toolchain.clone(),
        tool_pins: plan.tool_pins.clone(),
        plan_hash: plan_hash.to_owned(),
        input_fingerprint,
    };
    verification.validate()?;
    let verification_bytes = canonical_bytes(&verification, "Comparator verification")?;
    let verification_hash = sha256(&verification_bytes);
    let files = BTreeMap::from([
        ("Challenge.lean".to_owned(), challenge),
        ("Solution.lean".to_owned(), solution),
        ("config.json".to_owned(), config_bytes),
        ("formalization.yaml".to_owned(), formalization_yaml),
        ("verification.json".to_owned(), verification_bytes),
    ]);
    Ok(ComparatorProjection {
        files,
        verification,
        verification_hash,
    })
}

impl ComparatorConfig {
    fn validate(&self) -> Result<(), AppError> {
        if self.challenge_module != "Challenge"
            || self.solution_module != "Solution"
            || self.theorem_names.len() != 1
            || self
                .theorem_names
                .iter()
                .chain(&self.permitted_axioms)
                .any(|name| !is_lean_name(name))
            || self
                .permitted_axioms
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self.permitted_axioms.len() > 64
        {
            return Err(comparator_error(
                "MCL_COMPARATOR_CONFIG_INVALID",
                "Comparator config violates the fixed v1 module, theorem, or axiom contract",
                "Rebuild config.json from the canonical Comparator plan.",
            ));
        }
        Ok(())
    }
}

fn render_formalization_yaml(
    plan: &ComparatorPackagePlan,
    plan_hash: &str,
    release: &ReleaseIntegrity,
    formalization: &FormalizationPayload,
    environment: &EnvironmentSnapshot,
) -> Result<Vec<u8>, AppError> {
    let quote = |value: &str| {
        serde_json::to_string(value).map_err(|error| {
            comparator_error(
                "MCL_COMPARATOR_SERIALIZATION_FAILED",
                format!("formalization YAML scalar cannot be quoted: {error}"),
                "Report this deterministic Comparator serialization defect.",
            )
        })
    };
    let mut yaml = String::new();
    push_yaml_scalar(
        &mut yaml,
        "schema_version",
        &quote(COMPARATOR_FORMALIZATION_SCHEMA_VERSION)?,
        0,
    );
    push_yaml_scalar(
        &mut yaml,
        "mathematical_source",
        &quote(&plan.formalization_metadata.mathematical_source)?,
        0,
    );
    push_yaml_scalar(
        &mut yaml,
        "theorem_scope",
        &quote(&plan.formalization_metadata.theorem_scope)?,
        0,
    );
    push_yaml_scalar(
        &mut yaml,
        "ai_involvement",
        &quote(&plan.formalization_metadata.ai_involvement)?,
        0,
    );
    push_yaml_list(
        &mut yaml,
        "human_operators",
        &plan.formalization_metadata.human_operators,
        &quote,
    )?;
    push_yaml_list(
        &mut yaml,
        "upstream_repositories",
        &plan.formalization_metadata.upstream_repositories,
        &quote,
    )?;
    let publication_status = match plan.formalization_metadata.publication_status {
        ComparatorPublicationStatus::Draft => "draft",
        ComparatorPublicationStatus::Internal => "internal",
        ComparatorPublicationStatus::Published => "published",
    };
    push_yaml_scalar(
        &mut yaml,
        "publication_status",
        &quote(publication_status)?,
        0,
    );
    yaml.push_str("lean:\n");
    for (key, value) in [
        ("toolchain", environment.manifest.lean_toolchain.as_str()),
        ("environment_hash", formalization.environment_hash.as_str()),
        (
            "formalization_object_id",
            plan.formalization.object_id.as_str(),
        ),
        (
            "formalization_version_hash",
            plan.formalization.version_hash.as_str(),
        ),
        ("declaration_name", formalization.declaration_name.as_str()),
        (
            "exact_theorem_type",
            formalization.exact_theorem_type.as_str(),
        ),
        (
            "module_artifact_hash",
            formalization.module_artifact_hash.as_str(),
        ),
        (
            "source_release_manifest_hash",
            release.manifest_hash.as_str(),
        ),
    ] {
        push_yaml_scalar(&mut yaml, key, &quote(value)?, 2);
    }
    yaml.push_str("tools:\n");
    for (name, repository, commit) in [
        (
            "comparator",
            plan.tool_pins.comparator_repository.as_str(),
            plan.tool_pins.comparator_commit.as_str(),
        ),
        (
            "lean4export",
            plan.tool_pins.lean4export_repository.as_str(),
            plan.tool_pins.lean4export_commit.as_str(),
        ),
        (
            "landrun",
            plan.tool_pins.landrun_repository.as_str(),
            plan.tool_pins.landrun_commit.as_str(),
        ),
    ] {
        yaml.push_str(&format!("  {name}:\n"));
        push_yaml_scalar(&mut yaml, "repository", &quote(repository)?, 4);
        push_yaml_scalar(&mut yaml, "commit", &quote(commit)?, 4);
    }
    yaml.push_str("package_status:\n");
    push_yaml_scalar(&mut yaml, "status", &quote("ready")?, 2);
    push_yaml_scalar(&mut yaml, "comparator_verified", "false", 2);
    push_yaml_scalar(&mut yaml, "authoritative", "false", 2);
    push_yaml_scalar(&mut yaml, "plan_hash", &quote(plan_hash)?, 2);
    if yaml.len() as u64 > MAX_COMPARATOR_FILE_BYTES {
        return Err(comparator_error(
            "MCL_COMPARATOR_FILE_TOO_LARGE",
            "formalization.yaml exceeds the 4 MiB package-member bound",
            "Reduce reviewed formalization metadata.",
        ));
    }
    Ok(yaml.into_bytes())
}

fn push_yaml_scalar(output: &mut String, key: &str, value: &str, indent: usize) {
    output.push_str(&" ".repeat(indent));
    output.push_str(key);
    output.push_str(": ");
    output.push_str(value);
    output.push('\n');
}

fn push_yaml_list<F>(
    output: &mut String,
    key: &str,
    values: &[String],
    quote: &F,
) -> Result<(), AppError>
where
    F: Fn(&str) -> Result<String, AppError>,
{
    if values.is_empty() {
        output.push_str(key);
        output.push_str(": []\n");
    } else {
        output.push_str(key);
        output.push_str(":\n");
        for value in values {
            output.push_str("  - ");
            output.push_str(&quote(value)?);
            output.push('\n');
        }
    }
    Ok(())
}

fn verify_package_integrity(root: &Path) -> Result<ComparatorPackageIntegrity, AppError> {
    let root = require_real_directory(root, "Comparator package root")?;
    let observed = inventory(&root)?;
    let expected = PACKAGE_PATHS
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if observed != expected {
        return Err(comparator_error(
            "MCL_COMPARATOR_PACKAGE_INVENTORY_MISMATCH",
            "Comparator package inventory differs from the exact five-file contract",
            "Restore exactly Challenge.lean, Solution.lean, config.json, formalization.yaml, and verification.json.",
        ));
    }
    let mut files = BTreeMap::new();
    for path in PACKAGE_PATHS {
        files.insert(
            path.to_owned(),
            read_real_file(&root.join(path), MAX_COMPARATOR_FILE_BYTES)?,
        );
    }
    let verification: ComparatorPackageVerification = decode_canonical(
        required_package(&files, "verification.json")?,
        "Comparator verification",
    )?;
    verification.validate()?;
    let config: ComparatorConfig = decode_canonical(
        required_package(&files, "config.json")?,
        "Comparator config",
    )?;
    config.validate()?;
    if config.theorem_names != [verification.declaration_name.clone()] {
        return Err(binding_error(
            "Comparator config theorem does not match verification metadata",
        ));
    }
    for binding in [
        &verification.challenge,
        &verification.solution,
        &verification.config,
        &verification.formalization,
    ] {
        let bytes = required_package(&files, &binding.path)?;
        if binding.content_hash != sha256(bytes) || binding.byte_size != bytes.len() as u64 {
            return Err(binding_error(format!(
                "Comparator package member `{}` differs from verification metadata",
                binding.path
            )));
        }
    }
    let formalization_yaml = required_package(&files, "formalization.yaml")?;
    if std::str::from_utf8(formalization_yaml).is_err()
        || formalization_yaml.contains(&0)
        || formalization_yaml.contains(&b'\r')
        || !formalization_yaml.ends_with(b"\n")
    {
        return Err(comparator_error(
            "MCL_COMPARATOR_FORMALIZATION_INVALID",
            "formalization.yaml is not bounded LF-terminated UTF-8",
            "Restore the deterministic machine-readable YAML member.",
        ));
    }
    let verification_hash = sha256(required_package(&files, "verification.json")?);
    Ok(ComparatorPackageIntegrity {
        files,
        verification,
        verification_hash,
    })
}

fn read_plan(path: &Path) -> Result<(ComparatorPackagePlan, String), AppError> {
    let bytes = read_real_file(path, PLAN_MAX_BYTES)?;
    let plan: ComparatorPackagePlan = decode_canonical(&bytes, "Comparator package plan")?;
    plan.validate()?;
    Ok((plan, sha256(&bytes)))
}

fn file_binding(path: &str, bytes: &[u8]) -> ComparatorFileBinding {
    ComparatorFileBinding {
        path: path.to_owned(),
        content_hash: sha256(bytes),
        byte_size: bytes.len() as u64,
    }
}

fn source_binding(member: &ReleaseMember) -> ComparatorSourceMemberBinding {
    ComparatorSourceMemberBinding {
        path: member.path.clone(),
        content_hash: member.content_hash.clone(),
        byte_size: member.byte_size,
    }
}

fn required_member<'a>(
    release: &'a ReleaseIntegrity,
    path: &str,
) -> Result<&'a ReleaseMember, AppError> {
    release
        .manifest
        .members
        .iter()
        .find(|member| member.path == path)
        .ok_or_else(|| binding_error(format!("source release member `{path}` is unbound")))
}

fn required_source<'a>(release: &'a ReleaseIntegrity, path: &str) -> Result<&'a [u8], AppError> {
    release
        .files
        .get(path)
        .map(Vec::as_slice)
        .ok_or_else(|| binding_error(format!("required source release member `{path}` is absent")))
}

fn required_package<'a>(
    files: &'a BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<&'a [u8], AppError> {
    files.get(path).map(Vec::as_slice).ok_or_else(|| {
        binding_error(format!(
            "required Comparator package member `{path}` is absent"
        ))
    })
}

fn canonical_bytes<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, AppError> {
    let value = serde_json::to_value(value).map_err(|error| {
        comparator_error(
            "MCL_COMPARATOR_SERIALIZATION_FAILED",
            format!("{label} cannot be serialized: {error}"),
            "Report this deterministic Comparator serialization defect.",
        )
    })?;
    canonical_json(&value)
}

fn decode_canonical<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    label: &str,
) -> Result<T, AppError> {
    let decoded: T = serde_json::from_slice(bytes).map_err(|error| {
        comparator_error(
            "MCL_COMPARATOR_JSON_INVALID",
            format!("{label} is not closed valid JSON: {error}"),
            "Restore the exact canonical JSON member without unknown fields.",
        )
    })?;
    if canonical_bytes(&decoded, label)? != bytes {
        return Err(comparator_error(
            "MCL_COMPARATOR_JSON_NONCANONICAL",
            format!("{label} is not exact canonical JSON"),
            "Use compact RFC 8785 canonical JSON without trailing whitespace.",
        ));
    }
    Ok(decoded)
}

fn resolve_new_output(output: &Path) -> Result<(PathBuf, PathBuf), AppError> {
    let absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| AppError::io("resolve current directory", error))?
            .join(output)
    };
    if fs::symlink_metadata(&absolute).is_ok() {
        return Err(comparator_error(
            "MCL_COMPARATOR_OUTPUT_EXISTS",
            format!(
                "Comparator package output already exists at {}",
                absolute.display()
            ),
            "Choose a new destination; Comparator packages never overwrite paths.",
        ));
    }
    let parent = absolute.parent().ok_or_else(|| {
        comparator_error(
            "MCL_COMPARATOR_OUTPUT_UNSAFE",
            "Comparator package output has no parent directory",
            "Choose a named directory beneath a real existing parent.",
        )
    })?;
    let parent = require_real_directory(parent, "Comparator package output parent")?;
    let name = absolute.file_name().ok_or_else(|| {
        comparator_error(
            "MCL_COMPARATOR_OUTPUT_UNSAFE",
            "Comparator package output has no plain directory name",
            "Choose a named directory beneath a real existing parent.",
        )
    })?;
    Ok((parent.clone(), parent.join(name)))
}

fn write_new_member(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), AppError> {
    let destination = safe_member_path(root, relative)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&destination)
        .map_err(|error| AppError::io("create Comparator package member", error))?;
    file.write_all(bytes)
        .map_err(|error| AppError::io("write Comparator package member", error))?;
    file.sync_all()
        .map_err(|error| AppError::io("sync Comparator package member", error))
}

fn require_real_directory(path: &Path, label: &str) -> Result<PathBuf, AppError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| AppError::io("inspect directory", error))?;
    if is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(comparator_error(
            "MCL_COMPARATOR_PATH_UNSAFE",
            format!("{label} is not a real directory"),
            "Use a real directory tree without symbolic links, junctions, or reparse points.",
        ));
    }
    path.canonicalize()
        .map_err(|error| AppError::io("canonicalize Comparator package directory", error))
}

fn safe_member_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || relative.contains('\\')
    {
        return Err(comparator_error(
            "MCL_COMPARATOR_PATH_UNSAFE",
            format!("unsafe Comparator package path `{relative}`"),
            "Use only the five fixed root-level package names.",
        ));
    }
    Ok(root.join(path))
}

fn read_real_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io("inspect Comparator input member", error))?;
    if is_link_or_reparse(&metadata) || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(comparator_error(
            "MCL_COMPARATOR_PATH_UNSAFE",
            format!("Comparator input {} is unsafe or oversized", path.display()),
            "Use the exact bounded regular file without links or reparse points.",
        ));
    }
    fs::read(path).map_err(|error| AppError::io("read Comparator input member", error))
}

fn inventory(root: &Path) -> Result<BTreeSet<String>, AppError> {
    let mut files = BTreeSet::new();
    let mut entries = 0;
    for entry in fs::read_dir(root)
        .map_err(|error| AppError::io("read Comparator package directory", error))?
    {
        let entry = entry.map_err(|error| AppError::io("read Comparator package entry", error))?;
        entries += 1;
        if entries > MAX_PACKAGE_TREE_ENTRIES {
            return Err(comparator_error(
                "MCL_COMPARATOR_PACKAGE_INVENTORY_MISMATCH",
                "Comparator package exceeds its exact five-entry bound",
                "Restore the exact five-file package.",
            ));
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| AppError::io("inspect Comparator package entry", error))?;
        if is_link_or_reparse(&metadata) || !metadata.is_file() {
            return Err(comparator_error(
                "MCL_COMPARATOR_PATH_UNSAFE",
                "Comparator package contains a link, reparse point, directory, or non-file entry",
                "Use only five real regular files at the package root.",
            ));
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| binding_error("Comparator package entry name is not UTF-8"))?;
        files.insert(name.to_owned());
    }
    Ok(files)
}

fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn is_lean_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'\''))
        })
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn require_hash(value: &str, label: &str, code: &'static str) -> Result<(), AppError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(comparator_error(
            code,
            format!("{label} is not a lowercase SHA-256 identity"),
            "Use the exact identity emitted by the trusted release or package channel.",
        ));
    }
    Ok(())
}

fn binding_error(message: impl Into<String>) -> AppError {
    comparator_error(
        "MCL_COMPARATOR_BINDING_MISMATCH",
        message,
        "Quarantine the package and rebuild it from the exact canonical plan and frozen release.",
    )
}

fn comparator_error(
    code: &'static str,
    message: impl Into<String>,
    corrective_action: impl Into<String>,
) -> AppError {
    AppError::new(code, message, false, corrective_action)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::domain::environment::EnvironmentFormalSystem;
    use crate::domain::schemas::{FormalSystem, FormalizationClaimPolarity};
    use crate::domain::{
        ArtifactRestriction, COMPARATOR_REPOSITORY, ComparatorFormalizationMetadata,
        ComparatorPublicationStatus, ComparatorToolPins, DependencyRevision, EnvironmentManifest,
        EnvironmentPlatform, LANDRUN_REPOSITORY, LEAN4EXPORT_REPOSITORY, ReleaseManifest,
        ReleaseMemberKind, ReleasePedagogyBinding, ReleasePedagogyMode, ReleaseProfile,
        ReleasePublicationBinding, ReleaseReplayBinding, ResourceLimits, TrustProfile,
        VerifierArgument, VerifierCommandTemplate, VerifierExecutable, WorkingDirectoryPolicy,
    };

    use super::*;

    #[test]
    fn projection_is_deterministic_exactly_five_files_and_non_authoritative() {
        let (release, plan, plan_hash) = synthetic_fixture("", "v4.32.0");
        let first = project_comparator(&release, &plan, &plan_hash).expect("first projection");
        let second = project_comparator(&release, &plan, &plan_hash).expect("second projection");
        assert_eq!(first.files, second.files);
        assert_eq!(first.verification, second.verification);
        assert_eq!(first.verification_hash, second.verification_hash);
        assert_eq!(
            first.files.keys().cloned().collect::<BTreeSet<_>>(),
            PACKAGE_PATHS
                .into_iter()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            first.files["Solution.lean"],
            release.files["replay/Submission.lean"]
        );
        assert_eq!(first.verification.status, ComparatorPackageStatus::Ready);
        assert!(!first.verification.comparator_verified);
        assert!(!first.verification.authoritative);
        let yaml = std::str::from_utf8(&first.files["formalization.yaml"])
            .expect("formalization YAML is UTF-8");
        assert!(yaml.contains("status: \"ready\""));
        assert!(yaml.contains("comparator_verified: false"));
        assert!(yaml.contains("authoritative: false"));
    }

    #[test]
    fn every_stale_sensitive_input_changes_package_identity() {
        let (release, plan, plan_hash) = synthetic_fixture("", "v4.32.0");
        let baseline = project_comparator(&release, &plan, &plan_hash).expect("baseline");

        let mut challenge = plan.clone();
        challenge.challenge_source.push('\n');
        let challenge_hash = plan_identity(&challenge);
        let challenge_projection =
            project_comparator(&release, &challenge, &challenge_hash).expect("changed challenge");
        assert_ne!(
            challenge_projection.verification_hash,
            baseline.verification_hash
        );

        let mut config = plan.clone();
        config.permitted_axioms = vec!["propext".to_owned()];
        let config_hash = plan_identity(&config);
        let config_projection =
            project_comparator(&release, &config, &config_hash).expect("changed config");
        assert_ne!(
            config_projection.verification_hash,
            baseline.verification_hash
        );

        let (changed_source, changed_source_plan, changed_source_plan_hash) =
            synthetic_fixture("\n", "v4.32.0");
        let source_projection = project_comparator(
            &changed_source,
            &changed_source_plan,
            &changed_source_plan_hash,
        )
        .expect("changed theorem source");
        assert_ne!(
            source_projection.verification_hash,
            baseline.verification_hash
        );

        let (changed_environment, changed_environment_plan, changed_environment_plan_hash) =
            synthetic_fixture("", "v4.31.0");
        let environment_projection = project_comparator(
            &changed_environment,
            &changed_environment_plan,
            &changed_environment_plan_hash,
        )
        .expect("changed dependency manifest");
        assert_ne!(
            environment_projection.verification_hash,
            baseline.verification_hash
        );
    }

    #[test]
    fn package_integrity_rejects_extra_missing_and_altered_members() {
        let (release, plan, plan_hash) = synthetic_fixture("", "v4.32.0");
        let projection = project_comparator(&release, &plan, &plan_hash).expect("projection");

        let extra = tempfile::tempdir().expect("extra package root");
        materialize(extra.path(), &projection.files);
        fs::write(extra.path().join("extra.txt"), b"extra").expect("extra member");
        assert_eq!(
            verify_package_integrity(extra.path())
                .expect_err("extra member rejected")
                .code,
            "MCL_COMPARATOR_PACKAGE_INVENTORY_MISMATCH"
        );

        let missing = tempfile::tempdir().expect("missing package root");
        materialize(missing.path(), &projection.files);
        fs::remove_file(missing.path().join("config.json")).expect("remove config");
        assert_eq!(
            verify_package_integrity(missing.path())
                .expect_err("missing member rejected")
                .code,
            "MCL_COMPARATOR_PACKAGE_INVENTORY_MISMATCH"
        );

        let altered = tempfile::tempdir().expect("altered package root");
        materialize(altered.path(), &projection.files);
        fs::write(
            altered.path().join("Challenge.lean"),
            b"theorem forged : True := by trivial\n",
        )
        .expect("alter challenge");
        assert_eq!(
            verify_package_integrity(altered.path())
                .expect_err("altered member rejected")
                .code,
            "MCL_COMPARATOR_BINDING_MISMATCH"
        );
    }

    #[test]
    fn trusted_verification_hash_and_immutable_output_are_required() {
        let (release, plan, plan_hash) = synthetic_fixture("", "v4.32.0");
        let projection = project_comparator(&release, &plan, &plan_hash).expect("projection");
        let root = tempfile::tempdir().expect("package root");
        materialize(root.path(), &projection.files);
        let observed = verify_package_integrity(root.path()).expect("package integrity");
        assert_eq!(observed.verification_hash, projection.verification_hash);
        let substituted_hash = if projection.verification_hash == "a".repeat(64) {
            "b".repeat(64)
        } else {
            "a".repeat(64)
        };
        assert_eq!(
            verify_comparator_package(
                root.path(),
                &substituted_hash,
                Path::new("unread-plan"),
                Path::new("unread-release"),
                &"c".repeat(64),
            )
            .expect_err("substituted trusted hash rejected before source reads")
            .code,
            "MCL_COMPARATOR_VERIFICATION_HASH_MISMATCH"
        );

        let output_parent = tempfile::tempdir().expect("output parent");
        let output = output_parent.path().join("existing");
        fs::create_dir(&output).expect("existing output");
        assert_eq!(
            resolve_new_output(&output)
                .expect_err("overwrite rejected")
                .code,
            "MCL_COMPARATOR_OUTPUT_EXISTS"
        );
    }

    #[test]
    fn canonical_plan_file_is_required() {
        let (_, plan, _) = synthetic_fixture("", "v4.32.0");
        let root = tempfile::tempdir().expect("plan root");
        let pretty = serde_json::to_vec_pretty(&plan).expect("pretty plan");
        let path = root.path().join("plan.json");
        fs::write(&path, pretty).expect("write pretty plan");
        assert_eq!(
            read_plan(&path)
                .expect_err("noncanonical plan rejected")
                .code,
            "MCL_COMPARATOR_JSON_NONCANONICAL"
        );
    }

    #[test]
    fn release_and_formalization_substitution_fail_closed() {
        let (release, plan, _plan_hash) = synthetic_fixture("", "v4.32.0");

        let mut wrong_formalization = plan.clone();
        wrong_formalization.formalization.version_hash = "f".repeat(64);
        let wrong_plan_hash = plan_identity(&wrong_formalization);
        assert_eq!(
            project_comparator(&release, &wrong_formalization, &wrong_plan_hash)
                .expect_err("formalization substitution rejected")
                .code,
            "MCL_COMPARATOR_BINDING_MISMATCH"
        );

        let (mut changed_solution, changed_solution_plan, changed_solution_plan_hash) =
            synthetic_fixture("", "v4.32.0");
        changed_solution
            .files
            .get_mut("replay/Submission.lean")
            .expect("solution member")
            .push(b' ');
        assert_eq!(
            project_comparator(
                &changed_solution,
                &changed_solution_plan,
                &changed_solution_plan_hash,
            )
            .expect_err("solution substitution rejected")
            .code,
            "MCL_COMPARATOR_BINDING_MISMATCH"
        );

        let (mut changed_environment, changed_environment_plan, changed_environment_plan_hash) =
            synthetic_fixture("", "v4.32.0");
        changed_environment
            .files
            .get_mut("replay/environment.json")
            .expect("environment member")
            .push(b' ');
        assert_eq!(
            project_comparator(
                &changed_environment,
                &changed_environment_plan,
                &changed_environment_plan_hash,
            )
            .expect_err("dependency substitution rejected")
            .code,
            "MCL_COMPARATOR_JSON_NONCANONICAL"
        );
    }

    fn plan_identity(plan: &ComparatorPackagePlan) -> String {
        sha256(&canonical_bytes(plan, "test plan").expect("plan bytes"))
    }

    fn materialize(root: &Path, files: &BTreeMap<String, Vec<u8>>) {
        for (path, bytes) in files {
            write_new_member(root, path, bytes).expect("write package member");
        }
    }

    fn synthetic_fixture(
        module_suffix: &str,
        lean_version: &str,
    ) -> (ReleaseIntegrity, ComparatorPackagePlan, String) {
        let module = format!(
            "namespace Fixture\n\ntheorem theorem : True := by\n  trivial\n\nend Fixture\n{module_suffix}"
        )
        .into_bytes();
        let module_hash = sha256(&module);
        let environment_manifest = EnvironmentManifest {
            schema_version: "environment/1".to_owned(),
            formal_system: EnvironmentFormalSystem::Lean4,
            lean_toolchain: format!("leanprover/lean4:{lean_version}"),
            dependencies: Vec::<DependencyRevision>::new(),
            import_manifest: Vec::new(),
            project_configuration_hashes: BTreeMap::from([(
                "lean-toolchain".to_owned(),
                "1".repeat(64),
            )]),
            platform: EnvironmentPlatform::LinuxX86_64,
            trust_profile: TrustProfile::Local,
            verifier_command: VerifierCommandTemplate {
                executable: VerifierExecutable::Lean,
                arguments: vec![VerifierArgument::ModulePath],
            },
            resource_limits: ResourceLimits {
                timeout_seconds: 120,
                max_output_bytes: 1_048_576,
                max_memory_bytes: None,
                concurrency: 1,
            },
            network_access: false,
            working_directory_policy: WorkingDirectoryPolicy::TemporaryWorkspace,
        };
        let environment_hash = environment_manifest
            .environment_hash()
            .expect("environment identity");
        let environment = EnvironmentSnapshot {
            environment_hash: environment_hash.clone(),
            manifest: environment_manifest,
            created_at: 1_700_000_000,
            created_by: "fixture".to_owned(),
        };
        let environment_bytes =
            canonical_bytes(&environment, "test environment").expect("environment bytes");

        let formalization_id = "00000000-0000-4000-8000-000000000001";
        let claim_id = "00000000-0000-4000-8000-000000000002";
        let formalization_payload = FormalizationPayload {
            claim_version: ExactVersionReference {
                object_id: claim_id.to_owned(),
                version_hash: "2".repeat(64),
            },
            formal_system: FormalSystem::Lean4,
            claim_polarity: Some(FormalizationClaimPolarity::Claim),
            environment_hash: environment_hash.clone(),
            module_artifact_hash: module_hash.clone(),
            declaration_name: "Fixture.theorem".to_owned(),
            exact_theorem_type: "True".to_owned(),
            declaration_hash: "3".repeat(64),
            import_manifest: Vec::new(),
            formalization_notes: "Synthetic exact theorem.".to_owned(),
            fidelity_evidence_references: Vec::new(),
            verification_evidence_references: Vec::new(),
        };
        let formalization_value =
            serde_json::to_value(&formalization_payload).expect("formalization value");
        let formalization_version = record_version_hash("formalization/1", &formalization_value)
            .expect("formalization identity");
        let formalization_record = RecordSnapshot {
            object_id: formalization_id.to_owned(),
            kind: RecordKind::Formalization,
            version_hash: formalization_version.clone(),
            schema_version: "formalization/1".to_owned(),
            payload: formalization_value,
            predecessor_hash: None,
            created_at: 1_700_000_000,
            created_by: "fixture".to_owned(),
        };
        let formalization_bytes = canonical_bytes(&formalization_record, "test formalization")
            .expect("formalization bytes");
        let formalization_path =
            format!("objects/formalization/{formalization_id}@{formalization_version}.json");

        let authority_id = "00000000-0000-4000-8000-000000000003";
        let fidelity_id = "00000000-0000-4000-8000-000000000004";
        let publication = ReleasePublicationBinding {
            ingestion_receipt_hash: "4".repeat(64),
            authority_evidence_id: authority_id.to_owned(),
            authority_evidence_hash: "5".repeat(64),
            fidelity_evidence_id: fidelity_id.to_owned(),
            fidelity_evidence_hash: "6".repeat(64),
            fidelity_report_artifact_hash: "7".repeat(64),
            stage_hash: "8".repeat(64),
            report_artifact_hash: "9".repeat(64),
            retained_closure_artifact_hash: "a".repeat(64),
            attestation_bundle_artifact_hash: "b".repeat(64),
            raw_verification_hash: "c".repeat(64),
            request_hash: "d".repeat(64),
            policy_hash: "e".repeat(64),
            subject: ExactVersionReference {
                object_id: formalization_id.to_owned(),
                version_hash: formalization_version.clone(),
            },
            outcome: crate::domain::PublicationOutcome::Proof,
            environment_hash,
            module_artifact_hash: module_hash,
            declaration_name: "Fixture.theorem".to_owned(),
        };
        let fidelity_path = format!(
            "reports/fidelity/{}@{}.json",
            publication.fidelity_evidence_id, publication.fidelity_evidence_hash
        );

        let mut files = BTreeMap::<String, Vec<u8>>::from([
            ("edges/edge.json".to_owned(), b"x".to_vec()),
            ("environments/environment.json".to_owned(), b"x".to_vec()),
            ("evidence/evidence.json".to_owned(), b"x".to_vec()),
            ("exports/pedagogy-path.json".to_owned(), b"x".to_vec()),
            ("licenses/index.json".to_owned(), b"x".to_vec()),
            (formalization_path.clone(), formalization_bytes),
            ("replay/Submission.lean".to_owned(), module),
            ("replay/environment.json".to_owned(), environment_bytes),
            ("replay/replay.json".to_owned(), b"x".to_vec()),
            ("reports/attestation-bundle.json".to_owned(), b"x".to_vec()),
            (
                "reports/canonical-attestation-receipt.json".to_owned(),
                b"x".to_vec(),
            ),
            ("reports/publication-receipt.json".to_owned(), b"x".to_vec()),
            ("reports/publication-report.json".to_owned(), b"x".to_vec()),
            (
                "reports/publication-retained-closure.json".to_owned(),
                b"x".to_vec(),
            ),
            ("reports/publication-stage.json".to_owned(), b"x".to_vec()),
            (
                "reports/raw-attestation-verification.json".to_owned(),
                b"x".to_vec(),
            ),
            (fidelity_path, b"x".to_vec()),
        ]);
        let artifact_bytes = b"x".to_vec();
        let artifact_hash = sha256(&artifact_bytes);
        files.insert(format!("artifacts/{artifact_hash}"), artifact_bytes);

        let kind = |path: &str| {
            if path.starts_with("artifacts/") {
                ReleaseMemberKind::Artifact
            } else if path.starts_with("edges/") {
                ReleaseMemberKind::Edge
            } else if path.starts_with("environments/") {
                ReleaseMemberKind::Environment
            } else if path.starts_with("evidence/") {
                ReleaseMemberKind::Evidence
            } else if path.starts_with("exports/") {
                ReleaseMemberKind::Export
            } else if path.starts_with("licenses/") {
                ReleaseMemberKind::License
            } else if path.starts_with("objects/") {
                ReleaseMemberKind::Object
            } else if path.starts_with("replay/") {
                ReleaseMemberKind::Replay
            } else {
                ReleaseMemberKind::Report
            }
        };
        let members = files
            .iter()
            .map(|(path, bytes)| ReleaseMember {
                path: path.clone(),
                kind: kind(path),
                content_hash: sha256(bytes),
                byte_size: bytes.len() as u64,
                license_expression: None,
                restriction: ArtifactRestriction::Private,
                artifact_metadata: None,
            })
            .collect::<Vec<_>>();
        let pedagogy_root = ExactVersionReference {
            object_id: "00000000-0000-4000-8000-000000000005".to_owned(),
            version_hash: "f".repeat(64),
        };
        let manifest = ReleaseManifest {
            schema_version: crate::domain::RELEASE_MANIFEST_SCHEMA_VERSION.to_owned(),
            profile: ReleaseProfile::Private,
            publication,
            pedagogy: ReleasePedagogyBinding {
                mode: ReleasePedagogyMode::Prerequisites,
                include_soft: false,
                root: pedagogy_root.clone(),
                unit_order: vec![pedagogy_root],
                edge_ids: Vec::new(),
            },
            replay: ReleaseReplayBinding {
                module_path: "replay/Submission.lean".to_owned(),
                environment_path: "replay/environment.json".to_owned(),
                declaration_name: "Fixture.theorem".to_owned(),
            },
            members,
        };
        let manifest_hash = manifest.manifest_hash().expect("release manifest identity");
        let release = ReleaseIntegrity {
            manifest,
            manifest_hash: manifest_hash.clone(),
            files,
        };
        let plan = ComparatorPackagePlan {
            schema_version: crate::domain::COMPARATOR_PACKAGE_PLAN_SCHEMA_VERSION.to_owned(),
            source_release_manifest_hash: manifest_hash,
            formalization: ExactVersionReference {
                object_id: formalization_id.to_owned(),
                version_hash: formalization_version,
            },
            challenge_source:
                "namespace Fixture\n\ntheorem theorem : True := by\n  sorry\n\nend Fixture\n"
                    .to_owned(),
            theorem_names: vec!["Fixture.theorem".to_owned()],
            permitted_axioms: Vec::new(),
            enable_nanoda: false,
            tool_pins: ComparatorToolPins {
                comparator_repository: COMPARATOR_REPOSITORY.to_owned(),
                comparator_commit: "1".repeat(40),
                lean4export_repository: LEAN4EXPORT_REPOSITORY.to_owned(),
                lean4export_commit: "2".repeat(40),
                landrun_repository: LANDRUN_REPOSITORY.to_owned(),
                landrun_commit: "3".repeat(40),
            },
            formalization_metadata: ComparatorFormalizationMetadata {
                mathematical_source: "Synthetic theorem source.".to_owned(),
                theorem_scope: "The proposition True.".to_owned(),
                ai_involvement: "Synthetic test fixture construction.".to_owned(),
                human_operators: vec!["MathOS test operator".to_owned()],
                upstream_repositories: vec!["https://github.com/Mnehmos/MathOS".to_owned()],
                publication_status: ComparatorPublicationStatus::Internal,
            },
        };
        let plan_hash = plan_identity(&plan);
        (release, plan, plan_hash)
    }
}
