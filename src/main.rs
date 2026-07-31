use spotify_canvas_downloader::{api, config::Config, spotify::SpotifyClient};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env()?;
    let bind_addr = config.bind_addr;
    let client = SpotifyClient::new(config.spotify)?;
    let app = api::router(client, config.host_origin.as_deref())?;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    tracing::info!(%bind_addr, "Spotify Canvas service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
