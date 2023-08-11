use std::{error::Error, path::PathBuf};

use clap::Parser;
use indicatif::{MultiProgress, ProgressStyle};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use walkdir::WalkDir;

use crate::{
    args::{Args, Config},
    repository::{GitRepository, Uninitialized},
};

mod args;
mod log;
mod repository;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<(), Box<dyn Error>> {
    let Args {
        root,
        recursive,
        max_depth,
        database,
        config,
    } = Args::parse();

    let mut tasks = Vec::new();
    let m = MultiProgress::new();

    let config = if config.exists() && config.is_file() {
        serde_json::from_str::<Config>(&std::fs::read_to_string(&config)?)?
    } else {
        Config::default()
    };

    let dirs = if recursive {
        WalkDir::new(&root)
            .max_depth(max_depth)
            .into_iter()
            .skip(1) // skip root directory
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
            .filter(|e| e.file_name().to_string_lossy() != ".git")
            .filter(|e| {
                if let Some(ignored_repositories) = &config.ignored_repositories {
                    let name = e.file_name().to_string_lossy().to_string();
                    !ignored_repositories.contains(&name)
                } else {
                    true
                }
            })
            .map(|e| e.path().to_owned())
            .collect::<Vec<_>>()
    } else {
        vec![root.into()]
    };

    let manager = SqliteConnectionManager::file(&database);
    let pool = Pool::new(manager)?;
    prepare_database_connection(&pool)?;

    for path in dirs {
        let pool = pool.clone();

        tasks.push(tokio::spawn(exec(path, pool, m.clone())));
    }

    for task in tasks {
        task.await?;
    }

    Ok(())
}

async fn exec(path: PathBuf, pool: Pool<SqliteConnectionManager>, m: MultiProgress) {
    let pb = m.add(indicatif::ProgressBar::new(1));
    pb.set_style(
        ProgressStyle::with_template("{prefix:<30!.blue} {bar:40.cyan/blue} {pos:>3}/{len:3} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );
    pb.set_prefix(path.file_name().unwrap().to_string_lossy().to_string());
    pb.set_length(5); // opening, analyzing, storing (repo, logs, files), done

    GitRepository::<Uninitialized>::try_new(path)
        .and_then(|uninitialized| {
            pb.set_message("opening");
            pb.inc(1);
            uninitialized.open()
        })
        .and_then(|opened| {
            pb.set_message("analyzing");
            pb.inc(1);
            opened.analyze()
        })
        .and_then(|repo| {
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
                pb.inc(1);
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

fn prepare_database_connection(pool: &Pool<SqliteConnectionManager>) -> Result<(), Box<dyn Error>> {
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

    Ok(())
}
