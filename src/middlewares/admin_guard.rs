use axum::{
    body::Body,
    extract::{Request, State},
    http::{Response, StatusCode},
    middleware::Next,
};
use crate::core::state::AppState;
use subtle::ConstantTimeEq;

pub async fn admin_guard(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response<Body>, StatusCode> {
    let xsecurity_header = req
        .headers()
        .get("x-security")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if xsecurity_header.as_bytes().ct_ne(state.admin_passkey.as_bytes()).into() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}
