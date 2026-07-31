use std::{env, net::SocketAddr};

use base64::{Engine, engine::general_purpose::STANDARD};
use thiserror::Error;

use crate::spotify::{SpotifySettings, TotpSecret};

const TOKEN_URL: &str = "https://open.spotify.com/api/token";
const CANVAS_URL: &str = "https://gew1-spclient.spotify.com/canvaz-cache/v0/canvases";
const CATALOG_URL: &str =
    "https://code.thetadev.de/ThetaDev/spotify-secrets/raw/branch/main/secrets/secretDict.json";

#[derive(Debug)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub host_origin: Option<String>,
    pub spotify: SpotifySettings,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("SP_DC must be set and non-empty")]
    MissingSpDc,
    #[error("invalid BIND_ADDR: {0}")]
    InvalidBindAddress(#[from] std::net::AddrParseError),
    #[error("SPOTIFY_TOTP_VERSION and SPOTIFY_TOTP_SECRET_BASE64 must be set together")]
    IncompleteTotpOverride,
    #[error("invalid SPOTIFY_TOTP_VERSION")]
    InvalidTotpVersion,
    #[error("invalid SPOTIFY_TOTP_SECRET_BASE64")]
    InvalidTotpSecret,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let sp_dc = non_empty_env("SP_DC").ok_or(ConfigError::MissingSpDc)?;
        let sp_key = non_empty_env("SP_KEY");
        let bind_addr = env::var("BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8000".to_owned())
            .parse()?;
        let host_origin = non_empty_env("HOST_ORIGIN");
        let catalog_url =
            non_empty_env("SPOTIFY_TOTP_CATALOG_URL").unwrap_or_else(|| CATALOG_URL.to_owned());
        let totp_override = totp_override_from_env()?;

        Ok(Self {
            bind_addr,
            host_origin,
            spotify: SpotifySettings {
                sp_dc,
                sp_key,
                token_url: TOKEN_URL.to_owned(),
                canvas_url: CANVAS_URL.to_owned(),
                catalog_url,
                totp_override,
            },
        })
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn totp_override_from_env() -> Result<Option<TotpSecret>, ConfigError> {
    let version = non_empty_env("SPOTIFY_TOTP_VERSION");
    let secret = non_empty_env("SPOTIFY_TOTP_SECRET_BASE64");
    parse_totp_override(version, secret)
}

fn parse_totp_override(
    version: Option<String>,
    secret: Option<String>,
) -> Result<Option<TotpSecret>, ConfigError> {
    match (version, secret) {
        (None, None) => Ok(None),
        (Some(version), Some(secret)) => {
            let version = version
                .parse()
                .map_err(|_| ConfigError::InvalidTotpVersion)?;
            let bytes = STANDARD
                .decode(secret)
                .map_err(|_| ConfigError::InvalidTotpSecret)?;
            if bytes.is_empty() {
                return Err(ConfigError::InvalidTotpSecret);
            }
            Ok(Some(TotpSecret { version, bytes }))
        }
        _ => Err(ConfigError::IncompleteTotpOverride),
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::STANDARD};

    use super::{ConfigError, parse_totp_override};

    #[test]
    fn parses_absent_and_valid_totp_overrides() {
        assert!(parse_totp_override(None, None).unwrap().is_none());
        let secret = parse_totp_override(Some("61".into()), Some(STANDARD.encode([44_u8, 55, 47])))
            .unwrap()
            .unwrap();
        assert_eq!(secret.version, 61);
        assert_eq!(secret.bytes, [44, 55, 47]);
    }

    #[test]
    fn rejects_invalid_totp_overrides() {
        assert!(matches!(
            parse_totp_override(Some("61".into()), None),
            Err(ConfigError::IncompleteTotpOverride)
        ));
        assert!(matches!(
            parse_totp_override(Some("x".into()), Some("QQ==".into())),
            Err(ConfigError::InvalidTotpVersion)
        ));
        assert!(matches!(
            parse_totp_override(Some("61".into()), Some("!".into())),
            Err(ConfigError::InvalidTotpSecret)
        ));
        assert!(matches!(
            parse_totp_override(Some("61".into()), Some(String::new())),
            Err(ConfigError::InvalidTotpSecret)
        ));
    }
}
