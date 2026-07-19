use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppError;

#[derive(Clone, Debug)]
pub struct ArtifactStore {
    root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactStoreScan {
    pub content_hashes: Vec<String>,
    pub incomplete_temporary_files: usize,
}

impl ArtifactStore {
    pub fn open(root: &Path) -> Result<Self, AppError> {
        if let Ok(metadata) = fs::symlink_metadata(root)
            && metadata.file_type().is_symlink()
        {
            return Err(AppError::new(
                "MCL_ARTIFACT_ROOT_UNSAFE",
                "artifact root may not be a symbolic link",
                false,
                "Configure a real directory under the instance root.",
            ));
        }
        fs::create_dir_all(root).map_err(|error| AppError::io("create artifact root", error))?;
        let root = root
            .canonicalize()
            .map_err(|error| AppError::io("canonicalize artifact root", error))?;
        Ok(Self { root })
    }

    pub fn put(&self, bytes: &[u8]) -> Result<String, AppError> {
        let hash = format!("{:x}", Sha256::digest(bytes));
        let destination = self.path_for_hash(&hash)?;
        if destination.exists() {
            self.verify_file(&destination, &hash, "MCL_ARTIFACT_HASH_COLLISION")?;
            return Ok(hash);
        }
        let parent = destination.parent().expect("hash path has parent");
        self.reject_symlink_components(parent)?;
        fs::create_dir_all(parent)
            .map_err(|error| AppError::io("create artifact hash directory", error))?;
        self.reject_symlink_components(&destination)?;
        let temporary = parent.join(format!(".tmp-{}", Uuid::now_v7()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| AppError::io("create temporary artifact", error))?;
        file.write_all(bytes)
            .map_err(|error| AppError::io("write temporary artifact", error))?;
        file.sync_all()
            .map_err(|error| AppError::io("sync temporary artifact", error))?;
        drop(file);
        let written = fs::read(&temporary)
            .map_err(|error| AppError::io("verify temporary artifact", error))?;
        if format!("{:x}", Sha256::digest(&written)) != hash {
            let _ = fs::remove_file(&temporary);
            return Err(AppError::new(
                "MCL_ARTIFACT_WRITE_CORRUPT",
                "artifact changed before atomic commit",
                true,
                "Retry the operation and inspect storage health if it repeats.",
            ));
        }
        match fs::rename(&temporary, &destination) {
            Ok(()) => Ok(hash),
            Err(_) if destination.exists() => {
                let _ = fs::remove_file(&temporary);
                self.verify_file(&destination, &hash, "MCL_ARTIFACT_HASH_COLLISION")?;
                Ok(hash)
            }
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(AppError::io("commit artifact atomically", error))
            }
        }
    }

    pub fn read(&self, hash: &str) -> Result<Vec<u8>, AppError> {
        let path = self.path_for_hash(hash)?;
        self.reject_symlink_components(&path)?;
        let bytes = fs::read(path).map_err(|error| AppError::io("read artifact", error))?;
        if format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Err(AppError::new(
                "MCL_ARTIFACT_INTEGRITY_FAILED",
                format!("artifact {hash} failed hash verification"),
                false,
                "Quarantine the artifact store and restore a verified backup.",
            ));
        }
        Ok(bytes)
    }

    pub fn materialize(
        &self,
        hash: &str,
        workspace_root: &Path,
        file_name: &str,
    ) -> Result<PathBuf, AppError> {
        if file_name.is_empty()
            || file_name == "."
            || file_name == ".."
            || file_name.contains('/')
            || file_name.contains('\\')
            || Path::new(file_name).is_absolute()
            || Path::new(file_name).components().count() != 1
        {
            return Err(AppError::new(
                "MCL_ARTIFACT_MATERIALIZATION_UNSAFE",
                "artifact materialization requires one plain file name",
                false,
                "Use a verifier-selected file name without path components.",
            ));
        }
        let root_metadata = fs::symlink_metadata(workspace_root)
            .map_err(|error| AppError::io("inspect materialization workspace", error))?;
        if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
            return Err(AppError::new(
                "MCL_ARTIFACT_MATERIALIZATION_UNSAFE",
                "artifact materialization workspace must be a real existing directory",
                false,
                "Use a newly created temporary verifier workspace.",
            ));
        }
        let workspace_root = workspace_root
            .canonicalize()
            .map_err(|error| AppError::io("canonicalize materialization workspace", error))?;
        let destination = workspace_root.join(file_name);
        if destination.exists()
            || fs::symlink_metadata(&destination)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(AppError::new(
                "MCL_ARTIFACT_MATERIALIZATION_EXISTS",
                format!(
                    "materialization destination {} already exists",
                    destination.display()
                ),
                false,
                "Use a fresh temporary verifier workspace.",
            ));
        }
        let bytes = self.read(hash)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&destination)
            .map_err(|error| AppError::io("create materialized artifact", error))?;
        file.write_all(&bytes)
            .map_err(|error| AppError::io("write materialized artifact", error))?;
        file.sync_all()
            .map_err(|error| AppError::io("sync materialized artifact", error))?;
        Ok(destination)
    }

    pub fn scan(&self) -> Result<ArtifactStoreScan, AppError> {
        const MAX_SCAN_ENTRIES: usize = 100_000;
        let sha_root = self.root.join("sha256");
        if !sha_root.exists() {
            return Ok(ArtifactStoreScan {
                content_hashes: Vec::new(),
                incomplete_temporary_files: 0,
            });
        }
        self.reject_symlink_components(&sha_root)?;
        let mut hashes = Vec::new();
        let mut temporary_files = 0;
        for first in read_directory(&sha_root, "scan artifact prefix")? {
            require_real_directory(&first, "artifact first-level prefix")?;
            let first_name = safe_file_name(&first)?;
            require_hex_prefix(&first_name)?;
            for second in read_directory(&first, "scan artifact subprefix")? {
                require_real_directory(&second, "artifact second-level prefix")?;
                let second_name = safe_file_name(&second)?;
                require_hex_prefix(&second_name)?;
                for artifact in read_directory(&second, "scan artifact objects")? {
                    let metadata = fs::symlink_metadata(&artifact)
                        .map_err(|error| AppError::io("inspect artifact scan entry", error))?;
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        return Err(AppError::new(
                            "MCL_ARTIFACT_PATH_UNSAFE",
                            format!("unsafe entry in artifact store: {}", artifact.display()),
                            false,
                            "Quarantine the artifact store and inspect its directory structure.",
                        ));
                    }
                    let name = safe_file_name(&artifact)?;
                    if name.starts_with(".tmp-") {
                        temporary_files += 1;
                    } else {
                        self.path_for_hash(&name)?;
                        if !name.starts_with(&format!("{first_name}{second_name}")) {
                            return Err(AppError::new(
                                "MCL_ARTIFACT_PATH_UNSAFE",
                                format!("artifact {name} is stored under the wrong hash prefix"),
                                false,
                                "Quarantine the artifact store and inspect its directory structure.",
                            ));
                        }
                        hashes.push(name);
                    }
                    if hashes.len() + temporary_files > MAX_SCAN_ENTRIES {
                        return Err(AppError::new(
                            "MCL_ARTIFACT_SCAN_LIMIT",
                            "artifact scan exceeded its reviewed entry bound",
                            false,
                            "Inspect storage growth before increasing the scan policy.",
                        ));
                    }
                }
            }
        }
        hashes.sort();
        Ok(ArtifactStoreScan {
            content_hashes: hashes,
            incomplete_temporary_files: temporary_files,
        })
    }

    fn path_for_hash(&self, hash: &str) -> Result<PathBuf, AppError> {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AppError::new(
                "MCL_ARTIFACT_HASH_INVALID",
                "artifact identity must be a 64-character hexadecimal SHA-256 hash",
                false,
                "Use an artifact hash returned by the engine.",
            ));
        }
        Ok(self
            .root
            .join("sha256")
            .join(&hash[0..2])
            .join(&hash[2..4])
            .join(hash))
    }

    fn verify_file(
        &self,
        path: &Path,
        hash: &str,
        error_code: &'static str,
    ) -> Result<(), AppError> {
        self.reject_symlink_components(path)?;
        let existing =
            fs::read(path).map_err(|error| AppError::io("read existing artifact", error))?;
        if format!("{:x}", Sha256::digest(&existing)) != hash {
            return Err(AppError::new(
                error_code,
                format!("stored artifact does not match identity {hash}"),
                false,
                "Quarantine the artifact store and restore a verified backup.",
            ));
        }
        Ok(())
    }

    fn reject_symlink_components(&self, path: &Path) -> Result<(), AppError> {
        let relative = path.strip_prefix(&self.root).map_err(|_| {
            AppError::new(
                "MCL_ARTIFACT_PATH_UNSAFE",
                "artifact path escaped its configured root",
                false,
                "Quarantine the artifact store and inspect its directory structure.",
            )
        })?;
        let mut current = self.root.clone();
        for component in relative.components() {
            current.push(component);
            if let Ok(metadata) = fs::symlink_metadata(&current)
                && metadata.file_type().is_symlink()
            {
                return Err(AppError::new(
                    "MCL_ARTIFACT_PATH_UNSAFE",
                    format!(
                        "symbolic link is forbidden in artifact path: {}",
                        current.display()
                    ),
                    false,
                    "Quarantine the artifact store and inspect its directory structure.",
                ));
            }
        }
        Ok(())
    }
}

fn read_directory(path: &Path, operation: &'static str) -> Result<Vec<PathBuf>, AppError> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| AppError::io(operation, error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| AppError::io(operation, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    Ok(entries)
}

fn require_real_directory(path: &Path, label: &str) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| AppError::io("inspect artifact directory", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::new(
            "MCL_ARTIFACT_PATH_UNSAFE",
            format!("{label} is not a real directory: {}", path.display()),
            false,
            "Quarantine the artifact store and inspect its directory structure.",
        ));
    }
    Ok(())
}

fn safe_file_name(path: &Path) -> Result<String, AppError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            AppError::new(
                "MCL_ARTIFACT_PATH_UNSAFE",
                format!("artifact path has a non-UTF-8 name: {}", path.display()),
                false,
                "Quarantine the artifact store and inspect its directory structure.",
            )
        })
}

fn require_hex_prefix(prefix: &str) -> Result<(), AppError> {
    if prefix.len() != 2
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::new(
            "MCL_ARTIFACT_PATH_UNSAFE",
            format!("artifact hash prefix `{prefix}` is malformed"),
            false,
            "Quarantine the artifact store and inspect its directory structure.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn artifacts_are_content_addressed_and_idempotent() {
        let temporary = TempDir::new().expect("temporary directory");
        let store = ArtifactStore::open(temporary.path()).expect("artifact store");
        let first = store.put(b"mathematics").expect("first write");
        let second = store.put(b"mathematics").expect("second write");
        assert_eq!(first, second);
        assert_eq!(store.read(&first).expect("read"), b"mathematics");
    }

    #[test]
    fn invalid_hash_cannot_escape_store() {
        let temporary = TempDir::new().expect("temporary directory");
        let store = ArtifactStore::open(temporary.path()).expect("artifact store");
        assert_eq!(
            store.read("../../etc/passwd").expect_err("reject").code,
            "MCL_ARTIFACT_HASH_INVALID"
        );
    }

    #[test]
    fn uppercase_hash_is_not_a_canonical_identity() {
        let temporary = TempDir::new().expect("temporary directory");
        let store = ArtifactStore::open(temporary.path()).expect("artifact store");
        assert_eq!(
            store
                .read(&"A".repeat(64))
                .expect_err("reject uppercase")
                .code,
            "MCL_ARTIFACT_HASH_INVALID"
        );
    }

    #[test]
    fn verified_bytes_materialize_only_to_a_fresh_plain_file() {
        let artifacts = TempDir::new().expect("artifact root");
        let workspace = TempDir::new().expect("workspace root");
        let store = ArtifactStore::open(artifacts.path()).expect("artifact store");
        let hash = store
            .put(b"theorem truth : True := by trivial\n")
            .expect("put");
        let materialized = store
            .materialize(&hash, workspace.path(), "Artifact.lean")
            .expect("materialize");
        assert_eq!(
            fs::read(materialized).expect("read materialized bytes"),
            b"theorem truth : True := by trivial\n"
        );
        assert_eq!(
            store
                .materialize(&hash, workspace.path(), "../escape.lean")
                .expect_err("path escape rejected")
                .code,
            "MCL_ARTIFACT_MATERIALIZATION_UNSAFE"
        );
        assert_eq!(
            store
                .materialize(&hash, workspace.path(), "Artifact.lean")
                .expect_err("overwrite rejected")
                .code,
            "MCL_ARTIFACT_MATERIALIZATION_EXISTS"
        );
    }

    #[test]
    fn scan_distinguishes_complete_content_from_crash_window_temporary_files() {
        let artifacts = TempDir::new().expect("artifact root");
        let store = ArtifactStore::open(artifacts.path()).expect("artifact store");
        let hash = store.put(b"orphan bytes").expect("orphan put");
        let destination = store.path_for_hash(&hash).expect("hash path");
        fs::write(
            destination
                .parent()
                .expect("hash parent")
                .join(".tmp-crash"),
            b"incomplete",
        )
        .expect("crash residue writes");
        assert_eq!(
            store.scan().expect("artifact scan"),
            ArtifactStoreScan {
                content_hashes: vec![hash],
                incomplete_temporary_files: 1,
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_root_is_rejected() {
        use std::os::unix::fs::symlink;

        let temporary = TempDir::new().expect("temporary directory");
        let actual = temporary.path().join("actual");
        fs::create_dir(&actual).expect("actual artifact root");
        let linked = temporary.path().join("linked");
        symlink(&actual, &linked).expect("artifact root symlink");

        assert_eq!(
            ArtifactStore::open(&linked)
                .expect_err("reject symlink root")
                .code,
            "MCL_ARTIFACT_ROOT_UNSAFE"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_inside_hash_path_is_rejected() {
        use std::os::unix::fs::symlink;

        let temporary = TempDir::new().expect("temporary directory");
        let outside = TempDir::new().expect("outside directory");
        let store = ArtifactStore::open(temporary.path()).expect("artifact store");
        let hash = format!("{:x}", Sha256::digest(b"escape"));
        let sha_root = temporary.path().join("sha256");
        fs::create_dir(&sha_root).expect("sha root");
        symlink(outside.path(), sha_root.join(&hash[0..2])).expect("nested symlink");

        assert_eq!(
            store
                .put(b"escape")
                .expect_err("reject nested symlink")
                .code,
            "MCL_ARTIFACT_PATH_UNSAFE"
        );
    }
}
