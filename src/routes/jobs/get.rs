use super::*;

#[get("/jobs")]
pub async fn get_jobs_handler(
    pool: web::Data<SqlitePool>,
    query: web::Query<JobsQueryParams>,
) -> impl Responder {
    if let Some(from_str) = &query.from
        && DateTime::parse_from_rfc3339(from_str).is_err()
    {
        return HttpResponse::BadRequest().json(ErrorResponse {
            reason: "ERR_INVALID_ARGUMENT",
            code: 1,
        });
    }

    let jobs = db::fetch_jobs_by_query(query, pool.into_inner()).await;

    match jobs {
        Ok(records) => {
            log::info!("Got {} job records", records.len());
            HttpResponse::Ok().json(records)
        }
        Err(e) => {
            log::error!("Failed to retrieve job records: {e}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            })
        }
    }
}

#[get("/jobs/{id}")]
pub async fn get_job_by_id_handler(
    pool: web::Data<SqlitePool>,
    path: web::Path<(u32,)>,
) -> impl Responder {
    let job_id = path.into_inner().0;

    match db::fetch_job(job_id, pool.into_inner()).await {
        Ok(record) => {
            log::info!("Got the record of job {job_id} from database");
            HttpResponse::Ok().json(record)
        }
        Err(sqlx::Error::RowNotFound) => {
            log::info!("Got nothing with job id {job_id} from database");
            HttpResponse::NotFound().json(ErrorResponseWithMessage {
                reason: "ERR_NOT_FOUND",
                code: 3,
                message: format!("Job {job_id} not found."),
            })
        }
        Err(e) => {
            log::error!("Failed to retrieve job record from database: {e}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            })
        }
    }
}
