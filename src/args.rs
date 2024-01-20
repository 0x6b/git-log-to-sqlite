use std::{collections::HashMap, error::Error, path::PathBuf};

use camino::Utf8PathBuf;
use clap::Parser;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use walkdir::WalkDir;

use crate::config::Config;

/// Command line arguments
#[derive(Parser)]
#[clap(about, version)]
pub struct Args {
    /// Path to the root directory to scan
    #[clap()]
    pub root: Utf8PathBuf,

    /// Recursively scan the root directory
    #[clap(short, long)]
    pub recursive: bool,

    /// Max depth of the recursive scan
    #[clap(short, long, default_value = "1")]
    pub max_depth: usize,

    /// Path to the database
    #[clap(short, long, default_value = "repositories.db")]
    pub database: Utf8PathBuf,

    /// Path to TOML configuration file
    #[clap(short = 'f', long, default_value = "config.toml")]
    pub config: Utf8PathBuf,

    /// Delete all records from the database before scanning
    #[clap(short, long)]
    pub clear: bool,

    /// Number of worker threads
    #[clap(short, long, default_value = "8")]
    pub num_threads: usize,

    /// List of directories to scan
    #[clap(skip)]
    pub dirs: Vec<PathBuf>,

    /// List of ignored repositories
    #[clap(skip)]
    pub ignored_repositories: Vec<String>,

    /// Email address and user name map to normalize the author name
    #[clap(skip)]
    pub author_map: Option<HashMap<String, String>>,
}

impl Args {
    pub fn new() -> Self {
        let args = Self::parse();
        let (dirs, ignored_repositories, author_map) = args.get_directories_to_scan();
        Args { dirs, ignored_repositories, author_map, ..args }
    }

    fn get_config(&self) -> Config {
        let config = &self.config;
        if config.exists() && config.is_file() {
            toml::from_str(&std::fs::read_to_string(config).unwrap()).unwrap()
        } else {
            Config::default()
        }
    }

    fn get_directories_to_scan(
        &self,
    ) -> (Vec<PathBuf>, Vec<String>, Option<HashMap<String, String>>) {
        let mut ignored = Vec::new();
        let config = &self.get_config();
        // let author_map = config.author_map.clone().unwrap_or_default();

        let dirs = if self.recursive {
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
                    if let Some(ignored_repositories) = &config.ignored_repositories {
                        if ignored_repositories.contains(&name) {
                            ignored.push(name);
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

        (dirs, ignored, config.author_map.clone())
    }

    pub fn prepare_database(
        &self,
        pool: &Pool<SqliteConnectionManager>,
    ) -> Result<(), Box<dyn Error>> {
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
