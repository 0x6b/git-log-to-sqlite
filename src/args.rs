use camino::Utf8PathBuf;
use clap::Parser;

#[derive(Parser)]
#[clap(about, version)]
pub struct Args {
    /// Path to the git repository
    #[clap()]
    pub paths: Vec<Utf8PathBuf>,

    /// Path to the database
    #[clap(short, long, default_value = "repositories.db")]
    pub database: Utf8PathBuf,
}
