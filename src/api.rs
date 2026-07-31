use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderValue, Method},
    routing::get,
};
use serde::Serialize;
use std::sync::Arc;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};

use crate::{error::AppError, spotify::CanvasLookup};

#[derive(Clone)]
struct AppState {
    spotify: Arc<dyn CanvasLookup>,
}

#[derive(Serialize)]
struct CanvasResponse {
    success: bool,
    canvas_url: String,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

pub fn router<S>(
    spotify: S,
    host_origin: Option<&str>,
) -> Result<Router, http::header::InvalidHeaderValue>
where
    S: CanvasLookup + 'static,
{
    let mut origins = Vec::new();
    if cfg!(debug_assertions) {
        origins.push(HeaderValue::from_static("http://localhost:3000"));
    }
    if let Some(origin) = host_origin {
        let origin = HeaderValue::from_str(origin)?;
        if !origins.contains(&origin) {
            origins.push(origin);
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET]);

    Ok(Router::new()
        .route("/api/canvas/{track_id}", get(get_canvas))
        .route("/api/health", get(health))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(AppState {
            spotify: Arc::new(spotify),
        }))
}

async fn get_canvas(
    State(state): State<AppState>,
    Path(track_id): Path<String>,
) -> Result<Json<CanvasResponse>, AppError> {
    if !is_valid_track_id(&track_id) {
        return Err(AppError::InvalidTrackId);
    }

    let canvas_url = state
        .spotify
        .canvas_for_track(&track_id)
        .await?
        .ok_or(AppError::CanvasNotFound)?;
    Ok(Json(CanvasResponse {
        success: true,
        canvas_url,
    }))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "up" })
}

fn is_valid_track_id(track_id: &str) -> bool {
    track_id.len() == 22 && track_id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::is_valid_track_id;

    #[test]
    fn validates_spotify_track_ids() {
        assert!(is_valid_track_id("2qSkIjg1o9h3YT9RAgYN75"));
        assert!(!is_valid_track_id("short"));
        assert!(!is_valid_track_id("2qSkIjg1o9h3YT9RAgYN7-"));
    }
}
