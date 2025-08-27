use super::*;

#[put("/jobs/{id}")]
pub async fn put_job_handler(
    job_queue: web::Data<JobQueue>,
    pool: web::Data<SqlitePool>,
    path: web::Path<(u32,)>,
    blocking: web::Data<bool>,
) -> impl Responder {
    let job_id = path.into_inner().0;

    match db::fetch_job(job_id, pool.clone().into_inner()).await {
        Ok(record) if record.state == "Finished" || record.state == "Canceled" => {
            match db::revert_job_to_queueing(job_id, pool.into_inner()).await {
                Ok(reverted_cases) => {
                    super::post::handle_job_submission(
                        job_id,
                        job_queue.get_ref(),
                        **blocking,
                        record.submission,
                        reverted_cases,
                    )
                    .await
                }
                Err(e) => {
                    log::error!("Failed to revert job to queueing in database: {e}");
                    HttpResponse::InternalServerError().json(ErrorResponse {
                        reason: "ERR_EXTERNAL",
                        code: 5,
                    })
                }
            }
        }
        Ok(record) => {
            log::info!(
                "Put nothing because job {} was in state {}",
                job_id,
                record.state
            );
            HttpResponse::BadRequest().json(ErrorResponseWithMessage {
                reason: "ERR_INVALID_STATE",
                code: 2,
                message: format!("Job {job_id} not finished."),
            })
        }
        Err(sqlx::Error::RowNotFound) => {
            log::info!("Put nothing because job {job_id} was not found");
            HttpResponse::NotFound().json(ErrorResponseWithMessage {
                reason: "ERR_NOT_FOUND",
                code: 3,
                message: format!("Job {job_id} not found."),
            })
        }
        Err(e) => {
            log::error!("Failed to retrieve job state from database: {e}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            })
        }
    }
}
