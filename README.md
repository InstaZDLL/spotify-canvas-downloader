# Spotify Canvas Downloader

A small Rust HTTP service that resolves the Canvas media URL for a Spotify track.

> [!WARNING]
> Spotify does not publish a Canvas API. This project relies on private Spotify endpoints that can change without notice. The token endpoint currently states that its use is not permitted under Spotify's Developer Terms and Developer Policy. Review those terms before deploying this service.

## Requirements

- Rust 1.85 or newer
- A Spotify browser session with a valid `sp_dc` cookie

The protobuf compiler is bundled as a build dependency; no system `protoc` installation is required.

## Run Locally

```sh
cp env.example .env
# Set SP_DC in .env, then run:
cargo run
```

The service listens on `127.0.0.1:8000` by default. Check it with:

```sh
curl http://localhost:8000/api/health
curl http://localhost:8000/api/canvas/2qSkIjg1o9h3YT9RAgYN75
```

`BIND_ADDR` changes the listen address. `HOST_ORIGIN` adds one allowed CORS origin. `SP_KEY` is optional.

## API

`GET /api/canvas/{track_id}` accepts a 22-character Spotify track ID.

```json
{"success":true,"canvas_url":"https://..."}
```

Invalid IDs return `400`, missing Canvases return `404`, Spotify failures return `502`, and upstream rate limits return `503`. Errors use `{"success":false,"code":"...","message":"..."}`.

`GET /api/health` returns `{"status":"up"}` without contacting Spotify.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

To exercise the private Spotify endpoints, set `SPOTIFY_TEST_TRACK_ID` to a track known to have a Canvas and run:

```sh
cargo test --test live_spotify -- --ignored
```

## License

This project is licensed under the GNU General Public License v3.0. See [`LICENSE`](LICENSE).
