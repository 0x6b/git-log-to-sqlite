use std::error::Error;

use crate::analyzer::GitRepositoryAnalyzer;

mod analyzer;
mod config;
mod log;
mod repository;

fn main() -> Result<(), Box<dyn Error>> {
    let analyzer = GitRepositoryAnalyzer::new().try_prepare()?;
    let duration = analyzer.analyze()?;
    println!("# Done in {duration} seconds\n");

    let (repositories, not_stored_dirs) = analyzer.get_repositories()?;
    println!("# {} repositories in the table\n\n{}\n", repositories.len(), repositories.join(", "));
    println!(
        "# {} ignored repositories:\n\n{}\n",
        analyzer.ignored_repositories.len(),
        analyzer.ignored_repositories.join(", ")
    );

    if !not_stored_dirs.is_empty() {
        println!(
            "# {} directories were not stored for some reason. Maybe empty, or not a git repository?:\n{}",
            not_stored_dirs.len(),
            not_stored_dirs.join("\n")
        );
    }
    Ok(())
}
