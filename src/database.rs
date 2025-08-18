use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use actix_web::web;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{QueryBuilder, Sqlite};

use crate::routes::{
    CaseResult, JobRecord, JobSubmission, JobsQueryParams, RanklistEntry, User, UserScore,
};

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
        .max_connections(1) // Reduce from 2 to 1 to minimize memory overhead
        .min_connections(0) // Allow pool to shrink when idle
        .connect(&db_url)
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
    pool: Arc<SqlitePool>,
    len: u32,
) -> sqlx::Result<u32> {
    let now = crate::memory_optimization::create_timestamp();

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

    // Batch insert all cases in a single prepared statement for better performance
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

pub async fn find_job(id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        SELECT 1 as "exists_flag: i32" FROM jobs WHERE id = ?
        "#,
        id
    )
    .fetch_optional(pool.as_ref())
    .await?;

    Ok(result.is_some())
}

pub async fn find_user(id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        SELECT 1 as "exists_flag: i32" FROM users WHERE id = ?
        "#,
        id
    )
    .fetch_optional(pool.as_ref())
    .await?;

    Ok(result.is_some())
}

pub async fn fetch_job(id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<JobRecord> {
    log::debug!("Trying to fetch job {id} full record from database");

    // Use a single query to get both job data and case data
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

    // Fetch case results in a single query and pre-allocate the vector
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

    // Pre-allocate the cases vector with the exact size needed
    let mut cases = Vec::with_capacity(case_data.len());
    for case in case_data {
        cases.push(CaseResult {
            id: case.case_index as u32,
            result: crate::memory_optimization::get_or_create_string(&case.result),
            time: case.time_us as u32,
            memory: case.memory_kb as u32, // memory in KB
            info: case.info.unwrap_or_default(),
        });
    }

    log::debug!("Fetched job {id} full record from database");
    Ok(JobRecord {
        id,
        created_time: job_data.created_time,
        updated_time: job_data.updated_time,
        submission,
        state: crate::memory_optimization::get_or_create_string(&job_data.state),
        result: crate::memory_optimization::get_or_create_string(&job_data.result),
        score: job_data.score,
        cases,
    })
}

pub async fn update_job_to_running(id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<()> {
    let now = crate::memory_optimization::create_timestamp();
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

pub async fn update_job_to_canceled(id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<()> {
    let now = crate::memory_optimization::create_timestamp();
    let mut tx = pool.begin().await?;

    // Update job state and result to Canceled
    sqlx::query!(
        r#"
        UPDATE jobs 
        SET state = 'Canceled', result = 'Skipped', updated_time = ?
        WHERE id = ?
        "#,
        now,
        id
    )
    .execute(tx.as_mut())
    .await?;

    // Update all case results to Canceled
    sqlx::query!(
        r#"
        UPDATE job_case 
        SET result = 'Skipped'
        WHERE job_id = ?
        "#,
        id
    )
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Returns the number of cases reverted
pub async fn revert_job_to_queueing(id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<usize> {
    let now = crate::memory_optimization::create_timestamp();
    let mut tx = pool.begin().await?;

    // Revert job state, result and score
    sqlx::query!(
        r#"
        UPDATE jobs 
        SET state = 'Queueing', result = 'Waiting', score = 0.0, updated_time = ?
        WHERE id = ?
        "#,
        now,
        id
    )
    .execute(tx.as_mut())
    .await?;

    // Revert each case
    let reverted_cases = sqlx::query!(
        r#"
        UPDATE job_case 
        SET result = 'Waiting', time_us = 0, memory_kb = 0, info = ''
        WHERE job_id = ?
        "#,
        id
    )
    .execute(tx.as_mut())
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(reverted_cases as usize)
}

pub async fn save_result(id: u32, pool: Arc<SqlitePool>, result: &JobRecord) -> sqlx::Result<()> {
    let now = crate::memory_optimization::create_timestamp();
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
        now,
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
    // Build a single query to get all job data and cases in one go
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT j.id, j.created_time, j.updated_time, j.user_id, j.contest_id, j.problem_id, \
         j.source_code, j.language, j.state, j.result, j.score, \
         jc.case_index, jc.result as case_result, jc.time_us, jc.memory_kb, jc.info \
         FROM jobs j LEFT JOIN job_case jc ON j.id = jc.job_id WHERE 1=1",
    );

    if let Some(user_id) = query.user_id {
        qb.push(" AND j.user_id = ").push_bind(user_id);
    }
    if let Some(ref user_name) = query.user_name {
        qb.push(" AND j.user_id IN (SELECT id FROM users WHERE name = ")
            .push_bind(user_name)
            .push(")");
    }
    if let Some(contest_id) = query.contest_id {
        qb.push(" AND j.contest_id = ").push_bind(contest_id);
    }
    if let Some(problem_id) = query.problem_id {
        qb.push(" AND j.problem_id = ").push_bind(problem_id);
    }
    if let Some(ref language) = query.language {
        qb.push(" AND j.language = ").push_bind(language);
    }
    if let Some(ref from) = query.from {
        qb.push(" AND j.created_time >= ").push_bind(from);
    }
    if let Some(ref to) = query.to {
        qb.push(" AND j.created_time <= ").push_bind(to);
    }
    if let Some(ref state) = query.state {
        qb.push(" AND j.state = ").push_bind(state);
    }
    if let Some(ref result) = query.result {
        qb.push(" AND j.result = ").push_bind(result);
    }
    qb.push(" ORDER BY j.created_time, jc.case_index");

    #[derive(sqlx::FromRow)]
    struct JobRowData {
        id: u32,
        created_time: String,
        updated_time: String,
        user_id: u32,
        contest_id: u32,
        problem_id: u32,
        source_code: String,
        language: String,
        state: String,
        result: String,
        score: f64,
        case_index: Option<u32>,
        case_result: Option<String>,
        time_us: Option<u32>,
        memory_kb: Option<u32>,
        info: Option<String>,
    }

    let rows = qb
        .build_query_as::<JobRowData>()
        .fetch_all(pool.as_ref())
        .await?;

    // Group rows by job ID and build JobRecord structs
    let mut jobs_map: std::collections::HashMap<u32, JobRecord> = std::collections::HashMap::new();

    for row in rows {
        let job = jobs_map.entry(row.id).or_insert_with(|| JobRecord {
            id: row.id,
            created_time: row.created_time.clone(),
            updated_time: row.updated_time.clone(),
            submission: JobSubmission {
                user_id: row.user_id,
                contest_id: row.contest_id,
                problem_id: row.problem_id,
                source_code: row.source_code.clone(),
                language: row.language.clone(),
            },
            state: row.state.clone(),
            result: row.result.clone(),
            score: row.score,
            cases: Vec::new(),
        });

        // Add case result if present
        if let (Some(case_index), Some(case_result)) = (row.case_index, row.case_result) {
            job.cases.push(CaseResult {
                id: case_index,
                result: case_result,
                time: row.time_us.unwrap_or(0),
                memory: row.memory_kb.unwrap_or(0),
                info: row.info.unwrap_or_default(),
            });
        }
    }

    // Convert to sorted vector
    let mut jobs: Vec<JobRecord> = jobs_map.into_values().collect();
    jobs.sort_by(|a, b| a.created_time.cmp(&b.created_time));

    // Sort cases within each job
    for job in &mut jobs {
        job.cases.sort_by_key(|case| case.id);
    }

    Ok(jobs)
}

/// Get all users from the database
pub async fn get_users(pool: Arc<SqlitePool>) -> sqlx::Result<Vec<User>> {
    let users = sqlx::query_as!(
        User,
        r#"
        SELECT id as "id: u32", name
        FROM users
        ORDER BY id
        "#
    )
    .fetch_all(pool.as_ref())
    .await?;

    Ok(users)
}

/// Check if a user name already exists for a different user ID
pub async fn user_name_exists(
    name: &str,
    exclude_id: Option<u32>,
    pool: Arc<SqlitePool>,
) -> sqlx::Result<bool> {
    if let Some(id) = exclude_id {
        let result = sqlx::query!(
            r#"
            SELECT 1 as "exists_flag: i32" FROM users WHERE name = ? AND id != ?
            "#,
            name,
            id
        )
        .fetch_optional(pool.as_ref())
        .await?;
        Ok(result.is_some())
    } else {
        let result = sqlx::query!(
            r#"
            SELECT 1 as "exists_flag: i32" FROM users WHERE name = ?
            "#,
            name
        )
        .fetch_optional(pool.as_ref())
        .await?;
        Ok(result.is_some())
    }
}

/// Get the next available user ID
pub async fn get_next_user_id(pool: Arc<SqlitePool>) -> sqlx::Result<u32> {
    let max_id = sqlx::query!(
        r#"
        SELECT MAX(id) as max_id FROM users
        "#
    )
    .fetch_one(pool.as_ref())
    .await?;

    Ok(max_id.max_id.map(|id| id + 1).unwrap_or(0) as u32)
}

/// Create a new user with auto-generated ID
pub async fn create_user(name: &str, pool: Arc<SqlitePool>) -> sqlx::Result<User> {
    let new_id = get_next_user_id(pool.clone()).await?;

    sqlx::query!(
        r#"
        INSERT INTO users (id, name) VALUES (?, ?)
        "#,
        new_id,
        name
    )
    .execute(pool.as_ref())
    .await?;

    Ok(User {
        id: new_id,
        name: name.to_string(),
    })
}

/// Update an existing user
pub async fn update_user(id: u32, name: &str, pool: Arc<SqlitePool>) -> sqlx::Result<User> {
    sqlx::query!(
        r#"
        UPDATE users SET name = ? WHERE id = ?
        "#,
        name,
        id
    )
    .execute(pool.as_ref())
    .await?;

    Ok(User {
        id,
        name: name.to_string(),
    })
}

/// Get global ranklist (contest_id = 0)
pub async fn get_global_ranklist(
    scoring_rule: Option<String>,
    tie_breaker: Option<String>,
    problems: Arc<crate::config::ProblemConfig>,
    pool: Arc<SqlitePool>,
) -> sqlx::Result<Vec<RanklistEntry>> {
    let scoring_rule = scoring_rule.unwrap_or_else(|| "latest".to_string());
    let tie_breaker = tie_breaker.unwrap_or_default();

    // Get all users
    let users = get_users(pool.clone()).await?;

    // Get all problem IDs from configuration (sorted)
    let mut problem_ids: Vec<u32> = problems.iter().map(|p| p.id).collect();
    problem_ids.sort();

    // Calculate scores for each user
    let mut user_scores = Vec::new();

    for user in users {
        let mut user_score = UserScore {
            user_id: user.id,
            user_name: user.name.clone(),
            problem_scores: HashMap::new(),
            total_score: 0.0,
            latest_submission_time: None,
            submission_count: 0,
        };

        // Initialize all problem scores to 0
        for problem_id in &problem_ids {
            user_score.problem_scores.insert(*problem_id, 0.0);
        }

        // Get user's job submission data
        let jobs = get_user_jobs(user.id, pool.clone()).await?;
        user_score.submission_count = jobs.len() as u32;

        // Calculate score and the scoring-used submission time for each problem based on scoring rule
        for problem_id in &problem_ids {
            let problem_jobs: Vec<_> = jobs
                .iter()
                .filter(|job| job.submission.problem_id == *problem_id)
                .collect();

            if !problem_jobs.is_empty() {
                // Choose the job used for scoring for this problem, and remember its time
                let chosen_job = match scoring_rule.as_str() {
                    "latest" => {
                        // Use the latest submission
                        problem_jobs
                            .iter()
                            .max_by_key(|job| &job.created_time)
                            .copied()
                    }
                    "highest" => {
                        // Use the highest score, with earliest submission time as tie breaker
                        problem_jobs
                            .iter()
                            .max_by(|a, b| {
                                a.score
                                    .partial_cmp(&b.score)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                                    // If scores equal, earlier created_time is better
                                    .then_with(|| b.created_time.cmp(&a.created_time))
                            })
                            .copied()
                    }
                    _ => None,
                };

                if let Some(job) = chosen_job {
                    user_score.problem_scores.insert(*problem_id, job.score);

                    // Update the user's latest scoring-used submission time
                    if user_score
                        .latest_submission_time
                        .as_ref()
                        .map_or(true, |t| t < &job.created_time)
                    {
                        user_score.latest_submission_time = Some(job.created_time.clone());
                    }
                }
            }
        }

        // Calculate total score
        user_score.total_score = user_score.problem_scores.values().sum();
        user_scores.push(user_score);
    }

    // Sort users based on scores and tie breaker
    user_scores.sort_unstable_by(|a, b| {
        // First sort by total score (descending)
        match b
            .total_score
            .partial_cmp(&a.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
        {
            std::cmp::Ordering::Equal => {
                // If scores are equal, apply tie breaker
                match tie_breaker.as_str() {
                    "submission_time" => {
                        // Earlier latest submission time is better
                        match (&a.latest_submission_time, &b.latest_submission_time) {
                            (Some(a_time), Some(b_time)) => a_time.cmp(b_time),
                            (Some(_), None) => std::cmp::Ordering::Less, // Has submission is better than no submission
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            _ => std::cmp::Ordering::Equal, // Both have no submissions
                        }
                    }
                    "submission_count" => {
                        // Fewer submissions is better
                        a.submission_count.cmp(&b.submission_count)
                    }
                    "user_id" => {
                        // Smaller user_id is better
                        a.user_id.cmp(&b.user_id)
                    }
                    _ => std::cmp::Ordering::Equal, // Default to none
                }
            }
            other => other,
        }
    });

    // Calculate ranks and build result
    let mut result = Vec::new();
    let mut current_rank = 1;

    for (index, user_score) in user_scores.iter().enumerate() {
        // Update rank if this user has different score from previous
        if index > 0 {
            let prev_user = &user_scores[index - 1];
            if user_score.total_score < prev_user.total_score {
                current_rank = index as u32 + 1;
            }
            // If scores are equal, check tie breaker
            else {
                let tie_broken = match tie_breaker.as_str() {
                    "submission_time" => {
                        match (
                            &user_score.latest_submission_time,
                            &prev_user.latest_submission_time,
                        ) {
                            (Some(curr_time), Some(prev_time)) => curr_time != prev_time,
                            (None, None) => false,
                            _ => true, // One has submission, the other doesn't
                        }
                    }
                    "submission_count" => user_score.submission_count != prev_user.submission_count,
                    "user_id" => user_score.user_id != prev_user.user_id,
                    _ => false,
                };

                if tie_broken {
                    current_rank = index as u32 + 1;
                }
            }
        }

        // Build scores array in problem_id order
        let scores: Vec<f64> = problem_ids
            .iter()
            .map(|problem_id| {
                user_score
                    .problem_scores
                    .get(problem_id)
                    .copied()
                    .unwrap_or(0.0)
            })
            .collect();

        result.push(RanklistEntry {
            user: User {
                id: user_score.user_id,
                name: user_score.user_name.clone(),
            },
            rank: current_rank,
            scores,
        });
    }

    Ok(result)
}

/// Get all jobs for a specific user
async fn get_user_jobs(user_id: u32, pool: Arc<SqlitePool>) -> sqlx::Result<Vec<JobRecord>> {
    let job_ids = sqlx::query!(
        r#"
        SELECT id FROM jobs WHERE user_id = ? ORDER BY created_time
        "#,
        user_id
    )
    .fetch_all(pool.as_ref())
    .await?;

    let mut jobs = Vec::new();
    for row in job_ids {
        if let Ok(job) = fetch_job(row.id as u32, pool.clone()).await {
            jobs.push(job);
        }
    }

    Ok(jobs)
}
