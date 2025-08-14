use std::sync::Arc;

use actix_web::{App, HttpServer, dev::Server, middleware, web};
use sqlx::sqlite::SqlitePool;

use crate::config::{LanguageConfig, ProblemConfig, ServerConfig};
use crate::queue::JobQueue;
use crate::routes::{
    delete_job_handler, exit, get_job_by_id_handler, get_jobs_handler, get_users_handler,
    json_error_handler, post_job_handler, post_users_handler, put_job_handler, query_error_handler,
};

pub fn build_server(
    server_config: ServerConfig,
    problems: Arc<ProblemConfig>,
    languages: Arc<LanguageConfig>,
    db_pool: Arc<SqlitePool>,
    job_queue: Arc<JobQueue>,
) -> std::io::Result<Server> {
    let db_pool = web::Data::from(db_pool);
    let problems = web::Data::from(problems);
    let languages = web::Data::from(languages);
    let job_queue = web::Data::from(job_queue); // Construct directly from Arc
    let blocking = web::Data::new(server_config.blocking.unwrap_or(false));

    let server = HttpServer::new(move || {
        App::new()
            .app_data(db_pool.clone())
            .app_data(problems.clone())
            .app_data(languages.clone())
            .app_data(job_queue.clone())
            .app_data(blocking.clone())
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .app_data(web::QueryConfig::default().error_handler(query_error_handler))
            .wrap(middleware::Logger::default())
            .service(post_job_handler)
            .service(get_job_by_id_handler)
            .service(get_jobs_handler)
            .service(put_job_handler)
            .service(delete_job_handler)
            .service(get_users_handler)
            .service(post_users_handler)
            .service(exit)
    })
    .bind((
        server_config
            .bind_address
            .unwrap_or("127.0.0.1".to_string()),
        server_config.bind_port.unwrap_or(12345),
    ))?
    .workers(5) // TODO: examine the server's internal workers count
    .run();

    Ok(server)
}
