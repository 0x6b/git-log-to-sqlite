use std::{collections::HashMap, ops::Deref, path::PathBuf};

use anyhow::Result;
use camino::Utf8PathBuf;
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use walkdir::WalkDir;

use crate::{config::Config, repository::GitRepository};

/// A git repository analyzer. To prevent the impossible operation from executing (i.e. run analysis
/// before setting up the database, etc.), the analyzer must be successfully constructed before
/// analysis. The state transitions are as follows:
///
/// Uninitialized -> Prepared
pub struct GitRepositoryAnalyzer<S> {
    state: S,
}

/// Convenient deref implementation which returns the inner state.
impl<S> Deref for GitRepositoryAnalyzer<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

#[derive(Parser)]
#[clap(about, version)]
pub struct Uninitialized {
    /// Path to the root directory to scan
    #[arg()]
    pub root: Utf8PathBuf,

    /// Recursively scan the root directory
    #[arg(short, long)]
    pub recursive: bool,

    /// Max depth of the recursive scan
    #[arg(short, long, default_value = "1")]
    pub max_depth: usize,

    /// Path to the database
    #[arg(short, long, default_value = "repositories.db")]
    pub database: Utf8PathBuf,

    /// Path to TOML configuration file
    #[arg(short = 'f', long, default_value = "config.toml")]
    pub config: Utf8PathBuf,

    /// Delete all records from the database before scanning
    #[arg(short, long)]
    pub clear: bool,

    /// Number of worker threads
    #[arg(short, long, default_value = "8")]
    pub num_threads: usize,
}

pub struct Prepared {
    /// Number of worker threads
    pub num_threads: usize,

    /// Database connection pool
    pub pool: Pool<SqliteConnectionManager>,

    /// List of directories to scan
    pub directories: Vec<PathBuf>,

    /// List of ignored repositories
    pub ignored_repositories: Vec<String>,

    /// Email address and user name map to normalize the author name
    pub author_map: Option<HashMap<String, String>>,
}

impl GitRepositoryAnalyzer<Uninitialized> {
    pub fn new() -> Self {
        GitRepositoryAnalyzer { state: Uninitialized::parse() }
    }

    pub fn try_prepare(self) -> Result<GitRepositoryAnalyzer<Prepared>> {
        let (directories, ignored_repositories, author_map) = self.get_directories_to_scan();
        let pool = Pool::new(SqliteConnectionManager::file(&self.database))?;
        self.prepare_database(&pool)?;

        Ok(GitRepositoryAnalyzer {
            state: Prepared {
                num_threads: self.num_threads,
                pool,
                directories,
                ignored_repositories,
                author_map,
            },
        })
    }

    fn get_directories_to_scan(
        &self,
    ) -> (Vec<PathBuf>, Vec<String>, Option<HashMap<String, String>>) {
        let mut ignored_repositories = Vec::new();
        let config = &self.get_config();

        let directories = if self.recursive {
            WalkDir::new(&self.root)
                .max_depth(self.max_depth)
                .into_iter()
                .skip(1) // skip root directory
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_dir())
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name == ".git" {
                        return false;
                    }
                    if let Some(ir) = &config.ignored_repositories {
                        if ir.contains(&name) {
                            ignored_repositories.push(name);
                            return false;
                        }
                    }
                    true
                })
                .map(|e| e.path().to_owned())
                .collect::<Vec<_>>()
        } else {
            vec![self.root.clone().into()]
        };

        (directories, ignored_repositories, config.author_map.clone())
    }

    fn get_config(&self) -> Config {
        let config = &self.config;
        if config.exists() && config.is_file() {
            toml::from_str(&std::fs::read_to_string(config).unwrap()).unwrap()
        } else {
            Config::default()
        }
    }

    pub fn prepare_database(&self, pool: &Pool<SqliteConnectionManager>) -> Result<()> {
        let conn = pool.get()?;

        conn.execute(
            r#"
        CREATE TABLE IF NOT EXISTS repositories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            url TEXT
        )
        "#,
            [],
        )?;

        conn.execute(
            r#"
        CREATE TABLE IF NOT EXISTS logs (
            commit_hash TEXT PRIMARY KEY,
            author_name TEXT NOT NULL,
            author_email TEXT NOT NULL,
            message TEXT,
            commit_datetime DATETIME NOT NULL,
            insertions INTEGER,
            deletions INTEGER,
            repository_id INTEGER,
            parent_hash TEXT,
            FOREIGN KEY (repository_id) REFERENCES repositories (id)
        )
        "#,
            [],
        )?;

        conn.execute(
            r#"
        CREATE TABLE IF NOT EXISTS changed_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            commit_hash TEXT NOT NULL,
            file_path TEXT,
            FOREIGN KEY (commit_hash) REFERENCES logs (commit_hash)
        )
        "#,
            [],
        )?;

        if self.clear {
            conn.execute("DELETE FROM repositories", [])?;
            conn.execute("DELETE FROM logs", [])?;
            conn.execute("DELETE FROM changed_files", [])?;
        }

        Ok(())
    }
}

impl GitRepositoryAnalyzer<Prepared> {
    /// Analyze the git repositories and return the elapsed time in seconds, vec of analyzed repos,
    /// and skipped directories
    pub fn analyze(&self) -> Result<(f64, Vec<String>, Vec<String>)> {
        let mut tasks = Vec::new();
        let m = MultiProgress::new();

        let overall_progress = m.add(ProgressBar::new(self.directories.len() as u64));
        overall_progress.set_style(
            ProgressStyle::with_template(
                "{prefix:<30!.blue} [{bar:40.cyan/blue}] {pos:>3}/{len:3} [{elapsed_precise}]",
            )
            .unwrap()
            .progress_chars("=> "),
        );
        overall_progress.set_prefix("OVERALL PROGRESS");

        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(self.num_threads)
            .build()
            .unwrap()
            .block_on(async {
                for path in &self.directories {
                    tasks.push(tokio::spawn(Self::exec(
                        path.clone(),
                        self.author_map.clone(),
                        self.pool.clone(),
                        m.clone(),
                        overall_progress.clone(),
                    )));
                }

                for task in tasks {
                    task.await.unwrap();
                }
            });

        overall_progress.finish_and_clear();
        let (analyzed_repositories, skipped_directories) = self.get_repositories()?;
        Ok((
            overall_progress.elapsed().as_millis() as f64 / 1000.0,
            analyzed_repositories,
            skipped_directories,
        ))
    }

    /// Get the list of analyzed repositories and the list of directories ignored
    fn get_repositories(&self) -> Result<(Vec<String>, Vec<String>)> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT name FROM repositories ORDER BY name")?;
        let analyzed_repositories = stmt
            .query_map(params![], |row| row.get::<_, String>(0))?
            .filter_map(|name| name.ok())
            .collect::<Vec<_>>();

        let skipped_directories = self
            .directories
            .iter()
            .filter(|e| {
                !analyzed_repositories
                    .contains(&e.file_name().unwrap().to_string_lossy().to_string())
            })
            .map(|e| e.display().to_string())
            .collect::<Vec<_>>();

        Ok((analyzed_repositories, skipped_directories))
    }

    async fn exec(
        path: PathBuf,
        author_map: Option<HashMap<String, String>>,
        pool: Pool<SqliteConnectionManager>,
        m: MultiProgress,
        overall_progress: ProgressBar,
    ) {
        let pb = m.add(ProgressBar::new(1));
        pb.set_style(
            ProgressStyle::with_template("{prefix:<30!} [{bar:40}] {pos:>3}/{len:3} {msg}")
                .unwrap()
                .progress_chars("-> "),
        );
        pb.set_prefix(format!("- {}", path.file_name().unwrap().to_string_lossy()));
        pb.set_length(4); // opening, analyzing, storing (repo, logs), done

        GitRepository::<crate::repository::Uninitialized>::try_new(path)
            .and_then(|uninitialized| {
                pb.set_message("opening");
                pb.inc(1);
                uninitialized.open()
            })
            .and_then(|opened| {
                pb.set_message("analyzing");
                pb.inc(1);
                opened.analyze(author_map)
            })
            .and_then(|repo| {
                overall_progress.inc(1);
                pb.set_message("storing into repositories table");
                pb.inc(1);
                let mut conn = pool.get()?;
                conn.execute(
                    "INSERT OR IGNORE INTO repositories (name, url) VALUES (?1, ?2)",
                    params![repo.name(), repo.url()],
                )?;

                let tx = conn.transaction()?;
                pb.set_message(format!("storing {} logs", repo.logs().len()));
                pb.inc(1);
                for log in repo.logs() {
                    tx.execute(
                        r#"
                        INSERT INTO logs (
                            commit_hash,
                            parent_hash,
                            author_name,
                            author_email,
                            commit_datetime,
                            message,
                            insertions,
                            deletions,
                            repository_id
                        )
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, (SELECT id FROM repositories WHERE name = ?));
                        "#,
                        params![
                        log.commit_hash,
                        log.parent_hash,
                        log.author_name,
                        log.author_email,
                        log.commit_datetime,
                        log.message,
                        log.insertions as i64,
                        log.deletions as i64,
                        repo.name()
                    ],
                    )?;

                    pb.set_message(format!("storing {} changed files", log.changed_files.len()));
                    for path in &log.changed_files {
                        tx.execute(
                            "INSERT INTO changed_files (commit_hash, file_path) VALUES (?1, ?2)",
                            params![log.commit_hash, path],
                        )?;
                    }
                }

                tx.commit()?;
                pb.set_message("done");
                pb.finish_and_clear();
                Ok(())
            })
            .ok();
    }
}
