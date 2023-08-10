use camino::Utf8PathBuf;
use std::error::Error;

use clap::Parser;
use rusqlite::{params, Connection};

use crate::{
    args::Args,
    repository::{GitRepository, Opened, Uninitialized},
};

mod args;
mod log;
mod repository;

fn main() -> Result<(), Box<dyn Error>> {
    let Args { paths, database } = Args::parse();

    for path in paths {
        let repo: GitRepository<Opened> = GitRepository::<Uninitialized>::try_new(&path)?.try_into()?;
        let repo = repo.analyze()?;

        if let Ok(mut conn) = prepare_database_connection(&database) {
            let tx = conn.transaction()?;
            tx.execute(
                "INSERT OR IGNORE INTO repositories (name) VALUES (?1)",
                params![repo.name()],
            )?;
            let repository_id = tx.last_insert_rowid();

            for log in repo.logs() {
                tx.execute(
                    "INSERT INTO logs (commit_hash, parent_hash, author_name, author_email, commit_datetime, message, insertions, deletions, repository_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![log.commit_hash, log.parent_hash, log.author_name, log.author_email, log.commit_datetime, log.message, log.insertions as i64, log.deletions as i64, repository_id],
                )?;

                for path in &log.changed_files {
                    tx.execute(
                        "INSERT INTO changed_files (commit_hash, file_path) VALUES (?1, ?2)",
                        params![log.commit_hash, path],
                    )?;
                }
            }

            tx.commit()?;
            conn.close().unwrap();
        }
    }

    Ok(())
}

fn prepare_database_connection(path: &Utf8PathBuf) -> Result<Connection, Box<dyn Error>> {
    let conn = Connection::open(path)?;

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

    Ok(conn)
}
