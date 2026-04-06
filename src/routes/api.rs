use rocket::get;
use rocket::http::Status;
use rocket::serde::json::json;

use crate::errors::api_error::ApiResponse;

/// keepalive
#[get("/keepalive")]
pub async fn keep_alive() -> ApiResponse {
    ApiResponse { json: json!({ "alive": true }), code: Status::Ok.code }
}
