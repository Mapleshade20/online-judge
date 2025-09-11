use std::process::Command;
use std::sync::Arc;

use clap::Parser;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use oj::config::{CliArgs, Config};
use oj::database as db;
use oj::queue::JobQueue;
use oj::web_server::build_server;
use oj::worker::worker;

/// Check if a command exists in the system PATH
fn check_command_exists(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if the current user is root and warn if so
fn check_running_user() {
    if std::env::var("USER").unwrap_or_default() == "root"
        || std::env::var("LOGNAME").unwrap_or_default() == "root"
        || unsafe { libc::getuid() } == 0
    {
        log::warn!("WARNING: Running as root user is not recommended for security reasons!");
        log::warn!("Please consider running this application with a non-privileged user account.");
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cli = CliArgs::parse();
    let log_level = if cli.verbose { "debug" } else { "info" };
    env_logger::init_from_env(env_logger::Env::new().default_filter_or(log_level));

    let n_threads = cli.threads;
    if n_threads == 0 {
        log::error!("The number of worker threads must not be 0");
        std::process::exit(1);
    }

    // Check execution mode
    let no_isolate_mode = !check_command_exists("isolate");
    if no_isolate_mode {
        log::warn!("Running in NO_ISOLATE mode - security isolation disabled!");
        log::warn!("This mode should only be used in trusted development environments.");
    }

    if !check_command_exists("sqlite3") {
        log::error!("Required command 'sqlite3' not found. Please install SQLite3.");
        std::process::exit(1);
    }

    // Check running user and warn if running as root
    check_running_user();

    let Config {
        server: server_config,
        problems: problem_config,
        languages: language_config,
    } = cli.read_config().unwrap_or_else(|e| {
        log::error!("Failed to read configuration: {e}");
        std::process::exit(1);
    });

    let db_path = db::get_db_path().unwrap_or_else(|| {
        log::error!("Failed to determine database path");
        std::process::exit(1);
    });
    if cli.flush_data {
        db::remove_db(&db_path);
    }
    let db_pool = db::init_db(&db_path).await.unwrap_or_else(|e| {
        log::error!("Failed to initialize database: {e}");
        std::process::exit(1);
    });

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
    .unwrap_or_else(|e| {
        log::error!("Failed to start web server: {e}");
        std::process::exit(1);
    });

    let server_handle = server.handle();
    let server_task = actix_web::rt::spawn(server);

    // ===== EXECUTION END, WAITING FOR SHUTDOWN ======

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl-c received, shutting down...");
        }
        res_server = server_task => {
            log::error!("Server terminated unexpectedly: {res_server:?}");
        }
        Some(res_worker) = workers.join_next() => {
            log::error!("A worker terminated unexpectedly: {res_worker:?}");
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
                log::error!("Worker handle panicked: {e:?}");
            } else {
                log::error!("Worker handle finished with error: {e:?}");
            }
        }
    }

    log::info!("Shutdown complete");
    Ok(())
}
