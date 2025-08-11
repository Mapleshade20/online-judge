use actix_web::{HttpResponse, Responder, web};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use tokio::sync::oneshot;

use crate::config::{LanguageConfig, ProblemConfig};
use crate::database as db;
use crate::queue::JobQueue;
use crate::routes::ErrorResponse;

#[derive(Serialize, Deserialize, Debug)]
pub struct JobSubmission {
    pub user_id: u32,
    pub contest_id: u32,
    pub problem_id: u32,
    pub source_code: String,
    pub language: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobRecord {
    pub id: u32,
    pub created_time: String,
    pub updated_time: String,
    pub submission: JobSubmission,
    pub state: String,
    pub result: String,
    pub score: f64,
    pub cases: Vec<CaseResult>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CaseResult {
    pub id: u32, // index of the case
    pub result: String,
    pub time: u32,   // time in microseconds
    pub memory: u32, // memory in KB
    pub info: String,
}

pub enum JobMessage {
    FireAndForget {
        job_id: u32,
    },
    Blocking {
        job_id: u32,
        responder: oneshot::Sender<JobRecord>,
    },
}

impl JobMessage {
    pub fn id(&self) -> u32 {
        match self {
            Self::FireAndForget { job_id } => *job_id,
            Self::Blocking { job_id, .. } => *job_id,
        }
    }
}

pub async fn post_jobs_handler(
    job_queue: web::Data<JobQueue>,
    pool: web::Data<SqlitePool>,
    problems: web::Data<ProblemConfig>,
    languages: web::Data<LanguageConfig>,
    body: web::Json<JobSubmission>,
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

    let job_id = match db::create_job(&body, &pool).await {
        Ok(id) => {
            log::info!("Job submitted successfully, id = {id}");
            id
        }
        Err(e) => {
            log::error!("Failed to insert job into database: {e}");
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

        job_queue.push(job_message).await;
        log::debug!("Non-blocking job sent to queue, job_id = {job_id}");

        let cases = (0..=problem_config.cases.len()) // 0 is compile case, 1..=N are test cases
            .map(|i| CaseResult {
                id: i as u32,
                result: "Waiting".to_string(),
                time: 0,
                memory: 0,
                info: "".to_string(),
            })
            .collect::<Vec<_>>();

        HttpResponse::Ok().json(JobRecord {
            id: job_id,
            created_time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            updated_time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            submission: body.into_inner(),
            state: "Queueing".to_string(),
            result: "Waiting".to_string(),
            score: 0.0,
            cases,
        })
    } else {
        let (tx, rx) = oneshot::channel::<JobRecord>();
        let job_message = JobMessage::Blocking {
            job_id,
            responder: tx,
        };

        job_queue.push(job_message).await;
        log::debug!("Blocking job sent to queue, job_id = {job_id}");

        match rx.await {
            Ok(response) => {
                log::info!("Job completed successfully, id = {}", response.id);
                HttpResponse::Ok().json(response)
            }
            Err(e) => {
                log::error!("Failed to receive job response: {e}");
                HttpResponse::InternalServerError().json(ErrorResponse {
                    reason: "ERR_INTERNAL",
                    code: 6,
                })
            }
        }
    }
}
