use std::sync::Arc;

use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use crate::config::{LanguageConfig, ProblemConfig};
use crate::database as db;
use crate::queue::JobQueue;
use crate::routes::JobMessage;
use crate::sandbox::create_sandbox_runner;

pub async fn worker(
    id: u8,
    problems: Arc<ProblemConfig>,
    languages: Arc<LanguageConfig>,
    db_pool: Arc<SqlitePool>,
    queue: Arc<JobQueue>,
    token: CancellationToken,
) -> anyhow::Result<()> {
    let sandbox = Arc::new(create_sandbox_runner(id)?);
    log::info!("Worker {id} initialized");

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                log::info!("Worker {id} received shutdown signal, stopping");
                break;
            }

            job_message = queue.pop() => {
                let job_id = job_message.id();

                // 1. Get full job from database
                let job = db::fetch_job(job_id, db_pool.clone()).await;
                let job = match job {
                    Ok(job) => job,
                    Err(e) => {
                        log::error!("Failed to fetch job {job_id} from database, job discarded: {e}");
                        continue; // Skip to the next iteration
                    }
                };
                log::info!("Worker {id} got job {job_id} from queue");

                // 2. Update job status to Running
                if let Err(e) = db::update_job_to_running(job_id, db_pool.clone()).await {
                    log::error!("Failed to update job {job_id} status to Running: {e}");
                    continue; // Skip to the next iteration
                }

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
                        "Missing config for problem {} or language {}, job {job_id} discarded",
                        job.submission.problem_id,
                        job.submission.language
                    );
                    continue; // Skip to the next iteration
                }

                // 3. Spawn blocking judger and get its handle
                let sandbox_ref = Arc::clone(&sandbox);
                let result_handle = tokio::task::spawn_blocking(move || {
                    let result = sandbox_ref.run(job, problem_config.unwrap(), language_config.unwrap());
                    log::info!("Job {job_id} finished on worker {id}");

                    result
                });

                // 4. Give back control to the runtime until job is done
                match result_handle.await {
                    Ok(Ok(job_result)) => {
                        db::save_result(job_id, db_pool.clone(), &job_result)
                            .await
                            .unwrap_or_else(|e| log::error!("Failed to save job {job_id} result: {e}"));

                        if let JobMessage::Blocking { responder, .. } = job_message {
                            if responder.send(job_result).is_err() {
                                log::warn!("Failed to send blocking job {job_id} result back to server");
                            } else {
                                log::debug!("Blocking job {job_id} result sent back from worker {id}");
                            }
                        }
                    }
                    err => {
                        log::error!("Spawning job {job_id} failed on worker {id}: {err:?}");
                    }
                }
            }
        };
    }

    log::info!("Worker {id} has shut down gracefully");
    Ok(())
}
