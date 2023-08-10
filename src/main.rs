use std::error::Error;
use std::thread;

use clap::Parser;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;

use crate::{
    args::Args,
    repository::{GitRepository, Opened, Uninitialized},
};

mod args;
mod log;
mod repository;

fn main() -> Result<(), Box<dyn Error>> {
    let Args { paths, database } = Args::parse();
    let manager = SqliteConnectionManager::file(&database);
    let pool = Pool::new(manager)?;
    prepare_database_connection(&pool)?;

    let _ = paths
        .into_iter()
        .map(|path| {
            let pool = pool.clone();
            thread::spawn(move || {
                let repo: GitRepository<Opened> = GitRepository::<Uninitialized>::try_new(&path)
                    .unwrap()
                    .try_into()
                    .unwrap();
                let repo = repo.analyze().unwrap();
                let mut tx = pool.get().unwrap();
                let tx = tx.transaction().unwrap();
                tx.execute(
                    "INSERT OR IGNORE INTO repositories (name) VALUES (?1)",
                    params![repo.name()],
                ).unwrap();
                let repository_id = tx.last_insert_rowid();

                for log in repo.logs() {
                    tx.execute(
                        "INSERT INTO logs (commit_hash, parent_hash, author_name, author_email, commit_datetime, message, insertions, deletions, repository_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![log.commit_hash, log.parent_hash, log.author_name, log.author_email, log.commit_datetime, log.message, log.insertions as i64, log.deletions as i64, repository_id],
                    ).unwrap();

                    for path in &log.changed_files {
                        tx.execute(
                            "INSERT INTO changed_files (commit_hash, file_path) VALUES (?1, ?2)",
                            params![log.commit_hash, path],
                        ).unwrap();
                    }
                }

                tx.commit().unwrap();
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(thread::JoinHandle::join)
        .collect::<Vec<_>>();
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
