//! Minimal API router: health + WebSocket stub

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/ws", get(ws_upgrade))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "healthy" })
}

async fn ws_upgrade(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut socket: WebSocket) {
    // Simple heartbeat message then close; a real impl would broadcast device data
    let _ = socket
        .send(Message::Text("{\"messageType\":\"heartbeat\"}".into()))
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _ = socket.close().await;
}
