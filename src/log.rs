use std::fmt::Display;

#[derive(Debug)]
pub struct GitLog {
    pub commit_hash: String,
    pub parent_hash: String,
    pub author_name: String,
    pub author_email: String,
    pub commit_datetime: i64,
    pub message: String,
    pub insertions: usize,
    pub deletions: usize,
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
