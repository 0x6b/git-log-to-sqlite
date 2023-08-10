use camino::Utf8PathBuf;
use clap::Parser;

#[derive(Parser)]
#[clap(about, version)]
pub struct Args {
    /// Path to the git repository
    #[clap()]
    pub path: Utf8PathBuf,
}
