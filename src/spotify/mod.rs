mod auth;
mod canvas;

use std::{future::Future, pin::Pin};

pub use auth::TotpSecret;
pub use canvas::SpotifyClient;

pub(crate) fn retry_after(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

pub trait CanvasLookup: Send + Sync {
    fn canvas_for_track<'a>(
        &'a self,
        track_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, SpotifyError>> + Send + 'a>>;
}

impl CanvasLookup for SpotifyClient {
    fn canvas_for_track<'a>(
        &'a self,
        track_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, SpotifyError>> + Send + 'a>> {
        Box::pin(SpotifyClient::canvas_for_track(self, track_id))
    }
}

#[derive(Clone, Debug)]
pub struct SpotifySettings {
    pub sp_dc: String,
    pub sp_key: Option<String>,
    pub token_url: String,
    pub canvas_url: String,
    pub catalog_url: String,
    pub totp_override: Option<TotpSecret>,
}

#[derive(Debug, thiserror::Error)]
pub enum SpotifyError {
    #[error("Spotify authentication failed: {0}")]
    Authentication(String),
    #[error("Spotify rate limited the request")]
    RateLimited(Option<String>),
    #[error("Spotify HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Spotify returned HTTP {0}")]
    Upstream(http::StatusCode),
    #[error("Spotify returned malformed protobuf: {0}")]
    Protobuf(#[from] prost::DecodeError),
}
