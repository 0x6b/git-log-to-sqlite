use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
};

use git2::{DiffFile, Oid};

#[derive(Debug)]
pub struct ChangedFile {
    #[allow(unused)]
    commit_hash: String,
    path: String,
}

impl ChangedFile {
    pub fn new(commit_hash: Oid, file: DiffFile) -> Self {
        Self {
            commit_hash: commit_hash.to_string(),
            path: file.path().unwrap().display().to_string(),
        }
    }
}

#[derive(Debug)]
pub struct ChangedFiles {
    inner: Vec<ChangedFile>,
}

impl Deref for ChangedFiles {
    type Target = Vec<ChangedFile>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ChangedFiles {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Display for ChangedFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}",
            self.inner
                .iter()
                .map(|f| f.path.clone())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl From<Vec<ChangedFile>> for ChangedFiles {
    fn from(files: Vec<ChangedFile>) -> Self {
        Self { inner: files }
    }
}
