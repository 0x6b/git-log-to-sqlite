use std::{error::Error, path::PathBuf};

use camino::Utf8PathBuf;
use git2::{DiffFindOptions, DiffOptions, Oid, Repository};

use crate::log::GitLog;

pub struct Uninitialized {
    name: String,
    path: PathBuf,
}
pub struct Opened {
    name: String,
    repo: Repository,
    head: Oid,
}

pub struct Analyzed {
    name: String,
    logs: Vec<GitLog>,
}

pub struct GitRepository<S> {
    state: S,
}

impl GitRepository<Uninitialized> {
    pub fn try_new(path: &Utf8PathBuf) -> Result<Self, Box<dyn Error>> {
        if path.is_file() {
            return Err("Specified path is not a directory".into());
        }

        let name = match path.file_name() {
            Some(name) => name.to_string(),
            None => {
                return Err("Specified path is invalid".into());
            }
        };

        let path = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return Err("Specified path does not exist".into());
            }
        };

        Ok(Self {
            state: Uninitialized { path, name },
        })
    }
}

impl TryFrom<GitRepository<Uninitialized>> for GitRepository<Opened> {
    type Error = git2::Error;

    fn try_from(r: GitRepository<Uninitialized>) -> Result<Self, Self::Error> {
        let repo = Repository::open(r.state.path)?;
        let head = repo
            .head()?
            .target()
            .ok_or(git2::Error::from_str("failed to get OID to HEAD"))?;
        Ok(Self {
            state: Opened {
                repo,
                name: r.state.name,
                head,
            },
        })
    }
}

impl GitRepository<Opened> {
    pub fn analyze(&self) -> Result<GitRepository<Analyzed>, Box<dyn Error>> {
        let mut revwalk = self.state.repo.revwalk()?;
        revwalk.set_sorting(git2::Sort::TIME)?;
        revwalk.push(self.state.head)?;

        let commits = revwalk
            .filter_map(|oid| oid.ok())
            .map(|oid| self.state.repo.find_commit(oid))
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
                    .and_then(|oid| self.state.repo.find_commit(oid).ok())
                    .and_then(|parent_commit| parent_commit.tree().ok());

                let (insertions, deletions, changed_files) = self
                    .state
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
                            &mut DiffFindOptions::new().renames(true).copies(true).exact_match_only(true),
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

                GitLog {
                    commit_hash: commit.id().to_string(),
                    parent_hash: parent_oid.unwrap_or(Oid::zero()).to_string(),
                    author_name: commit.author().name().unwrap_or("(no author name)").to_string(),
                    author_email: commit.author().email().unwrap_or("(no author email)").to_string(),
                    commit_datetime: commit.time().seconds(),
                    message: commit.summary().unwrap_or("(no commit summary)").to_string(),
                    insertions,
                    deletions,
                    changed_files,
                }
            })
            .collect::<Vec<_>>();

        Ok(GitRepository {
            state: Analyzed {
                name: self.state.name.clone(),
                logs,
            },
        })
    }
}

impl GitRepository<Analyzed> {
    pub fn name(&self) -> &str {
        &self.state.name
    }

    pub fn logs(&self) -> &Vec<GitLog> {
        &self.state.logs
    }
}
