use std::{fs, path::PathBuf};

use bytes::Bytes;
use once_cell::sync::OnceCell;
use regex::Regex;
use tokio::io;

use crate::models::WorkerConfig;

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

        VALIDATE_REGEX.get().unwrap()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodeStoreError {
    #[error("invalid worker name {0:?}")]
    InvalidName(String),

    #[error(transparent)]
    IoError(#[from] io::Error),

    #[error(transparent)]
    SerializationError(#[from] serde_json::Error),
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

        let store = Self {
            parent,
            workers_path,
        };
        store.check_fs();

        store
    }

    /// Check the filesystem.
    /// If the required directory for storing workers does not exist,
    /// a new one is created.
    ///
    /// Returns the path for storing workers.
    #[inline]
    pub fn check_fs(&self) -> &PathBuf {
        if !self.workers_path.exists() {
            fs::create_dir_all(&self.workers_path).ok();
            fs::write(&self.parent.join(".gitignore"), "*").ok();
        }

        &self.workers_path
    }

    #[inline(always)]
    pub async fn upload_worker(
        &self,
        code: Bytes,
        config: WorkerConfig,
    ) -> Result<(), CodeStoreError> {
        let name = &config.name;
        if !get_validate_regex().is_match(name) {
            return Err(CodeStoreError::InvalidName(name.clone()));
        }

        let base = self.check_fs();
        let js_path = base.join(format!("{}.js", &name));
        let config_path = base.join(format!("{}.cfg", &name));

        tokio::fs::write(&js_path, code).await?;
        tokio::fs::write(&config_path, serde_json::to_vec(&config)?).await?;

        Ok(())
    }

    #[inline(always)]
    pub async fn remove_worker(&self, name: &str) {
        let path = self.check_fs().join(format!("{}.js", &name));
        fs::remove_file(path).ok();
    }

    #[inline(always)]
    pub async fn get_worker(&self, name: &str) -> Option<WorkerOnDisk> {
        let base = self.check_fs();
        let maybe_worker_code = {
            let path = base.join(format!("{}.js", &name));
            tokio::fs::read_to_string(path).await
        }
        .ok();
        let maybe_worker_config = {
            let path = base.join(format!("{}.cfg", &name));
            tokio::fs::read(path).await
        }
        .ok()
        .and_then(|v| match serde_json::from_slice(&v) {
            Ok(t) => Some(t),
            Err(err) => {
                tracing::error!("error when reading config: {err:?}");
                None
            }
        });

        maybe_worker_code
            .and_then(|code| maybe_worker_config.map(|config| WorkerOnDisk { code, config }))
    }
}

pub struct WorkerOnDisk {
    pub code: String,
    pub config: WorkerConfig,
}
