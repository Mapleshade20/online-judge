mod contests;
mod jobs;
mod users;

// pub use contests::*;
pub use jobs::*;
// pub use users::*;

use actix_web::error::{InternalError, JsonPayloadError, QueryPayloadError};
use actix_web::{HttpRequest, HttpResponse, Responder, post};
use serde::Serialize;

#[derive(Serialize)]
struct ErrorResponse {
    reason: &'static str,
    code: u32,
}

#[derive(Serialize)]
struct ErrorResponseWithMessage {
    reason: &'static str,
    code: u32,
    message: String,
}

pub fn json_error_handler(err: JsonPayloadError, _req: &HttpRequest) -> actix_web::Error {
    let response = HttpResponse::BadRequest().json(ErrorResponse {
        reason: "ERR_INVALID_ARGUMENT",
        code: 1,
    });
    InternalError::from_response(err, response).into()
}

pub fn query_error_handler(err: QueryPayloadError, _req: &HttpRequest) -> actix_web::Error {
    let response = HttpResponse::BadRequest().json(ErrorResponse {
        reason: "ERR_INVALID_ARGUMENT",
        code: 1,
    });
    InternalError::from_response(err, response).into()
}

/// NOTE: DO NOT REMOVE: used in automatic testing
#[post("/internal/exit")]
#[allow(unreachable_code)]
pub async fn exit() -> impl Responder {
    log::info!("Shutdown as requested");
    std::process::exit(0);
    "Exited".to_string()
}
