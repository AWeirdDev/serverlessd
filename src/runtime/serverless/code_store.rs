use std::{collections::HashMap, fs, path::PathBuf};

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
                Regex::new(r"^[^-][0-9A-Za-z-]*$")
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

pub struct CodeStore {
    pub(super) uploads: HashMap<String, PathBuf>,
}

impl CodeStore {
    #[inline]
    pub fn new() -> Self {
        Self {
            uploads: HashMap::new(),
        }
    }

    /// Check the filesystem.
    /// If the required directory (`.serverlessd/workers`) does not exist,
    /// a new one is created.
    #[inline]
    pub async fn check_fs(&self) -> PathBuf {
        let parent = PathBuf::from(".serverlessd/");
        let path = parent.join("workers/");

        if !path.exists() {
            fs::create_dir_all(&path).ok();
            tokio::fs::write(&parent.join(".gitignore"), "*").await.ok();
        }

        path
    }

    #[inline(always)]
    pub(super) async fn upload_worker_code(
        &mut self,
        name: String,
        code: String,
    ) -> Result<(), CodeStoreError> {
        if !get_validate_regex().is_match(&name) {
            return Err(CodeStoreError::InvalidName(name));
        }

        let path = self.check_fs().await.join(format!("{}.js", &name));
        tokio::fs::write(&path, code)
            .await
            .map_err(|err| CodeStoreError::IoError(err))?;
        self.uploads.insert(name, path);

        Ok(())
    }

    #[inline(always)]
    pub(super) async fn remove_worker_code(&mut self, name: &str) {
        let res = self.uploads.remove(name);

        // the worker exists, fuh
        if res.is_some() {
            let path = self.check_fs().await.join(format!("{}.js", &name));
            fs::remove_file(path).ok();
        }
    }
}
