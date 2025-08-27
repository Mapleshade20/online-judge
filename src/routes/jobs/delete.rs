use super::*;

#[delete("/jobs/{id}")]
pub async fn delete_job_handler(
    job_queue: web::Data<JobQueue>,
    pool: web::Data<SqlitePool>,
    path: web::Path<(u32,)>,
) -> impl Responder {
    let job_id = path.into_inner().0;
    if job_queue.cancel_job(job_id) {
        match db::update_job_to_canceled(job_id, pool.into_inner()).await {
            Ok(_) => HttpResponse::Ok().finish(),
            Err(e) => {
                log::error!("Failed to update job {job_id} status to Canceled: {e}");
                HttpResponse::InternalServerError().json(ErrorResponse {
                    reason: "ERR_EXTERNAL",
                    code: 5,
                })
            }
        }
    } else {
        match db::find_job(job_id, pool.into_inner()).await {
            Ok(exists) if exists => {
                // Job exists in the database but not in queueing state
                HttpResponse::BadRequest().json(ErrorResponseWithMessage {
                    reason: "ERR_INVALID_STATE",
                    code: 2,
                    message: format!("Job {job_id} not queueing."),
                })
            }
            Ok(_) => {
                // Job does not exist
                HttpResponse::NotFound().json(ErrorResponseWithMessage {
                    reason: "ERR_NOT_FOUND",
                    code: 3,
                    message: format!("Job {job_id} not found."),
                })
            }
            Err(e) => {
                log::error!("Failed to validate if job exists: {e}");
                HttpResponse::InternalServerError().json(ErrorResponse {
                    reason: "ERR_EXTERNAL",
                    code: 5,
                })
            }
        }
    }
}
