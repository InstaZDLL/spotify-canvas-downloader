use std::env;

use spotify_canvas_downloader::{config::Config, spotify::SpotifyClient};

#[tokio::test]
#[ignore = "requires SP_DC and SPOTIFY_TEST_TRACK_ID in the local environment"]
async fn resolves_a_real_spotify_canvas() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("valid Spotify configuration");
    let track_id = env::var("SPOTIFY_TEST_TRACK_ID")
        .expect("SPOTIFY_TEST_TRACK_ID must identify a track with a Canvas");
    let url = SpotifyClient::new(config.spotify)
        .expect("Spotify client")
        .canvas_for_track(&track_id)
        .await
        .expect("Spotify request")
        .expect("the track should have a Canvas");

    assert!(url.starts_with("https://"));
}
