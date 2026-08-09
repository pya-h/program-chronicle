use axum::http::StatusCode;

use crate::core::response::ApiResponse;

#[test]
fn ok_uses_200() {
    let resp = ApiResponse::ok(42);
    assert_eq!(resp.0, StatusCode::OK);
    assert_eq!(resp.1.0, 42);
}

#[test]
fn created_uses_201() {
    let resp = ApiResponse::created("x");
    assert_eq!(resp.0, StatusCode::CREATED);
}

#[test]
fn new_uses_the_given_status() {
    let resp = ApiResponse::new(StatusCode::ACCEPTED, ());
    assert_eq!(resp.0, StatusCode::ACCEPTED);
}

#[test]
fn from_defaults_to_ok() {
    let resp: ApiResponse<i32> = 5.into();
    assert_eq!(resp.0, StatusCode::OK);
    assert_eq!(resp.1.0, 5);
}
