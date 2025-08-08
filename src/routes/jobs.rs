use actix_web::{HttpResponse, Responder, web};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use tokio::sync::{mpsc, oneshot};

use crate::config::{LanguageConfig, ProblemConfig};
use crate::routes::ErrorResponse;

#[derive(Serialize, Deserialize)]
pub struct PostJobsRequest {
    pub source_code: String,
    pub language: String,
    pub user_id: u32,
    pub contest_id: u32,
    pub problem_id: u32,
}

#[derive(Serialize, Deserialize, Default)]
pub struct CaseResult {
    pub id: u32, // index of the case
    pub result: String,
    pub time: u32,
    pub memory: u32,
    pub info: String,
}

#[derive(Serialize, Deserialize)]
pub struct PostJobsResponse {
    pub id: u32,
    pub created_time: String,
    pub updated_time: String,
    pub submission: PostJobsRequest,
    pub state: String,
    pub result: String,
    pub score: f32,
    pub cases: Vec<CaseResult>,
}

pub enum JobMessage {
    FireAndForget {
        job_id: u32,
    },
    Blocking {
        job_id: u32,
        responder: oneshot::Sender<PostJobsResponse>,
    },
}

async fn create_job_in_db(
    body: &web::Json<PostJobsRequest>,
    pool: &web::Data<SqlitePool>,
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
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(result.last_insert_rowid() as u32)
}

pub async fn post_jobs_handler(
    queue_tx: web::Data<mpsc::Sender<JobMessage>>,
    body: web::Json<PostJobsRequest>,
    pool: web::Data<SqlitePool>,
    problems: web::Data<Vec<ProblemConfig>>,
    languages: web::Data<Vec<LanguageConfig>>,
) -> impl Responder {
    let found_language = languages.as_ref().iter().any(|l| l.name == body.language);
    let found_problem_idx = problems
        .as_ref()
        .iter()
        .position(|p| p.id == body.problem_id);

    if !found_language || found_problem_idx.is_none() {
        return HttpResponse::NotFound().json(ErrorResponse {
            reason: "ERR_NOT_FOUND",
            code: 3,
        });
    }

    let job_id = match create_job_in_db(&body, &pool).await {
        Ok(id) => {
            log::info!("job submitted successfully, id = {id}");
            id
        }
        Err(e) => {
            log::error!("failed to insert job into database: {e}");
            return HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            });
        }
    };

    let problem_config = problems.as_ref().get(found_problem_idx.unwrap()).unwrap();

    let non_blocking = problem_config.nonblocking.unwrap_or(false);

    if non_blocking {
        let job_message = JobMessage::FireAndForget { job_id };

        match queue_tx.send(job_message).await {
            Ok(_) => {
                log::debug!("nonblocking job sent to queue, job_id = {job_id}");

                let cases = (0..problem_config.cases.len())
                    .map(|i| CaseResult {
                        id: i as u32,
                        result: "Waiting".to_string(),
                        time: 0,
                        memory: 0,
                        info: "".to_string(),
                    })
                    .collect::<Vec<_>>();

                HttpResponse::Ok().json(PostJobsResponse {
                    id: job_id,
                    created_time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                    updated_time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                    submission: body.into_inner(),
                    state: "Queueing".to_string(),
                    result: "Waiting".to_string(),
                    score: 0.0,
                    cases,
                })
            }
            Err(e) => {
                log::error!("failed to send job message to queue: {e}");
                HttpResponse::InternalServerError().json(ErrorResponse {
                    reason: "ERR_INTERNAL",
                    code: 6,
                })
            }
        }
    } else {
        let (tx, rx) = oneshot::channel::<PostJobsResponse>();
        let job_message = JobMessage::Blocking {
            job_id,
            responder: tx,
        };

        match queue_tx.send(job_message).await {
            Ok(_) => {
                log::debug!("blocking job sent to queue, job_id = {job_id}");
                match rx.await {
                    Ok(response) => {
                        log::info!("job completed successfully, id = {}", response.id);
                        HttpResponse::Ok().json(response)
                    }
                    Err(e) => {
                        log::error!("failed to receive job response: {e}");
                        HttpResponse::InternalServerError().json(ErrorResponse {
                            reason: "ERR_INTERNAL",
                            code: 6,
                        })
                    }
                }
            }
            Err(e) => {
                log::error!("failed to send job message to queue: {e}");
                HttpResponse::InternalServerError().json(ErrorResponse {
                    reason: "ERR_INTERNAL",
                    code: 6,
                })
            }
        }
    }
}

// {
//   "id": 0,
//   "created_time": "2022-08-27T02:05:29.000Z",
//   "updated_time": "2022-08-27T02:05:30.000Z",
//   "submission": {
//     "source_code": "fn main() { println!('Hello World!'); }",
//     "language": "Rust",
//     "user_id": 0,
//     "contest_id": 0,
//     "problem_id": 0
//   },
//   "state": "Queueing",
//   "result": "Waiting",
//   "score": 87.5,
//   "cases": [
//     {
//       "id": 0,
//       "result": "Waiting",
//       "time": 0,
//       "memory": 0,
//       "info": ""
//     },
//     {
//       "id": 1,
//       "result": "Waiting",
//       "time": 0,
//       "memory": 0,
//       "info": ""
//     }
//   ]
// }
