use crate::core::{error::SvmIndexerError, state::AppState};
use axum::{
    body::Body,
    extract::{Request, State},
    http::Response,
    middleware::Next,
};
use subtle::ConstantTimeEq;

pub async fn admin_guard(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response<Body>, SvmIndexerError> {
    let xsecurity_header = req
        .headers()
        .get("x-security")
        .and_then(|h| h.to_str().ok())
        .ok_or(SvmIndexerError::Unauthorized)?;

    if xsecurity_header
        .as_bytes()
        .ct_ne(state.admin_passkey.as_bytes())
        .into()
    {
        return Err(SvmIndexerError::Unauthorized);
    }
    Ok(next.run(req).await)
}
