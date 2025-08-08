// use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
// use sqlx::sqlite::SqlitePool;

#[derive(sqlx::FromRow, Serialize)]
struct User {
    id: i64,
    name: String,
}

// #[get("/users")]
// async fn get_users(pool: web::Data<SqlitePool>) -> impl Responder {
//     let result = sqlx::query_as::<_, User>("SELECT id, name FROM users")
//         .fetch_all(pool.get_ref())
//         .await;
//
//     match result {
//         Ok(users) => HttpResponse::Ok().json(users),
//         Err(e) => {
//             eprintln!("Failed to fetch users: {e}");
//             HttpResponse::InternalServerError().body("Error fetching data from database")
//         }
//     }
// }
