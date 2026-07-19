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
