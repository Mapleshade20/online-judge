mod delete;
mod get;
mod post;
mod put;

pub use delete::delete_job_handler;
pub use get::{get_job_by_id_handler, get_jobs_handler};
pub use post::post_job_handler;
pub use put::put_job_handler;

use actix_web::{HttpResponse, Responder, delete, get, post, put, web};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use tokio::sync::oneshot;

use super::{ErrorResponse, ErrorResponseWithMessage};
use crate::config::{LanguageConfig, ProblemConfig};
use crate::create_timestamp;
use crate::database as db;
use crate::queue::JobQueue;

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

#[derive(Deserialize)]
pub struct JobsQueryParams {
    pub user_id: Option<u32>,
    pub user_name: Option<String>,
    pub contest_id: Option<u32>,
    pub problem_id: Option<u32>,
    pub language: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub state: Option<String>,
    pub result: Option<String>,
}
