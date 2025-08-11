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

    // TODO: "isolate" existence check
    // TODO: running user check

    let db_path = db::get_db_path();
    let cli = CliArgs::parse();
    let n_threads = cli.threads;

    if n_threads == 0 {
        panic!("The number of worker threads must not be 0");
    }

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

    // ======= PREPARATION END, EXECUTION START =======

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
    let server_task = actix_web::rt::spawn(server);

    // ===== EXECUTION END, WAITING FOR SHUTDOWN ======

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl-c received, shutting down...");
        }
        res_server = server_task => {
            log::error!("Server terminated unexpectedly: {:?}", res_server);
        }
        Some(res_worker) = workers.join_next() => {
            log::error!("A worker terminated unexpectedly: {:?}", res_worker);
        }
    }

    // 1. Shutdown actix-web server gracefully
    server_handle.stop(true).await;

    // 2. Broadcast shutdown signal to workers
    shutdown_token.cancel();
    log::info!("Shutdown signal sent to workers, waiting for them to finish...");

    // 3. Wait until every worker terminates
    while let Some(res) = workers.join_next().await {
        if let Err(e) = res {
            if e.is_panic() {
                log::error!("Worker handle panicked: {:?}", e);
            } else {
                log::error!("Worker handle finished with error: {:?}", e);
            }
        }
    }

    log::info!("Shutdown complete");
    Ok(())
}
