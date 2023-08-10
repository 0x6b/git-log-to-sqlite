use std::error::Error;

use crate::{
    args::Args,
    repository::{GitRepository, Opened, Uninitialized},
};
use clap::Parser;

mod args;
mod log;
mod repository;

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let repo: GitRepository<Opened> = GitRepository::<Uninitialized>::new(&args.path).try_into()?;
    let logs = repo.get_logs()?;
    logs.iter().for_each(|l| println!("{}", l));

    Ok(())
}
