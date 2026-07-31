# Repository Guidelines

## Project Structure & Module Organization

The maintained service is a Rust crate. `src/main.rs` starts Axum, while `src/api.rs` owns routes and response contracts. Spotify authentication and Canvas requests live under `src/spotify/`; shared configuration and API error mapping are in `src/config.rs` and `src/error.rs`. `proto/canvas.proto` is compiled by `build.rs` with a vendored `protoc`. Integration tests live in `tests/`. A local `legacy-python/` archive may exist but is ignored and is not part of the maintained repository.

## Build, Test, and Development Commands

- `cp env.example .env`: create local configuration; replace the `SP_DC` placeholder.
- `cargo run`: start the API at `127.0.0.1:8000` by default.
- `cargo build --release`: produce the optimized binary in `target/release/`.
- `cargo fmt --check`: verify standard Rust formatting.
- `cargo clippy --all-targets -- -D warnings`: reject lint warnings across code and tests.
- `cargo test`: run deterministic tests; the real Spotify test remains ignored.
- `cargo test --test live_spotify -- --ignored`: validate against Spotify using local secrets and `SPOTIFY_TEST_TRACK_ID`.

## Coding Style & Naming Conventions

Use Rust 2024 conventions and `rustfmt` defaults: four-space indentation, `snake_case` functions/modules, `PascalCase` types, and `SCREAMING_SNAKE_CASE` constants. Keep route handlers thin; authentication, token caching, protobuf handling, and upstream status mapping belong in `src/spotify/`. Model failures with typed errors rather than string matching. Never log cookies, bearer tokens, or complete authentication responses.

## Testing Guidelines

Place focused unit tests beside private implementation details and public behavior tests in `tests/`. Tests must not depend on network access or real credentials. Inject `CanvasLookup` doubles for route tests and use fixed timestamps for TOTP vectors. Cover success, invalid input, missing Canvas, authentication failure, rate limiting, malformed protobuf, and token refresh behavior when changed.

## Commit & Pull Request Guidelines

Follow the existing Conventional Commit-style history: `feat(scope): ...`, `fix: ...`, `refactor: ...`, `build: ...`, or `docs: ...`. Keep commits focused. Pull requests should describe API or configuration changes, link relevant issues, and report `fmt`, Clippy, and test results. Never commit `.env`, `SP_DC`, `SP_KEY`, access tokens, or captured Spotify responses containing credentials.
