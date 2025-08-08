use clap::Parser;
use oj::routes::JobMessage;
use tokio::sync::mpsc;

use oj::config::CliArgs;
use oj::web_server::{build_server, get_db_path, init_db, remove_db};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let db_path = get_db_path();
    let cli = CliArgs::parse();
    let config = cli.to_config().expect("Failed to load configuration");

    if cli.flush_data {
        remove_db(&db_path);
    }

    let db_pool = init_db(&db_path)
        .await
        .expect("Failed to initialize database");

    let (tx, mut rx) = mpsc::channel::<JobMessage>(100);

    build_server(config, db_pool, tx)
        .expect("Failed to start server")
        .await?;

    todo!()
}
