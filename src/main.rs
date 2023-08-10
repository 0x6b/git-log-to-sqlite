use std::{error::Error, sync::mpsc, thread};

use clap::Parser;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use walkdir::WalkDir;

use crate::{
    args::Args,
    repository::{GitRepository, Opened, Uninitialized},
};

mod args;
mod log;
mod repository;

fn main() -> Result<(), Box<dyn Error>> {
    let Args { root, database } = Args::parse();
    let manager = SqliteConnectionManager::file(&database);
    let pool = Pool::new(manager)?;
    prepare_database_connection(&pool)?;

    let dirs = WalkDir::new(&root)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .skip(1) // skip root directory
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .filter(|e| e.file_name() != "router-backup")
        .map(|e| e.path().to_owned())
        .collect::<Vec<_>>();

    let (sender, receiver) = mpsc::channel();

    for path in dirs {
        let pool = pool.clone();
        let sender = sender.clone();

        thread::spawn(move || {
            sender.send(format!("Processing {}", &path)).unwrap();
            let repo: GitRepository<Opened> = GitRepository::<Uninitialized>::try_new(&path)
                .unwrap()
                .try_into()
                .unwrap();
            let repo = repo.analyze().unwrap();

            sender.send(format!("Processed {}", &repo.name())).unwrap();

            let mut conn = pool.get().unwrap();
            conn.execute(
                "INSERT OR IGNORE INTO repositories (name) VALUES (?1)",
                params![repo.name()],
            )
            .unwrap();

            let tx = conn.transaction().unwrap();
            sender.send("Storing logs into SQLite database".to_string()).unwrap();
            for log in repo.logs() {
                tx.execute(
                    r#"
INSERT INTO logs (commit_hash, parent_hash, author_name, author_email, commit_datetime, message, insertions, deletions, repository_id)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, (SELECT id FROM repositories WHERE name = ?));
"#,
                    params![log.commit_hash, log.parent_hash, log.author_name, log.author_email, log.commit_datetime, log.message, log.insertions as i64, log.deletions as i64, repo.name()],
                ).unwrap();

                for path in &log.changed_files {
                    tx.execute(
                        "INSERT INTO changed_files (commit_hash, file_path) VALUES (?1, ?2)",
                        params![log.commit_hash, path],
                    )
                    .unwrap();
                }
            }

            tx.commit().unwrap();
        });
    }

    drop(sender);

    for received in receiver {
        println!("{received}");
    }

    Ok(())
}

fn prepare_database_connection(pool: &Pool<SqliteConnectionManager>) -> Result<(), Box<dyn Error>> {
    let conn = pool.get()?;

    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS repositories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL
        )
        "#,
        [],
    )?;

    conn.execute(
        r#"
        CREATE TABLE IF NOT EXISTS logs (
            commit_hash TEXT PRIMARY KEY,
            parent_hash TEXT,
            author_name TEXT NOT NULL,
            author_email TEXT NOT NULL,
            message TEXT,
            commit_datetime DATETIME NOT NULL,
            insertions INTEGER,
            deletions INTEGER,
            repository_id INTEGER,
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
