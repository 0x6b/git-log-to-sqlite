use camino::Utf8PathBuf;
use clap::Parser;
use serde_derive::Deserialize;

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

    /// Path to JSON configuration file
    #[clap(short = 'f', long, default_value = "config.json")]
    pub config: Utf8PathBuf,

    /// Delete all records from the database before scanning
    #[clap(short, long)]
    pub clear: bool,
}

/// Configuration file
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// List of repositories to ignore
    pub ignored_repositories: Option<Vec<String>>,
}
