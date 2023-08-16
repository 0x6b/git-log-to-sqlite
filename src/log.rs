/// A library to interact with Git logs.
use std::fmt::Display;

/// Represents a Git log with various details from the commit.
#[derive(Debug)]
pub struct GitLog {
    /// Commit hash.
    pub commit_hash: String,
    /// Parent commit hash. If the commit is the first commit, this will be the zero hash.
    pub parent_hash: String,
    /// Name of the author.
    pub author_name: String,
    /// Email address of the author.
    pub author_email: String,
    /// Commit date time in UNIX epoch.
    pub commit_datetime: i64,
    /// Commit message, only summary (title).
    pub message: String,
    /// Number of insertions in the commit.
    pub insertions: usize,
    /// Number of deletions in the commit.
    pub deletions: usize,
    /// Changed files in the commit.
    pub changed_files: Vec<String>,
}

impl Display for GitLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "commit: {}\nparent: {}\nauthor: {}\nemail: {}\nsummary: {}\ndate: {}\ninsertions: {}\ndeletions: {}\nchanged files: {}",
            self.commit_hash,
            self.parent_hash,
            self.author_name,
            self.author_email,
            self.message,
            self.commit_datetime,
            self.insertions,
            self.deletions,
            self.changed_files.join(", ")
        )
    }
}
