use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};

use actix_web::{App, test, web};
use chrono::{SecondsFormat, Utc};
use sqlx::sqlite::SqlitePool;

use oj::database as db;
use oj::routes::get_jobs_handler;

// Global counter to ensure unique test database names
static TEST_DB_COUNTER: AtomicU32 = AtomicU32::new(0);

// Helper function to create isolated test database
async fn create_test_db() -> (SqlitePool, String) {
    let test_id = TEST_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let db_name = format!("test_get_jobs_{}.db", test_id);
    let db_path = format!("data/{}", db_name);

    // Remove existing test database if it exists
    let _ = fs::remove_file(&db_path);

    let db_pool = db::init_db(&db_path).await.unwrap();

    // Add test users
    for i in 1..=5 {
        let user_name = format!("test_user_{}", i);
        sqlx::query!(
            "INSERT OR IGNORE INTO users (id, name) VALUES (?, ?)",
            i,
            user_name
        )
        .execute(&db_pool)
        .await
        .unwrap();
    }

    (db_pool, db_path)
}

// Helper function to cleanup test database
fn cleanup_test_db(db_path: &str) {
    let _ = fs::remove_file(db_path);
    let _ = fs::remove_file(format!("{}-wal", db_path));
    let _ = fs::remove_file(format!("{}-shm", db_path));
}

// Test guard that ensures cleanup on drop
struct TestDbGuard {
    db_path: String,
}

impl TestDbGuard {
    fn new(db_path: String) -> Self {
        Self { db_path }
    }
}

impl Drop for TestDbGuard {
    fn drop(&mut self) {
        cleanup_test_db(&self.db_path);
    }
}

// Helper function to insert test jobs
async fn insert_test_jobs(pool: &SqlitePool) -> Vec<u32> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut job_ids = Vec::new();

    let test_jobs = vec![
        (1, 0, 0, "Rust", "Finished", "Accepted", 100.0),
        (2, 0, 1, "Python", "Finished", "Wrong Answer", 0.0),
        (1, 1, 0, "Rust", "Running", "Running", 0.0),
        (3, 0, 0, "C++", "Queueing", "Waiting", 0.0),
        (1, 0, 0, "Python", "Finished", "Time Limit Exceeded", 0.0),
    ];

    for (user_id, contest_id, problem_id, language, state, result_status, score) in test_jobs {
        let source_code = format!("// Test code for user {} problem {}", user_id, problem_id);

        let result = sqlx::query!(
            r#"
            INSERT INTO jobs (user_id, contest_id, problem_id, source_code, language, state, result, score, created_time, updated_time)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            user_id,
            contest_id,
            problem_id,
            source_code,
            language,
            state,
            result_status,
            score,
            now,
            now
        )
        .execute(pool)
        .await
        .unwrap();

        let job_id = result.last_insert_rowid() as u32;

        // Insert test cases for each job
        for case_idx in 0..2 {
            let case_result = if state == "Finished" {
                result_status
            } else {
                "Waiting"
            };
            let case_time = if state == "Finished" { 1000 } else { 0 };
            let case_memory = if state == "Finished" { 1024 } else { 0 };

            sqlx::query!(
                r#"
                INSERT INTO job_case (job_id, case_index, result, time_us, memory_kb)
                VALUES (?, ?, ?, ?, ?)
                "#,
                job_id,
                case_idx,
                case_result,
                case_time,
                case_memory
            )
            .execute(pool)
            .await
            .unwrap();
        }

        job_ids.push(job_id);
    }

    job_ids
}

#[actix_web::test]
async fn test_get_jobs_no_filter() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    // Insert test data
    let _job_ids = insert_test_jobs(&db_pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    let req = test::TestRequest::get().uri("/jobs").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let jobs: serde_json::Value = test::read_body_json(resp).await;
    let jobs_array = jobs.as_array().unwrap();

    // Should return all 5 jobs
    assert_eq!(jobs_array.len(), 5);

    // Verify structure of first job
    let first_job = &jobs_array[0];
    assert!(first_job["id"].is_number());
    assert!(first_job["created_time"].is_string());
    assert!(first_job["updated_time"].is_string());
    assert!(first_job["submission"].is_object());
    assert!(first_job["state"].is_string());
    assert!(first_job["result"].is_string());
    assert!(first_job["score"].is_number());
    assert!(first_job["cases"].is_array());
}

#[actix_web::test]
async fn test_get_jobs_filter_by_user_id() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    let _job_ids = insert_test_jobs(&db_pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    let req = test::TestRequest::get().uri("/jobs?user_id=1").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let jobs: serde_json::Value = test::read_body_json(resp).await;
    let jobs_array = jobs.as_array().unwrap();

    // Should return 3 jobs for user_id=1
    assert_eq!(jobs_array.len(), 3);

    // Verify all jobs belong to user_id=1
    for job in jobs_array {
        assert_eq!(job["submission"]["user_id"], 1);
    }
}

#[actix_web::test]
async fn test_get_jobs_filter_by_user_name() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    let _job_ids = insert_test_jobs(&db_pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/jobs?user_name=test_user_2")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let jobs: serde_json::Value = test::read_body_json(resp).await;
    let jobs_array = jobs.as_array().unwrap();

    // Should return 1 job for user_name=test_user_2 (user_id=2)
    assert_eq!(jobs_array.len(), 1);
    assert_eq!(jobs_array[0]["submission"]["user_id"], 2);
}

#[actix_web::test]
async fn test_get_jobs_filter_by_language() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    let _job_ids = insert_test_jobs(&db_pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/jobs?language=Rust")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let jobs: serde_json::Value = test::read_body_json(resp).await;
    let jobs_array = jobs.as_array().unwrap();

    // Should return 2 jobs with language=Rust
    assert_eq!(jobs_array.len(), 2);

    // Verify all jobs have language=Rust
    for job in jobs_array {
        assert_eq!(job["submission"]["language"], "Rust");
    }
}

#[actix_web::test]
async fn test_get_jobs_filter_by_state() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    let _job_ids = insert_test_jobs(&db_pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/jobs?state=Finished")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let jobs: serde_json::Value = test::read_body_json(resp).await;
    let jobs_array = jobs.as_array().unwrap();

    // Should return 3 jobs with state=Finished
    assert_eq!(jobs_array.len(), 3);

    // Verify all jobs have state=Finished
    for job in jobs_array {
        assert_eq!(job["state"], "Finished");
    }
}

#[actix_web::test]
async fn test_get_jobs_multiple_filters() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    let _job_ids = insert_test_jobs(&db_pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/jobs?user_id=1&state=Finished")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let jobs: serde_json::Value = test::read_body_json(resp).await;
    let jobs_array = jobs.as_array().unwrap();

    // Should return 2 jobs for user_id=1 AND state=Finished
    assert_eq!(jobs_array.len(), 2);

    // Verify all jobs match both filters
    for job in jobs_array {
        assert_eq!(job["submission"]["user_id"], 1);
        assert_eq!(job["state"], "Finished");
    }
}

#[actix_web::test]
async fn test_get_jobs_no_results() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    let _job_ids = insert_test_jobs(&db_pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/jobs?user_id=999") // Non-existent user
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let jobs: serde_json::Value = test::read_body_json(resp).await;
    let jobs_array = jobs.as_array().unwrap();

    // Should return empty array
    assert_eq!(jobs_array.len(), 0);
}

#[actix_web::test]
async fn test_get_jobs_invalid_from_date() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/jobs?from=invalid-date")
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400); // Bad Request

    let error: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(error["reason"], "ERR_INVALID_ARGUMENT");
    assert_eq!(error["code"], 1);
}

#[actix_web::test]
async fn test_get_jobs_valid_from_date() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);

    let _job_ids = insert_test_jobs(&db_pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .service(get_jobs_handler),
    )
    .await;

    // Use a date from yesterday
    let yesterday = chrono::Utc::now() - chrono::Duration::days(1);
    let from_date = yesterday.to_rfc3339_opts(SecondsFormat::Millis, true);

    let req = test::TestRequest::get()
        .uri(&format!("/jobs?from={}", from_date))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let jobs: serde_json::Value = test::read_body_json(resp).await;
    let jobs_array = jobs.as_array().unwrap();

    // Should return all jobs since they were created today
    assert_eq!(jobs_array.len(), 5);
}
