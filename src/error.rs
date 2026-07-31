use axum::{Json, response::IntoResponse};
use http::{HeaderValue, StatusCode, header::RETRY_AFTER};
use serde::Serialize;

use crate::spotify::SpotifyError;

#[derive(Debug)]
pub enum AppError {
    InvalidTrackId,
    CanvasNotFound,
    Spotify(SpotifyError),
}

#[derive(Serialize)]
struct ErrorBody {
    success: bool,
    code: &'static str,
    message: &'static str,
}

impl From<SpotifyError> for AppError {
    fn from(error: SpotifyError) -> Self {
        Self::Spotify(error)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message, retry_after) = match self {
            Self::InvalidTrackId => (
                StatusCode::BAD_REQUEST,
                "invalid_track_id",
                "Track ID must contain exactly 22 base62 characters",
                None,
            ),
            Self::CanvasNotFound => (
                StatusCode::NOT_FOUND,
                "canvas_not_found",
                "No Canvas was found for this track",
                None,
            ),
            Self::Spotify(SpotifyError::RateLimited(retry_after)) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "spotify_rate_limited",
                "Spotify temporarily rate limited the request",
                retry_after,
            ),
            Self::Spotify(SpotifyError::Authentication(_)) => (
                StatusCode::BAD_GATEWAY,
                "spotify_auth_failed",
                "Spotify authentication failed",
                None,
            ),
            Self::Spotify(_) => (
                StatusCode::BAD_GATEWAY,
                "spotify_upstream_error",
                "Spotify returned an invalid or unavailable response",
                None,
            ),
        };

        let mut response = (
            status,
            Json(ErrorBody {
                success: false,
                code,
                message,
            }),
        )
            .into_response();
        if let Some(value) = retry_after.and_then(|value| HeaderValue::from_str(&value).ok()) {
            response.headers_mut().insert(RETRY_AFTER, value);
        }
        response
    }
}
