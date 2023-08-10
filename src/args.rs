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

    /// Path to the database
    #[clap(short, long, default_value = "repositories.db")]
    pub database: Utf8PathBuf,

    /// Path to JSON configuration file
    #[clap(short, long, default_value = "config.json")]
    pub config: Utf8PathBuf,
}

/// Configuration file
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// List of repositories to ignore
    pub ignored_repositories: Option<Vec<String>>,
}
