use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use actix_web::web;
use chrono::{SecondsFormat, Utc};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{QueryBuilder, Sqlite};

use crate::routes::{CaseResult, JobRecord, JobSubmission, JobsQueryParams};

const DATABASE_NAME: &str = "oj.sqlite3";

pub fn get_db_path() -> PathBuf {
    use directories::ProjectDirs;

    let proj_dirs = ProjectDirs::from("", "", "oj").expect("Unable to find user directory");
    let data_dir = proj_dirs.data_local_dir();

    fs::create_dir_all(data_dir).expect("Failed to create local data dir");

    data_dir.join(DATABASE_NAME)
}

pub async fn init_db(db_path: impl AsRef<Path>) -> sqlx::Result<SqlitePool> {
    let db_url = format!("sqlite://{}?mode=rwc", db_path.as_ref().display()); // rwc = read/write/create
    let db_pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(&db_url) // TODO: Use environment variable
        .await?;

    // Execute PRAGMA statements first (these cannot be run inside a transaction)
    for pragma_sql in &[
        "PRAGMA foreign_keys = ON;",
        "PRAGMA busy_timeout = 2000;", // 2 seconds timeout for lock contention
        "PRAGMA journal_mode = WAL;",  // Write-Ahead Logging for better concurrency
        "PRAGMA synchronous = NORMAL;", // Balance between safety and performance
    ] {
        sqlx::query(pragma_sql).execute(&db_pool).await?;
    }

    // Use a transaction for table creation and data initialization
    let mut tx = db_pool.begin().await?;

    for sql in &[
        r"
        CREATE TABLE IF NOT EXISTS users (
            id            INTEGER PRIMARY KEY,
            name          TEXT    NOT NULL UNIQUE
        );",
        r"
        CREATE TABLE IF NOT EXISTS jobs (
            pk            INTEGER  PRIMARY KEY,
            id            INTEGER  GENERATED ALWAYS AS (pk - 1) STORED UNIQUE,
            created_time  TEXT     NOT NULL,
            updated_time  TEXT     NOT NULL,
            user_id       INTEGER  NOT NULL,
            contest_id    INTEGER  NOT NULL,
            problem_id    INTEGER  NOT NULL,
            source_code   TEXT     NOT NULL,
            language      TEXT     NOT NULL,
            state         TEXT     NOT NULL,
            result        TEXT     NOT NULL,
            score         REAL     NOT NULL,
            FOREIGN KEY (user_id)  REFERENCES users (id)
        );",
        r"
        CREATE TABLE IF NOT EXISTS job_case (
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
        sqlx::query(sql).execute(tx.as_mut()).await?;
    }

    tx.commit().await?;

    log::info!("Initialized database at {}", db_path.as_ref().display());

    Ok(db_pool)
}

pub fn remove_db(db_path: impl AsRef<Path>) {
    // Remove WAL and SHM files (ignore errors as they might not exist)
    let wal_path = format!("{}-wal", db_path.as_ref().display());
    let shm_path = format!("{}-shm", db_path.as_ref().display());
    let _ = fs::remove_file(wal_path);
    let _ = fs::remove_file(shm_path);

    // Remove main database file
    if let Err(e) = std::fs::remove_file(&db_path) {
        log::warn!(
            "Unable to remove database at {}: {e}",
            db_path.as_ref().display()
        );
    } else {
        log::info!("Removed database at {}", db_path.as_ref().display());
    }
}

/// Creates a new job entry in the database along with its associated test cases.
///
/// * `len` - The number of cases, including compilation, to create for this job.
///
/// # Errors
///
/// This function will return an `Err` in the following cases:
///
/// - If the database connection pool cannot begin a transaction.
/// - If the insertion of the job record into the `jobs` table fails (e.g., due to constraint violations).
/// - If the insertion of any of the associated test cases into the `job_case` table fails.
/// - If committing the transaction fails.
pub async fn create_job(
    body: &web::Json<JobSubmission>,
    pool: &web::Data<SqlitePool>,
    len: u32,
) -> sqlx::Result<u32> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

    // Use a transaction for better error handling and potential future batch operations
    let mut tx = pool.begin().await?;

    let result = sqlx::query!(
        r#"
        INSERT INTO jobs (user_id, contest_id, problem_id, source_code, language, state, result, score, created_time, updated_time)
        VALUES (?, ?, ?, ?, ?, 'Queueing', 'Waiting', 0.0, ?, ?)
        "#,
        body.user_id,
        body.contest_id,
        body.problem_id,
        body.source_code,
        body.language,
        now,
        now
    )
    .execute(tx.as_mut())
    .await?;

    let pk = result.last_insert_rowid() as u32;
    let job_id = pk - 1; // Since id is generated as pk - 1

    for i in 0..len {
        sqlx::query!(
            r#"
            INSERT INTO job_case (job_id, case_index, result, time_us, memory_kb)
            VALUES (?, ?, 'Waiting', 0, 0)
            "#,
            job_id,
            i
        )
        .execute(tx.as_mut())
        .await?;
    }

    tx.commit().await?;
    Ok(job_id)
}

pub async fn fetch_job(id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<JobRecord> {
    let job_data = sqlx::query!(
        r#"
        SELECT user_id, contest_id, problem_id, source_code, language, state, result, score, created_time, updated_time
        FROM jobs
        WHERE id = ?
        "#,
        id
    )
    .fetch_one(pool.as_ref())
    .await?;

    let submission = JobSubmission {
        user_id: job_data.user_id as u32,
        contest_id: job_data.contest_id as u32,
        problem_id: job_data.problem_id as u32,
        source_code: job_data.source_code,
        language: job_data.language,
    };

    // Fetch case results
    let case_data = sqlx::query!(
        r#"
        SELECT case_index, result, time_us, memory_kb, info
        FROM job_case
        WHERE job_id = ?
        ORDER BY case_index
        "#,
        id
    )
    .fetch_all(pool.as_ref())
    .await?;

    let cases = case_data
        .into_iter()
        .map(|case| CaseResult {
            id: case.case_index as u32,
            result: case.result,
            time: case.time_us as u32,
            memory: case.memory_kb as u32, // memory in KB
            info: case.info.unwrap_or_default(),
        })
        .collect();

    Ok(JobRecord {
        id,
        created_time: job_data.created_time,
        updated_time: job_data.updated_time,
        submission,
        state: job_data.state,
        result: job_data.result,
        score: job_data.score,
        cases,
    })
}

pub async fn update_job_to_running(id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<()> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut tx = pool.begin().await?;

    // Update job state and result to Running
    sqlx::query!(
        r#"
        UPDATE jobs 
        SET state = 'Running', result = 'Running', updated_time = ?
        WHERE id = ?
        "#,
        now,
        id
    )
    .execute(tx.as_mut())
    .await?;

    // Update case 0 result to Running
    sqlx::query!(
        r#"
        UPDATE job_case 
        SET result = 'Running'
        WHERE job_id = ? AND case_index = 0
        "#,
        id
    )
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn save_result(id: u32, pool: Arc<SqlitePool>, result: &JobRecord) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;

    // Update job record
    sqlx::query!(
        r#"
        UPDATE jobs 
        SET state = ?, result = ?, score = ?, updated_time = ?
        WHERE id = ?
        "#,
        result.state,
        result.result,
        result.score,
        result.updated_time,
        id
    )
    .execute(tx.as_mut())
    .await?;

    // Delete existing case results
    sqlx::query!(
        r#"
        DELETE FROM job_case WHERE job_id = ?
        "#,
        id
    )
    .execute(tx.as_mut())
    .await?;

    // Insert new case results
    for case in &result.cases {
        sqlx::query!(
            r#"
            INSERT INTO job_case (job_id, case_index, result, time_us, memory_kb, info)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            id,
            case.id,
            case.result,
            case.time,
            case.memory, // memory already in KB
            case.info
        )
        .execute(tx.as_mut())
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn fetch_jobs_by_query(
    query: web::Query<JobsQueryParams>,
    pool: Arc<SqlitePool>,
) -> sqlx::Result<Vec<JobRecord>> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT id FROM jobs WHERE 1=1");

    if let Some(user_id) = query.user_id {
        qb.push(" AND user_id = ").push_bind(user_id);
    }
    if let Some(ref user_name) = query.user_name {
        qb.push(" AND user_id IN (SELECT id FROM users WHERE name = ")
            .push_bind(user_name)
            .push(")");
    }
    if let Some(contest_id) = query.contest_id {
        qb.push(" AND contest_id = ").push_bind(contest_id);
    }
    if let Some(problem_id) = query.problem_id {
        qb.push(" AND problem_id = ").push_bind(problem_id);
    }
    if let Some(ref language) = query.language {
        qb.push(" AND language = ").push_bind(language);
    }
    if let Some(ref from) = query.from {
        qb.push(" AND created_time >= ").push_bind(from);
    }
    if let Some(ref to) = query.to {
        qb.push(" AND created_time <= ").push_bind(to);
    }
    if let Some(ref state) = query.state {
        qb.push(" AND state = ").push_bind(state);
    }
    if let Some(ref result) = query.result {
        qb.push(" AND result = ").push_bind(result);
    }
    qb.push(" ORDER BY created_time");

    let job_ids = qb
        .build_query_as::<(u32,)>()
        .fetch_all(pool.as_ref())
        .await?;
    let mut jobs = Vec::new();

    for (id,) in job_ids {
        jobs.push(fetch_job(id, pool.clone()).await.unwrap());
    }
    Ok(jobs)
}
