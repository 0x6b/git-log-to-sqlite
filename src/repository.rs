use crate::{file::ChangedFile, log::GitLog};
use camino::Utf8PathBuf;
use git2::{DiffFindOptions, DiffOptions, Oid, Repository};
use std::{error::Error, path::PathBuf};

pub struct Uninitialized {
    path: PathBuf,
}
pub struct Opened {
    repo: Repository,
    head: Oid,
}

pub struct GitRepository<S> {
    state: S,
}

impl GitRepository<Uninitialized> {
    pub fn new(path: &Utf8PathBuf) -> Self {
        Self {
            state: Uninitialized {
                path: path.canonicalize().unwrap(),
            },
        }
    }
}

impl TryFrom<GitRepository<Uninitialized>> for GitRepository<Opened> {
    type Error = git2::Error;

    fn try_from(r: GitRepository<Uninitialized>) -> Result<Self, Self::Error> {
        let repo = Repository::open(r.state.path).expect("failed to open repository");
        let head = repo
            .head()
            .expect("failed to get HEAD")
            .target()
            .expect("failed to get OID to HEAD");
        Ok(Self {
            state: Opened { repo, head },
        })
    }
}

impl GitRepository<Opened> {
    pub fn get_logs(&self) -> Result<Vec<GitLog>, Box<dyn Error>> {
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
                            &mut DiffFindOptions::new()
                                .renames(true)
                                .copies(true)
                                .exact_match_only(true),
                        ))
                        .map(|_| {
                            let changed_files = diff
                                .deltas()
                                .map(|delta| ChangedFile::new(commit.id(), delta.new_file()))
                                .collect::<Vec<_>>();

                            let (insertions, deletions) = diff
                                .stats()
                                .map_or((0, 0), |stats| (stats.insertions(), stats.deletions()));

                            (insertions, deletions, changed_files.into())
                        })
                    })
                    .unwrap_or((0, 0, vec![].into()));

                GitLog {
                    commit_hash: commit.id().to_string(),
                    parent_hash: parent_oid.unwrap_or(Oid::zero()).to_string(),
                    author_name: commit.author().name().unwrap_or_default().to_string(),
                    author_email: commit.author().email().unwrap_or_default().to_string(),
                    commit_datetime: commit.time().seconds(),
                    message: commit.summary().unwrap_or_default().to_string(),
                    insertions,
                    deletions,
                    changed_files,
                }
            })
            .collect::<Vec<_>>();

        Ok(logs)
    }
}
