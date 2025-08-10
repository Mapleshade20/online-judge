use std::sync::Arc;

use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use crate::config::{LanguageConfig, ProblemConfig};
use crate::database as db;
use crate::judge;
use crate::queue::JobQueue;
use crate::routes::JobMessage;

pub async fn worker(
    id: u8,
    problems: Arc<ProblemConfig>,
    languages: Arc<LanguageConfig>,
    db_pool: Arc<SqlitePool>,
    queue: Arc<JobQueue>,
    token: CancellationToken,
) {
    log::info!("worker {id} initialized");

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                log::info!("worker {id} received shutdown signal, stopping");
                break;
            }

            job_message = queue.pop() => {
                let job_id = job_message.id();

                // 1. Get full job from database
                let job = db::fetch_job(job_id, db_pool.clone()).await;
                let job = match job {
                    Ok(job) => job,
                    Err(e) => {
                        log::error!("failed to fetch job {job_id} from database, job discarded: {e}");
                        continue; // Skip to the next iteration
                    }
                };
                log::info!("worker {id} got job {job_id} from queue");

                let problem_config = problems
                    .iter()
                    .find(|p| p.id == job.submission.problem_id)
                    .cloned();
                let language_config = languages
                    .iter()
                    .find(|l| l.name == job.submission.language)
                    .cloned();

                if problem_config.is_none() || language_config.is_none() {
                    log::error!(
                        "missing config for problem {} or language {}, job {job_id} discarded",
                        job.submission.problem_id,
                        job.submission.language
                    );
                    continue; // Skip to the next iteration
                }

                // 2. Spawn blocking judger and get its handle
                let result_handle = tokio::task::spawn_blocking(move || {
                    let result = judge::run(job, problem_config.unwrap(), language_config.unwrap()); // DEBUG: not supposed to panic
                    log::info!("job {job_id} finished on worker {id}");

                    result
                });

                // 3. Give back control to the runtime until job is done
                match result_handle.await {
                    Ok(job_result) => {
                        db::save_result(job_id, db_pool.clone(), &job_result)
                            .await
                            .unwrap_or_else(|e| log::error!("failed to save job {job_id} result: {e}"));

                        if let JobMessage::Blocking { responder, .. } = job_message {
                            if responder.send(job_result).is_err() {
                                log::warn!("failed to send blocking job {job_id} result back, receiver dropped");
                            } else {
                                log::debug!("blocking job {job_id} result sent back from worker {id}");
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("spawning job {job_id} panicked on worker {id}: {e}");
                    }
                }
            }
        };
    }

    log::info!("worker {id} has shut down gracefully");
}
