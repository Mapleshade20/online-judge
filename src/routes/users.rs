use std::sync::Arc;

use actix_web::{HttpResponse, Responder, get, post, web};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;

use super::{ErrorResponse, ErrorResponseWithMessage};
use crate::database as db;

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct UserRequest {
    pub id: Option<u32>,
    pub name: String,
}

#[get("/users")]
pub async fn get_users_handler(pool: web::Data<SqlitePool>) -> impl Responder {
    match db::get_users(pool.into_inner()).await {
        Ok(users) => HttpResponse::Ok().json(users),
        Err(e) => {
            log::error!("Failed to fetch users: {e}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            })
        }
    }
}

#[post("/users")]
pub async fn post_users_handler(
    body: web::Json<UserRequest>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let pool = pool.into_inner();

    // Check if we're updating an existing user or creating a new one
    if let Some(user_id) = body.id {
        update_user(user_id, &body.name, pool).await
    } else {
        create_user(&body.name, pool).await
    }
}

async fn update_user(user_id: u32, new_name: &str, pool: Arc<SqlitePool>) -> HttpResponse {
    // Check if user exists
    match db::find_user(user_id, pool.clone()).await {
        Ok(true) => {
            // User exists, check for name conflicts with other users
            match db::user_name_exists(new_name, Some(user_id), pool.clone()).await {
                Ok(true) => {
                    // Name already exists for another user
                    HttpResponse::BadRequest().json(ErrorResponseWithMessage {
                        reason: "ERR_INVALID_ARGUMENT",
                        code: 1,
                        message: format!("User name '{}' already exists.", new_name),
                    })
                }
                Ok(false) => {
                    // No conflict, update the user
                    match db::update_user(user_id, new_name, pool).await {
                        Ok(user) => HttpResponse::Ok().json(user),
                        Err(e) => {
                            log::error!("Failed to update user: {e}");
                            HttpResponse::InternalServerError().json(ErrorResponse {
                                reason: "ERR_EXTERNAL",
                                code: 5,
                            })
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to check name conflict: {e}");
                    HttpResponse::InternalServerError().json(ErrorResponse {
                        reason: "ERR_EXTERNAL",
                        code: 5,
                    })
                }
            }
        }
        Ok(false) => {
            // User doesn't exist
            HttpResponse::NotFound().json(ErrorResponseWithMessage {
                reason: "ERR_NOT_FOUND",
                code: 3,
                message: format!("User {} not found.", user_id),
            })
        }
        Err(e) => {
            log::error!("Failed to check user existence: {e}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            })
        }
    }
}

async fn create_user(name: &str, pool: Arc<SqlitePool>) -> HttpResponse {
    // Check if name already exists
    match db::user_name_exists(name, None, pool.clone()).await {
        Ok(true) => {
            // Name already exists
            HttpResponse::BadRequest().json(ErrorResponseWithMessage {
                reason: "ERR_INVALID_ARGUMENT",
                code: 1,
                message: format!("User name '{}' already exists.", name),
            })
        }
        Ok(false) => {
            // Name doesn't exist, create new user
            match db::create_user(name, pool).await {
                Ok(user) => HttpResponse::Ok().json(user),
                Err(e) => {
                    log::error!("Failed to create user in database: {e}");
                    HttpResponse::InternalServerError().json(ErrorResponse {
                        reason: "ERR_EXTERNAL",
                        code: 5,
                    })
                }
            }
        }
        Err(e) => {
            log::error!("Failed to check name existence: {e}");
            HttpResponse::InternalServerError().json(ErrorResponse {
                reason: "ERR_EXTERNAL",
                code: 5,
            })
        }
    }
}
