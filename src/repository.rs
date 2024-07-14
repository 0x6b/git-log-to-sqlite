use std::{collections::HashMap, ops::Deref, path::PathBuf};

use anyhow::{anyhow, Result};
use camino::Utf8PathBuf;
use git2::{DiffFindOptions, DiffOptions, Oid, Repository};

use crate::log::GitLog;

/// A git repository that can be used to analyze the commit history of a git repository. To prevent
/// the impossible operation from executing (i.e. run analysis before properly opening it, or
/// getting logs before analyzing it, etc.), the repository must be successfully opened before it
/// can be used to analysis.
///
/// The state of the repository is represented by the type parameter `S`. The state transitions are
/// as follows:
///
/// Uninitialized -> Opened -> Analyzed
pub struct GitRepository<S> {
    state: S,
}

/// Convenient deref implementation which returns the inner state.
impl<S> Deref for GitRepository<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

/// The initial state of the git repository.
pub struct Uninitialized {
    name: String,
    path: PathBuf,
}

/// The state of the git repository after it has been opened. After successful opening, we can use
/// the repository to analyze the commit history.
pub struct Opened {
    name: String,
    repo: Repository,
    head: Oid,
}

/// The state of the git repository after it has been analyzed. After successful analysis, we can
/// use the repository to get the commit history.
pub struct Analyzed {
    name: String,
    url: String,
    logs: Vec<GitLog>,
}

impl GitRepository<Uninitialized> {
    /// Creates a new git repository with the specified path. `path` must be a valid directory.
    pub fn try_new(path: PathBuf) -> Result<Self> {
        let path = Utf8PathBuf::from_path_buf(path).unwrap();
        if path.is_file() {
            return Err(anyhow!("Specified path is not a directory"));
        }

        let name = match path.file_name() {
            Some(name) => name.to_string(),
            None => {
                return Err(anyhow!("Specified path is invalid"));
            }
        };

        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return Err(anyhow!("Specified path does not exist"));
            }
        };

        Ok(Self { state: Uninitialized { path, name } })
    }

    pub fn open(self) -> Result<GitRepository<Opened>> {
        self.try_into()
    }
}

/// Tries to open the git repository. If successful, returns a `GitRepository<Opened>`.
impl TryFrom<GitRepository<Uninitialized>> for GitRepository<Opened> {
    type Error = anyhow::Error;

    fn try_from(r: GitRepository<Uninitialized>) -> Result<Self, Self::Error> {
        let repo = Repository::open(&r.path)?;
        let head = repo
            .head()?
            .target()
            .ok_or(git2::Error::from_str("failed to get OID to HEAD"))?;
        Ok(Self { state: Opened { repo, name: r.name.clone(), head } })
    }
}

impl GitRepository<Opened> {
    /// Analyzes the commit history of the git repository. If successful, returns a
    /// `GitRepository<Analyzed>`.
    pub fn analyze(
        &self,
        author_map: Option<HashMap<String, String>>,
    ) -> Result<GitRepository<Analyzed>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.set_sorting(git2::Sort::TIME)?;
        revwalk.push(self.head)?;

        let commits = revwalk
            .filter_map(|oid| oid.ok())
            .map(|oid| self.repo.find_commit(oid))
            .filter_map(|commit| commit.ok())
            .filter(|commit| commit.parent_count() < 2) // ignore merge commits
            .filter(|commit| commit.tree().is_ok())
            .collect::<Vec<_>>();

        let logs = commits
            .iter()
            .map(|commit| {
                let parent_oid = (commit.parent_count() != 0)
                    .then(|| commit.parent_id(0))
                    .transpose()
                    .ok()
                    .flatten(); // if commit has no parent (is a root), return None

                let parent_tree = parent_oid
                    .and_then(|oid| self.repo.find_commit(oid).ok())
                    .and_then(|parent_commit| parent_commit.tree().ok());

                let (insertions, deletions, changed_files) = self
                    .repo
                    .diff_tree_to_tree(
                        parent_tree.as_ref(),
                        Some(&commit.tree().unwrap()),
                        Some(
                            DiffOptions::new()
                                .disable_pathspec_match(true)
                                .ignore_submodules(true)
                                .include_typechange(true),
                        ),
                    )
                    .and_then(|mut diff| {
                        diff.find_similar(Some(
                            &mut DiffFindOptions::new()
                                .renames(true)
                                .copies(true)
                                .exact_match_only(true),
                        ))
                        .map(|_| {
                            let changed_files = diff
                                .deltas()
                                .map(|delta| delta.new_file().path().unwrap().display().to_string())
                                .collect::<Vec<_>>();

                            let (insertions, deletions) = diff
                                .stats()
                                .map_or((0, 0), |stats| (stats.insertions(), stats.deletions()));

                            (insertions, deletions, changed_files)
                        })
                    })
                    .unwrap_or((0, 0, vec![]));

                let mut author_name =
                    commit.author().name().unwrap_or("(no author name)").to_string();
                let author_email =
                    commit.author().email().unwrap_or("(no author email)").to_string();
                if let Some(map) = &author_map {
                    if let Some(name) = map.get(&author_email) {
                        author_name = name.clone();
                    }
                }

                GitLog {
                    commit_hash: commit.id().to_string(),
                    parent_hash: parent_oid.unwrap_or(Oid::zero()).to_string(),
                    author_name,
                    author_email,
                    commit_datetime: commit.time().seconds(),
                    message: commit.summary().unwrap_or("(no commit summary)").to_string(),
                    insertions,
                    deletions,
                    changed_files,
                }
            })
            .collect::<Vec<_>>();

        let url = self
            .repo
            .find_remote("origin")
            .ok()
            .and_then(|remote| remote.url().map(|url| url.to_string()).or(None))
            .unwrap_or("(no remote url)".to_string())
            .replace("git@github.com:", "https://github.com/");

        Ok(GitRepository {
            state: Analyzed { name: self.name.clone(), url, logs },
        })
    }
}

impl GitRepository<Analyzed> {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Finally we can get the logs! after initializing, opening, analyzing the git repository.
    pub fn logs(&self) -> &Vec<GitLog> {
        &self.logs
    }
}
