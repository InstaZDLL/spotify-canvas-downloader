use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use axum::{body::Body, http::Request};
use spotify_canvas_downloader::{
    api,
    spotify::{CanvasLookup, SpotifyError},
};
use tower::ServiceExt;

const TRACK_ID: &str = "2qSkIjg1o9h3YT9RAgYN75";

#[derive(Clone, Copy)]
enum Outcome {
    Canvas,
    Missing,
    Authentication,
    RateLimited,
    Upstream,
}

#[derive(Clone)]
struct MockLookup {
    outcome: Outcome,
    calls: Arc<AtomicUsize>,
}

impl CanvasLookup for MockLookup {
    fn canvas_for_track<'a>(
        &'a self,
        _track_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, SpotifyError>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            match self.outcome {
                Outcome::Canvas => Ok(Some("https://canvas.scdn.co/example.mp4".into())),
                Outcome::Missing => Ok(None),
                Outcome::Authentication => Err(SpotifyError::Authentication("invalid".into())),
                Outcome::RateLimited => Err(SpotifyError::RateLimited(Some("12".into()))),
                Outcome::Upstream => Err(SpotifyError::Upstream(http::StatusCode::BAD_GATEWAY)),
            }
        })
    }
}

#[tokio::test]
async fn resolves_a_canvas_url() {
    let (app, calls) = test_router(Outcome::Canvas);
    let response = app.oneshot(canvas_request(TRACK_ID)).await.unwrap();

    assert_eq!(response.status(), 200);
    let json = response_json(response).await;
    assert_eq!(json["success"], true);
    assert_eq!(json["canvas_url"], "https://canvas.scdn.co/example.mp4");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn maps_missing_canvas_to_not_found() {
    let (app, _) = test_router(Outcome::Missing);
    let response = app.oneshot(canvas_request(TRACK_ID)).await.unwrap();

    assert_eq!(response.status(), 404);
    assert_eq!(response_json(response).await["code"], "canvas_not_found");
}

#[tokio::test]
async fn rejects_invalid_track_before_lookup() {
    let (app, calls) = test_router(Outcome::Canvas);
    let response = app.oneshot(canvas_request("not-a-track")).await.unwrap();

    assert_eq!(response.status(), 400);
    assert_eq!(response_json(response).await["code"], "invalid_track_id");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn maps_authentication_failures() {
    let (app, _) = test_router(Outcome::Authentication);
    let response = app.oneshot(canvas_request(TRACK_ID)).await.unwrap();

    assert_eq!(response.status(), 502);
    assert_eq!(response_json(response).await["code"], "spotify_auth_failed");
}

#[tokio::test]
async fn propagates_spotify_rate_limit_delay() {
    let (app, _) = test_router(Outcome::RateLimited);
    let response = app.oneshot(canvas_request(TRACK_ID)).await.unwrap();

    assert_eq!(response.status(), 503);
    assert_eq!(response.headers()["retry-after"], "12");
    assert_eq!(
        response_json(response).await["code"],
        "spotify_rate_limited"
    );
}

#[tokio::test]
async fn maps_other_upstream_failures() {
    let (app, _) = test_router(Outcome::Upstream);
    let response = app.oneshot(canvas_request(TRACK_ID)).await.unwrap();

    assert_eq!(response.status(), 502);
    assert_eq!(
        response_json(response).await["code"],
        "spotify_upstream_error"
    );
}

#[tokio::test]
async fn health_is_local_and_structured() {
    let (app, calls) = test_router(Outcome::Canvas);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response_json(response).await["status"], "up");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

fn test_router(outcome: Outcome) -> (axum::Router, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let lookup = MockLookup {
        outcome,
        calls: calls.clone(),
    };
    (api::router(lookup, None).unwrap(), calls)
}

fn canvas_request(track_id: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/canvas/{track_id}"))
        .body(Body::empty())
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}
