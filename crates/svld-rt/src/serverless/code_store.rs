use std::{fs, path::PathBuf};

use bytes::Bytes;
use once_cell::sync::OnceCell;
use regex::Regex;
use tokio::io;

static VALIDATE_REGEX: OnceCell<Regex> = OnceCell::new();

fn get_validate_regex() -> &'static Regex {
    if let Some(validate) = VALIDATE_REGEX.get() {
        validate
    } else {
        VALIDATE_REGEX
            .set(
                Regex::new(r"^[0-9A-Za-z-.]+$")
                    .expect("failed to compile worker name validation regex"),
            )
            .ok();
        unsafe { VALIDATE_REGEX.get().unwrap_unchecked() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodeStoreError {
    #[error("invalid worker name {0:?}")]
    InvalidName(String),

    #[error("io error {0:#?}")]
    IoError(io::Error),
}

/// Worker code store, using the filesystem.
#[derive(Debug)]
pub struct CodeStore {
    parent: PathBuf,
    workers_path: PathBuf,
}

impl CodeStore {
    /// Creates a new store for worker code.
    ///
    /// # Parameters
    /// - `parent`: The parent path. For instance, `.serverlessd`.
    /// - `workers_path`: The worker path *name*.
    ///     For instance, providing `workers` can get you `.serverlessd/workers`,
    ///     depending on the `parent` parameter.
    #[inline]
    pub fn new<P: Into<PathBuf>>(parent: P, workers_path: P) -> Self {
        let parent = parent.into();
        let workers_path = parent.join(workers_path.into());
        Self {
            parent,
            workers_path,
        }
    }

    /// Check the filesystem.
    /// If the required directory for storing workers does not exist,
    /// a new one is created.
    ///
    /// Returns the path for storing workers.
    #[inline]
    pub async fn check_fs(&self) -> &PathBuf {
        if !self.workers_path.exists() {
            fs::create_dir_all(&self.workers_path).ok();
            tokio::fs::write(&self.parent.join(".gitignore"), "*")
                .await
                .ok();
        }

        &self.workers_path
    }

    #[inline(always)]
    pub async fn upload_worker_code(
        &self,
        name: String,
        code: Bytes,
    ) -> Result<(), CodeStoreError> {
        if !get_validate_regex().is_match(&name) {
            return Err(CodeStoreError::InvalidName(name));
        }

        let path = self.check_fs().await.join(format!("{}.js", &name));
        tokio::fs::write(&path, code)
            .await
            .map_err(|err| CodeStoreError::IoError(err))?;

        Ok(())
    }

    #[inline(always)]
    pub async fn remove_worker_code(&self, name: &str) {
        let path = self.check_fs().await.join(format!("{}.js", &name));
        fs::remove_file(path).ok();
    }

    #[inline(always)]
    pub async fn get_worker_code(&self, name: &str) -> Option<String> {
        let path = self.check_fs().await.join(format!("{}.js", &name));
        tokio::fs::read_to_string(path).await.ok()
    }
}
