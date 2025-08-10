use std::sync::Arc;

use clap::Parser;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use oj::config::{CliArgs, Config};
use oj::database as db;
use oj::queue::JobQueue;
use oj::web_server::build_server;
use oj::worker::worker;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let db_path = db::get_db_path();
    let cli = CliArgs::parse();
    let Config {
        server: server_config,
        problems: problem_config,
        languages: language_config,
    } = cli.to_config().expect("Failed to load configuration");

    if cli.flush_data {
        db::remove_db(&db_path);
    }

    let db_pool = db::init_db(&db_path)
        .await
        .expect("Failed to initialize database");

    let problem_config = Arc::new(problem_config);
    let language_config = Arc::new(language_config);
    let db_pool = Arc::new(db_pool);
    let job_queue = Arc::new(JobQueue::new());
    let shutdown_token = CancellationToken::new();
    let n_threads = cli.threads;

    let mut workers = JoinSet::new();
    for i in 1..=n_threads {
        workers.spawn(worker(
            i,
            problem_config.clone(),
            language_config.clone(),
            db_pool.clone(),
            job_queue.clone(),
            shutdown_token.clone(),
        ));
    }

    let server = build_server(
        server_config,
        problem_config,
        language_config,
        db_pool,
        job_queue,
    )
    .expect("Failed to build server");

    let server_handle = server.handle();
    let server_task = tokio::spawn(server);

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log::info!("ctrl-c received, shutting down...");
        }
        res = server_task => {
            log::error!("server task finished unexpectedly: {:?}", res);
        }
    }

    // 1. Shutdown actix-web server gracefully
    server_handle.stop(true).await;

    // 2. Broadcast shutdown signal to workers
    shutdown_token.cancel();
    log::info!("shutdown signal sent to workers, waiting for them to finish...");

    // 3. Wait until every worker terminates
    while let Some(res) = workers.join_next().await {
        if let Err(e) = res {
            if e.is_panic() {
                log::error!("worker handle panicked: {:?}", e);
            } else {
                log::error!("worker handle finished with error: {:?}", e);
            }
        }
    }

    log::info!("shutdown complete");
    Ok(())
}
