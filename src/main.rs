use anyhow::Result;
use git_log_to_sqlite::GitRepositoryAnalyzer;

fn main() -> Result<()> {
    let analyzer = GitRepositoryAnalyzer::new().try_prepare()?;
    let (duration, analyzed_repositories, skipped_directories) = analyzer.analyze()?;
    println!("# Done in {duration} seconds\n");

    println!(
        "# {} repositories in the table\n\n{}\n",
        analyzed_repositories.len(),
        analyzed_repositories.join(", ")
    );
    println!(
        "# {} ignored repositories:\n\n{}\n",
        analyzer.ignored_repositories.len(),
        analyzer.ignored_repositories.join(", ")
    );

    if !skipped_directories.is_empty() {
        println!(
            "# {} directories were not stored for some reason. Maybe empty, or not a git repository?:\n\n{}",
            skipped_directories.len(),
            skipped_directories.join("\n")
        );
    }
    Ok(())
}
