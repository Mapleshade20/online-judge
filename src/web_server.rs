use actix_web::{App, HttpServer, dev::Server, middleware, web};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::config::Config;
use crate::routes::{JobMessage, exit, json_error_handler, post_jobs_handler};

pub fn build_server(
    config: Config,
    db_pool: SqlitePool,
    queue_tx: mpsc::Sender<JobMessage>,
) -> std::io::Result<Server> {
    let Config {
        server: server_config,
        problems,
        languages,
    } = config;
    let db_pool = web::Data::new(db_pool);
    let problems = web::Data::new(problems);
    let languages = web::Data::new(languages);
    let queue_tx = web::Data::new(queue_tx);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(db_pool.clone())
            .app_data(problems.clone())
            .app_data(languages.clone())
            .app_data(queue_tx.clone())
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .wrap(middleware::Logger::default())
            .service(web::resource("/jobs").route(web::post().to(post_jobs_handler)))
            .service(exit)
    })
    .bind((
        server_config
            .bind_address
            .unwrap_or("127.0.0.1".to_string()),
        server_config.bind_port.unwrap_or(12345),
    ))?
    .run();

    Ok(server)
}

pub fn get_db_path() -> PathBuf {
    use directories::ProjectDirs;

    let proj_dirs = ProjectDirs::from("", "", "oj").expect("Unable to find user directory");
    let data_dir = proj_dirs.data_local_dir();

    fs::create_dir_all(data_dir).expect("Failed to create local data dir");

    data_dir.join("oj.sqlite3")
}

pub async fn init_db(db_path: impl AsRef<Path>) -> sqlx::Result<SqlitePool> {
    let db_url = format!("sqlite://{}?mode=rwc", db_path.as_ref().display()); // rwc = read/write/create
    let db_pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(&db_url) // TODO: Use environment variable
        .await?;

    for sql in &[
        "PRAGMA foreign_keys = ON;",
        "PRAGMA busy_timeout = 2000;", // 2 seconds timeout for lock contention
        "PRAGMA journal_mode = WAL;",  // Write-Ahead Logging for better concurrency
        "PRAGMA synchronous = NORMAL;", // Balance between safety and performance
        r"
        CREATE TABLE IF NOT EXISTS users (
            id            INTEGER PRIMARY KEY,
            name          TEXT    NOT NULL UNIQUE
        );",
        r"
        CREATE TABLE IF NOT EXISTS jobs (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            created_time  TEXT    NOT NULL,
            updated_time  TEXT    NOT NULL,
            user_id       INTEGER NOT NULL,
            contest_id    INTEGER,
            problem_id    INTEGER NOT NULL,
            source_code   TEXT    NOT NULL,
            language      TEXT    NOT NULL,
            state         TEXT    NOT NULL,
            result        TEXT    NOT NULL,
            score         REAL,
            FOREIGN KEY (user_id)  REFERENCES users (id)
        );",
        "CREATE INDEX IF NOT EXISTS idx_jobs_created_time ON jobs(created_time);",
        r"
        CREATE TABLE job_case (
            job_id         INTEGER      NOT NULL,
            case_index     INTEGER      NOT NULL,
            result         TEXT         NOT NULL,
            time_us        INTEGER      NOT NULL,
            memory_kb      INTEGER      NOT NULL,
            info           TEXT         DEFAULT '',
            PRIMARY KEY (job_id, case_index),
            FOREIGN KEY (job_id)  REFERENCES jobs (id)
        );",
        "INSERT OR IGNORE INTO users (id, name) VALUES (0, 'root');",
    ] {
        sqlx::query(sql).execute(&db_pool).await?;
    }

    log::info!("initialized database at {}", db_path.as_ref().display());

    Ok(db_pool)
}

pub fn remove_db(db_path: impl AsRef<Path>) {
    if let Err(e) = std::fs::remove_file(&db_path) {
        log::warn!(
            "unable to remove database at {}: {e}",
            db_path.as_ref().display()
        );
    } else {
        log::info!("removed database at {}", db_path.as_ref().display());
    }
}
