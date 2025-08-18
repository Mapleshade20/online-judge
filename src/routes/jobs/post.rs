use super::*;

#[post("/jobs")]
pub async fn post_job_handler(
    job_queue: web::Data<JobQueue>,
    pool: web::Data<SqlitePool>,
    problems: web::Data<ProblemConfig>,
    languages: web::Data<LanguageConfig>,
    blocking: web::Data<bool>,
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

    // Check if user ID exists
    let user_exists = match db::find_user(body.user_id, pool.clone().into_inner()).await {
        Ok(exists) => exists,
        Err(e) => {
            log::error!("Failed to check user existence: {e}");
            return HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            });
        }
    };

    if !user_exists {
        return HttpResponse::NotFound().json(ErrorResponse {
            reason: "ERR_NOT_FOUND",
            code: 3,
        });
    }

    let problem = problems.as_ref().get(found_problem_idx.unwrap()).unwrap();
    let total_cases = 1 + problem.cases.len() as u32; // Compile is case 0

    let job_id = match db::create_job(&body, pool.into_inner(), total_cases).await {
        Ok(id) => {
            log::info!("Inserted job {id} into databse");
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

    handle_job_submission(
        job_id,
        &job_queue,
        **blocking,
        body.into_inner(),
        problem.cases.len(),
    )
    .await
}

pub(super) async fn handle_job_submission(
    job_id: u32,
    job_queue: &JobQueue,
    blocking: bool,
    submission: JobSubmission,
    cases_count: usize,
) -> HttpResponse {
    if blocking {
        let (tx, rx) = oneshot::channel::<JobRecord>();
        let job_message = JobMessage::Blocking {
            job_id,
            responder: tx,
        };

        job_queue.push(job_message);
        log::debug!("Sent blocking job {job_id} to queue");

        match rx.await {
            Ok(response) => {
                log::info!("Received final result of blocking job {}", response.id);
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
    } else {
        let job_message = JobMessage::FireAndForget { job_id };

        job_queue.push(job_message);
        log::debug!("Sent non-blocking job {job_id} to queue");

        // Pre-allocate the cases vector with known capacity to avoid reallocations
        let mut cases = Vec::with_capacity(cases_count + 1);
        for i in 0..=cases_count {
            cases.push(CaseResult {
                id: i as u32,
                result: crate::memory_optimization::get_or_create_string("Waiting"),
                time: 0,
                memory: 0,
                info: String::new(), // Use String::new() instead of "".to_string()
            });
        }

        // Cache the timestamp to avoid multiple UTC calls
        let now = crate::memory_optimization::create_timestamp();

        HttpResponse::Ok().json(JobRecord {
            id: job_id,
            created_time: now.clone(),
            updated_time: now,
            submission,
            state: crate::memory_optimization::get_or_create_string("Queueing"),
            result: crate::memory_optimization::get_or_create_string("Waiting"),
            score: 0.0,
            cases,
        })
    }
}
