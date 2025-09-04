use std::collections::HashMap;

use actix_web::{HttpResponse, Responder, get, web};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;

use super::{ErrorResponse, ErrorResponseWithMessage, User};
use crate::database as db;

#[derive(Deserialize, Debug)]
pub struct RanklistQuery {
    pub scoring_rule: Option<String>,
    pub tie_breaker: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RanklistEntry {
    pub user: User,
    pub rank: u32,
    pub scores: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct UserScore {
    pub user_id: u32,
    pub user_name: String,
    pub problem_scores: HashMap<u32, f64>,
    pub total_score: f64,
    pub latest_submission_time: Option<String>,
    pub submission_count: u32,
}

#[get("/contests/{contest_id}/ranklist")]
pub async fn get_ranklist_handler(
    path: web::Path<u32>,
    query: web::Query<RanklistQuery>,
    pool: web::Data<SqlitePool>,
    problems: web::Data<crate::config::ProblemConfig>,
) -> impl Responder {
    let contest_id = path.into_inner();

    // Currently only support global ranklist (contest_id = 0)
    if contest_id != 0 {
        return HttpResponse::NotFound().json(ErrorResponseWithMessage {
            reason: "ERR_NOT_FOUND",
            code: 3,
            message: format!("Contest {contest_id} not found."),
        });
    }

    // Validate scoring_rule
    if let Some(ref rule) = query.scoring_rule {
        if rule != "latest" && rule != "highest" {
            return HttpResponse::BadRequest().json(ErrorResponseWithMessage {
                reason: "ERR_INVALID_ARGUMENT",
                code: 1,
                message: format!("Invalid scoring_rule: {rule}"),
            });
        }
    }

    // Validate tie_breaker
    if let Some(ref breaker) = query.tie_breaker {
        if breaker != "submission_time" && breaker != "submission_count" && breaker != "user_id" {
            return HttpResponse::BadRequest().json(ErrorResponseWithMessage {
                reason: "ERR_INVALID_ARGUMENT",
                code: 1,
                message: format!("Invalid tie_breaker: {breaker}"),
            });
        }
    }

    match db::get_global_ranklist(
        query.scoring_rule.clone(),
        query.tie_breaker.clone(),
        problems.into_inner(),
        pool.into_inner(),
    )
    .await
    {
        Ok(ranklist) => HttpResponse::Ok().json(ranklist),
        Err(e) => {
            log::error!("Failed to get ranklist: {e}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            })
        }
    }
}
