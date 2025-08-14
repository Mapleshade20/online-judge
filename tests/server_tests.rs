use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use actix_web::{
    App, HttpRequest,
    error::{Error, JsonPayloadError},
    test, web,
};
use chrono::{SecondsFormat, Utc};
use serde_json::json;
use sqlx::sqlite::SqlitePool;

use oj::config::{
    JudgeType, KiloByte, LanguageConfig, MicroSecond, OneCaseConfig, OneLanguageConfig,
    OneProblemConfig, ProblemConfig,
};
use oj::database as db;
use oj::queue::JobQueue;
use oj::routes::{CaseResult, JobMessage, JobRecord, JobSubmission, post_jobs_handler};

// Global counter to ensure unique test database names
static TEST_DB_COUNTER: AtomicU32 = AtomicU32::new(0);

// Helper function to create isolated test database
async fn create_test_db() -> (SqlitePool, String) {
    // Create a unique database file for each test
    let test_id = TEST_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let db_name = format!("test_oj_{}.db", test_id);
    let db_path = format!("data/{}", db_name);

    // Remove existing test database if it exists
    let _ = fs::remove_file(&db_path);

    let db_pool = db::init_db(&db_path).await.unwrap();

    // Add test users for integration tests
    for i in 1..10 {
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
    if let Err(e) = fs::remove_file(db_path) {
        eprintln!("Warning: Failed to remove test database {}: {}", db_path, e);
    } else {
        println!("Cleaned up test database: {}", db_path);
    }

    // Also remove WAL and SHM files if they exist
    let wal_path = format!("{}-wal", db_path);
    let shm_path = format!("{}-shm", db_path);
    let _ = fs::remove_file(wal_path);
    let _ = fs::remove_file(shm_path);
}

// Helper function for JSON error handling
fn json_error_handler(err: JsonPayloadError, _req: &HttpRequest) -> Error {
    actix_web::error::ErrorBadRequest(err)
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

// Helper function to create test config
fn create_test_config() -> (Arc<ProblemConfig>, Arc<LanguageConfig>) {
    let problems = vec![
        OneProblemConfig {
            id: 0,
            name: "test_problem_1".to_string(),
            judge_type: JudgeType::Standard,
            cases: vec![
                OneCaseConfig {
                    score: 50.0,
                    input_file: "test1.in".to_string(),
                    answer_file: "test1.ans".to_string(),
                    time_limit: MicroSecond(1000000),
                    memory_limit: KiloByte(1048576),
                },
                OneCaseConfig {
                    score: 50.0,
                    input_file: "test2.in".to_string(),
                    answer_file: "test2.ans".to_string(),
                    time_limit: MicroSecond(2000000),
                    memory_limit: KiloByte(1048576),
                },
            ],
        },
        OneProblemConfig {
            id: 1,
            name: "test_problem_2".to_string(),
            judge_type: JudgeType::Standard,
            cases: vec![OneCaseConfig {
                score: 100.0,
                input_file: "test1.in".to_string(),
                answer_file: "test1.ans".to_string(),
                time_limit: MicroSecond(1000000),
                memory_limit: KiloByte(1048576),
            }],
        },
    ];

    let languages = vec![
        OneLanguageConfig {
            name: "Rust".to_string(),
            file_name: "main.rs".to_string(),
            command: vec![
                "rustc".to_string(),
                "-o".to_string(),
                "%OUTPUT%".to_string(),
                "%INPUT%".to_string(),
            ],
        },
        OneLanguageConfig {
            name: "Python".to_string(),
            file_name: "main.py".to_string(),
            command: vec!["python3".to_string(), "%INPUT%".to_string()],
        },
    ];

    (Arc::new(problems), Arc::new(languages))
}

// Mock judger that simulates evaluation results
async fn mock_judger(job_queue: Arc<JobQueue>) {
    loop {
        let message = job_queue.pop().await;
        match message {
            JobMessage::FireAndForget { job_id } => {
                // For non-blocking jobs, we just consume the message
                println!("Mock judger received fire-and-forget job: {}", job_id);
            }
            JobMessage::Blocking { job_id, responder } => {
                // For blocking jobs, we send back a mock response
                println!("Mock judger received blocking job: {}", job_id);

                let mock_response = JobRecord {
                    id: job_id,
                    created_time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                    updated_time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                    submission: JobSubmission {
                        source_code: "fn main() { println!(\"Hello World!\"); }".to_string(),
                        language: "Rust".to_string(),
                        user_id: 0,
                        contest_id: 0,
                        problem_id: 0,
                    },
                    state: "Finished".to_string(),
                    result: "Accepted".to_string(),
                    score: 100.0,
                    cases: vec![
                        CaseResult {
                            id: 0,
                            result: "Accepted".to_string(),
                            time: 100,
                            memory: 1024,
                            info: "".to_string(),
                        },
                        CaseResult {
                            id: 1,
                            result: "Accepted".to_string(),
                            time: 150,
                            memory: 1024,
                            info: "".to_string(),
                        },
                    ],
                };

                let _ = responder.send(mock_response);
            }
        }
    }
}

#[actix_web::test]
async fn test_post_jobs_nonblocking_success() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(false);

    // Start mock judger
    tokio::spawn(mock_judger(job_queue.clone()));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    let request_body = json!({
        "source_code": "fn main() { println!(\"Hello World!\"); }",
        "language": "Rust",
        "user_id": 0,
        "contest_id": 0,
        "problem_id": 1
    });

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_json(&request_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200); // OK for nonblocking

    let response_body: serde_json::Value = test::read_body_json(resp).await;

    // Verify response structure
    assert!(response_body["id"].is_number());
    assert_eq!(response_body["state"], "Queueing");
    assert_eq!(response_body["result"], "Waiting");
    assert_eq!(response_body["score"], 0.0);
    assert_eq!(response_body["cases"].as_array().unwrap().len(), 2);
    assert_eq!(response_body["cases"][0]["result"], "Waiting");
    assert_eq!(response_body["cases"][1]["result"], "Waiting");

    // Verify job was stored in database
    let job_id = response_body["id"].as_u64().unwrap() as u32;
    let stored_job = sqlx::query!("SELECT * FROM jobs WHERE id = ?", job_id)
        .fetch_one(&db_pool)
        .await
        .expect("Failed to fetch job from database");

    assert_eq!(stored_job.user_id, 0);
    assert_eq!(stored_job.contest_id.unwrap(), 0);
    assert_eq!(stored_job.problem_id, 1);
    assert_eq!(stored_job.language, "Rust");
    assert_eq!(stored_job.state, "Queueing");
    assert_eq!(stored_job.result, "Waiting");
}

#[actix_web::test]
async fn test_post_jobs_blocking_success() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(true);

    // Start mock judger
    tokio::spawn(mock_judger(job_queue.clone()));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    let request_body = json!({
        "source_code": "fn main() { println!(\"Hello World!\"); }",
        "language": "Rust",
        "user_id": 0,
        "contest_id": 0,
        "problem_id": 0
    });

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_json(&request_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200); // OK for blocking

    let response_body: serde_json::Value = test::read_body_json(resp).await;

    // Verify response structure for completed job
    assert!(response_body["id"].is_number());
    assert_eq!(response_body["state"], "Finished");
    assert_eq!(response_body["result"], "Accepted");
    assert_eq!(response_body["score"], 100.0);
    assert_eq!(response_body["cases"].as_array().unwrap().len(), 2);
    assert_eq!(response_body["cases"][0]["result"], "Accepted");
    assert_eq!(response_body["cases"][1]["result"], "Accepted");

    // Verify job was stored in database
    let job_id = response_body["id"].as_u64().unwrap() as u32;
    let stored_job = sqlx::query!("SELECT * FROM jobs WHERE id = ?", job_id)
        .fetch_one(&db_pool)
        .await
        .expect("Failed to fetch job from database");

    assert_eq!(stored_job.user_id, 0);
    assert_eq!(stored_job.contest_id.unwrap(), 0);
    assert_eq!(stored_job.problem_id, 0);
    assert_eq!(stored_job.language, "Rust");
    assert_eq!(stored_job.state, "Queueing"); // Note: database state isn't updated in mock
}

#[actix_web::test]
async fn test_post_jobs_invalid_language() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(false);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    let request_body = json!({
        "source_code": "fn main() { println!(\"Hello World!\"); }",
        "language": "InvalidLanguage",  // Invalid language
        "user_id": 0,
        "contest_id": 0,
        "problem_id": 0
    });

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_json(&request_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404); // Not Found

    let response_body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(response_body["reason"], "ERR_NOT_FOUND");
    assert_eq!(response_body["code"], 3);
}

#[actix_web::test]
async fn test_post_jobs_invalid_problem_id() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(false);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    let request_body = json!({
        "source_code": "fn main() { println!(\"Hello World!\"); }",
        "language": "Rust",
        "user_id": 0,
        "contest_id": 0,
        "problem_id": 999  // Invalid problem_id
    });

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_json(&request_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404); // Not Found

    let response_body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(response_body["reason"], "ERR_NOT_FOUND");
    assert_eq!(response_body["code"], 3);
}

#[actix_web::test]
async fn test_post_jobs_invalid_json() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(false);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_payload("invalid json")
        .insert_header(("content-type", "application/json"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400); // Bad Request

    // For JSON parsing errors, we just check the status code
    // The error body might not be valid JSON
}

#[actix_web::test]
async fn test_post_jobs_missing_fields() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(false);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    let request_body = json!({
        "source_code": "fn main() { println!(\"Hello World!\"); }",
        "language": "Rust"
        // Missing user_id, contest_id, problem_id
    });

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_json(&request_body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400); // Bad Request

    // For JSON parsing errors, we just check the status code
    // The error body might not be valid JSON
}

#[actix_web::test]
async fn test_blocking_job_delayed_response() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(true);

    // Mock judger that responds after a delay
    let delayed_queue = job_queue.clone();
    tokio::spawn(async move {
        loop {
            let message = delayed_queue.pop().await;
            match message {
                JobMessage::Blocking { job_id, responder } => {
                    println!(
                        "Mock judger received blocking job, will respond after delay: {}",
                        job_id
                    );

                    // Simulate some processing time
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    let response = JobRecord {
                        id: job_id,
                        created_time: "2024-08-09T01:00:00.000Z".to_string(),
                        updated_time: "2024-08-09T01:00:01.000Z".to_string(),
                        submission: JobSubmission {
                            source_code: "fn main() { println!(\"Hello World!\"); }".to_string(),
                            language: "Rust".to_string(),
                            user_id: 0,
                            contest_id: 0,
                            problem_id: 0,
                        },
                        state: "Finished".to_string(),
                        result: "Accepted".to_string(),
                        score: 100.0,
                        cases: vec![CaseResult {
                            id: 1,
                            result: "Accepted".to_string(),
                            time: 1000,
                            memory: 1024,
                            info: "".to_string(),
                        }],
                    };

                    let _ = responder.send(response);
                }
                _ => {}
            }
        }
    });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    let request_body = json!({
        "source_code": "fn main() { println!(\"Hello World!\"); }",
        "language": "Rust",
        "user_id": 0,
        "contest_id": 0,
        "problem_id": 0
    });

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_json(&request_body)
        .to_request();

    // This should complete after the delay
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    let body: JobRecord = test::read_body_json(resp).await;
    assert!(body.id >= 1, "job_id should be positive"); // Don't check exact ID since it depends on previous tests
    assert_eq!(body.state, "Finished");
    assert_eq!(body.result, "Accepted");
}

#[actix_web::test]
async fn test_multiple_languages_support() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(false);

    tokio::spawn(mock_judger(job_queue.clone()));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    // Test Rust
    let rust_request = json!({
        "source_code": "fn main() { println!(\"Hello World!\"); }",
        "language": "Rust",
        "user_id": 0,
        "contest_id": 0,
        "problem_id": 1
    });

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_json(&rust_request)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Test Python
    let python_request = json!({
        "source_code": "print('Hello World!')",
        "language": "Python",
        "user_id": 0,
        "contest_id": 0,
        "problem_id": 1
    });

    let req = test::TestRequest::post()
        .uri("/jobs")
        .set_json(&python_request)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Verify both jobs were stored
    let job_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs")
        .fetch_one(&db_pool)
        .await
        .expect("Failed to count jobs");

    assert_eq!(job_count, 2);
}

#[actix_web::test]
async fn test_database_persistence() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(false);

    tokio::spawn(mock_judger(job_queue.clone()));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    // Send multiple requests
    for i in 0..3 {
        let request_body = json!({
            "source_code": format!("fn main() {{ println!(\"Test {}\"); }}", i),
            "language": "Rust",
            "user_id": i,
            "contest_id": 0,
            "problem_id": 1
        });

        println!("Sending request {}: {}", i, request_body);

        let req = test::TestRequest::post()
            .uri("/jobs")
            .set_json(&request_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    // Verify that jobs were inserted into the database
    let job_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs")
        .fetch_one(&db_pool)
        .await
        .expect("Failed to count jobs");

    assert_eq!(
        job_count.0, 3,
        "Expected 3 jobs to be inserted into database"
    );

    // Verify job details
    let jobs: Vec<(i64, String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT id, source_code, language, user_id, contest_id, problem_id FROM jobs ORDER BY id",
    )
    .fetch_all(&db_pool)
    .await
    .expect("Failed to fetch jobs");

    assert_eq!(jobs.len(), 3);

    for (i, (job_id, source_code, language, user_id, contest_id, problem_id)) in
        jobs.iter().enumerate()
    {
        // Don't check exact job_id since it's auto-increment and depends on previous tests
        assert!(job_id > &0, "job_id should be positive");
        assert_eq!(
            source_code,
            &format!("fn main() {{ println!(\"Test {}\"); }}", i)
        );
        assert_eq!(language, "Rust");
        assert_eq!(*user_id, i as i64);
        assert_eq!(*contest_id, 0);
        assert_eq!(*problem_id, 1);
    }
}

#[actix_web::test]
async fn test_concurrent_requests() {
    let (db_pool, db_path) = create_test_db().await;
    let _guard = TestDbGuard::new(db_path);
    let (problems, languages) = create_test_config();
    let job_queue = Arc::new(JobQueue::new());
    let blocking = Arc::new(false);

    tokio::spawn(mock_judger(job_queue.clone()));

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::from(problems))
            .app_data(web::Data::from(languages))
            .app_data(web::Data::from(job_queue))
            .app_data(web::Data::from(blocking))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .service(post_jobs_handler),
    )
    .await;

    // Create multiple concurrent requests
    let mut futures = vec![];

    for i in 0..5 {
        let request_body = json!({
            "source_code": format!("fn main() {{ println!(\"Concurrent {}\"); }}", i),
            "language": "Rust",
            "user_id": i,
            "contest_id": 0,
            "problem_id": 1
        });

        let req = test::TestRequest::post()
            .uri("/jobs")
            .set_json(&request_body)
            .to_request();

        futures.push(test::call_service(&app, req));
    }

    // Wait for all requests to complete
    let mut responses = vec![];
    for future in futures {
        responses.push(future.await);
    }

    // Check that all responses are successful
    for resp in responses {
        assert_eq!(resp.status(), 200);
        let body: JobRecord = test::read_body_json(resp).await;
        assert!(body.id >= 1, "job_id should be positive"); // Don't check upper bound since it depends on previous tests
    }

    // Verify that all jobs were inserted
    let job_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs")
        .fetch_one(&db_pool)
        .await
        .expect("Failed to count jobs");

    assert_eq!(
        job_count.0, 5,
        "Expected 5 jobs to be inserted into database"
    );
}
