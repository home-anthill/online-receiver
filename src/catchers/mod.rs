use rocket::catch;
use rocket::http::Status;
use rocket::request::Request;
use tracing::error;

use crate::errors::api_error::ApiError;

#[catch(400)]
pub fn bad_request(_: &Request) -> ApiError {
    error!(target: "app", "catcher 400 - bad_request");
    ApiError { code: Status::BadRequest.code, message: "Bad request".to_string() }
}

#[catch(404)]
pub fn not_found(_: &Request) -> ApiError {
    error!(target: "app", "catcher 404 - not_found");
    ApiError { code: Status::NotFound.code, message: "Not found".to_string() }
}

#[catch(500)]
pub fn internal_server_error(_: &Request) -> ApiError {
    error!(target: "app", "catcher 500 - internal_server_error");
    ApiError { code: Status::InternalServerError.code, message: "Internal server error".to_string() }
}

#[catch(503)]
pub fn service_unavailable(_: &Request) -> ApiError {
    error!(target: "app", "catcher 503 - service_unavailable");
    ApiError { code: Status::ServiceUnavailable.code, message: "Service Unavailable".to_string() }
}
