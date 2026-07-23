use crate::core::state::AppState;
use axum::Router;
use std::{env, net::SocketAddr};

mod core;
mod routes;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let state = AppState::new();
    let router = Router::new()
        .merge(routes::root::routes())
        .with_state(state)
        .fallback(routes::root::not_found_handler);

    let app_port = {
        let port = env::var("APP_PORT").unwrap_or_default().trim().to_string();
        if !port.is_empty() {
            port.parse().unwrap_or(3000)
        } else {
            3000
        }
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], app_port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect(&format!("Failed listening on the port: {}", app_port));
    axum::serve(listener, router)
        .await
        .expect("Failed serving and setting up routes with the tcp listener!");
}
