pub mod config;
pub mod database;
pub mod queue;
pub mod routes;
pub mod sandbox;
pub mod web_server;
pub mod worker;

pub fn create_timestamp() -> String {
    use chrono::{SecondsFormat, Utc};
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
