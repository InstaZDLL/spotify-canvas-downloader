use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::header::{COOKIE, HeaderValue, USER_AGENT};
use serde::Deserialize;
use tokio::sync::Mutex;
use totp_rs::{Algorithm, TOTP};
use tracing::warn;

use super::{SpotifyError, SpotifySettings, retry_after};

const USER_AGENT_VALUE: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";
const CATALOG_TTL: Duration = Duration::from_secs(60 * 60);
const TOKEN_REFRESH_SKEW_MS: u64 = 30_000;
const FALLBACK_VERSION: u32 = 61;
const FALLBACK_BYTES: &[u8] = &[
    44, 55, 47, 42, 70, 40, 34, 114, 76, 74, 50, 111, 120, 97, 75, 76, 94, 102, 43, 69, 49, 120,
    118, 80, 64, 78,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TotpSecret {
    pub version: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub(super) struct AuthClient {
    http: reqwest::Client,
    token_url: String,
    cookie: HeaderValue,
    secrets: std::sync::Arc<SecretProvider>,
    token: std::sync::Arc<Mutex<Option<CachedToken>>>,
}

#[derive(Clone, Debug)]
struct CachedToken {
    value: String,
    expires_at_ms: u64,
}

struct SecretProvider {
    http: reqwest::Client,
    catalog_url: String,
    override_secret: Option<TotpSecret>,
    cache: Mutex<Option<CachedSecret>>,
}

#[derive(Clone)]
struct CachedSecret {
    secret: TotpSecret,
    fetched_at: tokio::time::Instant,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenResponse {
    access_token: String,
    access_token_expiration_timestamp_ms: u64,
    #[serde(default)]
    is_anonymous: bool,
}

impl AuthClient {
    pub(super) fn new(
        http: reqwest::Client,
        settings: &SpotifySettings,
    ) -> Result<Self, SpotifyError> {
        let mut cookie = format!("sp_dc={}", settings.sp_dc);
        if let Some(sp_key) = &settings.sp_key {
            cookie.push_str("; sp_key=");
            cookie.push_str(sp_key);
        }
        let cookie = HeaderValue::from_str(&cookie).map_err(|_| {
            SpotifyError::Authentication("cookie contains invalid header characters".into())
        })?;
        let secrets = SecretProvider {
            http: http.clone(),
            catalog_url: settings.catalog_url.clone(),
            override_secret: settings.totp_override.clone(),
            cache: Mutex::new(None),
        };

        Ok(Self {
            http,
            token_url: settings.token_url.clone(),
            cookie,
            secrets: std::sync::Arc::new(secrets),
            token: std::sync::Arc::new(Mutex::new(None)),
        })
    }

    pub(super) async fn access_token(&self) -> Result<String, SpotifyError> {
        self.access_token_with_catalog_refresh(false).await
    }

    pub(super) async fn refresh_after_unauthorized(
        &self,
        failed_token: &str,
    ) -> Result<String, SpotifyError> {
        let mut guard = self.token.lock().await;
        if let Some(token) = guard.as_ref() {
            if token.value != failed_token && token_is_valid(token) {
                return Ok(token.value.clone());
            }
        }

        let token = self.request_token(true).await?;
        let value = token.value.clone();
        *guard = Some(token);
        Ok(value)
    }

    async fn access_token_with_catalog_refresh(
        &self,
        force_catalog_refresh: bool,
    ) -> Result<String, SpotifyError> {
        let mut guard = self.token.lock().await;
        if !force_catalog_refresh {
            if let Some(token) = guard.as_ref() {
                if token_is_valid(token) {
                    return Ok(token.value.clone());
                }
            }
        }

        let token = match self.request_token(force_catalog_refresh).await {
            Err(SpotifyError::Authentication(_)) if !force_catalog_refresh => {
                self.request_token(true).await?
            }
            result => result?,
        };
        let value = token.value.clone();
        *guard = Some(token);
        Ok(value)
    }

    async fn request_token(
        &self,
        force_catalog_refresh: bool,
    ) -> Result<CachedToken, SpotifyError> {
        let secret = self.secrets.latest(force_catalog_refresh).await;
        let now = unix_seconds()?;
        let totp_secret = spotify_totp_key(&secret.bytes);
        let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, totp_secret)
            .map_err(|error| SpotifyError::Authentication(error.to_string()))?
            .generate(now);

        let response = self
            .http
            .get(&self.token_url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(COOKIE, self.cookie.clone())
            .query(&[
                ("reason", "init".to_owned()),
                ("productType", "web-player".to_owned()),
                ("totp", totp.clone()),
                ("totpServer", totp),
                ("totpVer", secret.version.to_string()),
            ])
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SpotifyError::RateLimited(retry_after(&response)));
        }
        if !response.status().is_success() {
            return Err(SpotifyError::Authentication(format!(
                "token endpoint returned {}",
                response.status()
            )));
        }

        let body: TokenResponse = response.json().await.map_err(|error| {
            SpotifyError::Authentication(format!("invalid token response: {error}"))
        })?;
        if body.is_anonymous || body.access_token.is_empty() {
            return Err(SpotifyError::Authentication(
                "Spotify returned an anonymous or empty token; SP_DC is invalid".into(),
            ));
        }

        Ok(CachedToken {
            value: body.access_token,
            expires_at_ms: body.access_token_expiration_timestamp_ms,
        })
    }
}

impl SecretProvider {
    async fn latest(&self, force_refresh: bool) -> TotpSecret {
        if let Some(secret) = &self.override_secret {
            return secret.clone();
        }

        let mut cache = self.cache.lock().await;
        if !force_refresh {
            if let Some(cached) = cache.as_ref() {
                if cached.fetched_at.elapsed() < CATALOG_TTL {
                    return cached.secret.clone();
                }
            }
        }

        match self.fetch_latest().await {
            Ok(secret) => {
                *cache = Some(CachedSecret {
                    secret: secret.clone(),
                    fetched_at: tokio::time::Instant::now(),
                });
                secret
            }
            Err(error) => {
                warn!(%error, "failed to refresh the Spotify TOTP catalog; using fallback");
                cache
                    .as_ref()
                    .map(|cached| cached.secret.clone())
                    .unwrap_or_else(fallback_secret)
            }
        }
    }

    async fn fetch_latest(&self) -> Result<TotpSecret, SpotifyError> {
        let response = self.http.get(&self.catalog_url).send().await?;
        if !response.status().is_success() {
            return Err(SpotifyError::Upstream(response.status()));
        }
        let catalog: HashMap<String, serde_json::Value> = response.json().await?;
        select_latest(normalize_catalog(catalog))
            .ok_or_else(|| SpotifyError::Authentication("TOTP catalog is empty".into()))
    }
}

fn normalize_catalog(catalog: HashMap<String, serde_json::Value>) -> HashMap<String, Vec<u8>> {
    catalog
        .into_iter()
        .filter_map(|(version, value)| {
            let values = value.as_array()?;
            let bytes = values
                .iter()
                .map(|value| value.as_u64().filter(|value| *value <= u8::MAX as u64))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .map(|value| value as u8)
                .collect::<Vec<_>>();
            (!bytes.is_empty()).then_some((version, bytes))
        })
        .collect()
}

fn select_latest(catalog: HashMap<String, Vec<u8>>) -> Option<TotpSecret> {
    catalog
        .into_iter()
        .filter_map(|(version, bytes)| version.parse::<u32>().ok().map(|version| (version, bytes)))
        .filter(|(_, bytes)| !bytes.is_empty())
        .max_by_key(|(version, _)| *version)
        .map(|(version, bytes)| TotpSecret { version, bytes })
}

fn fallback_secret() -> TotpSecret {
    TotpSecret {
        version: FALLBACK_VERSION,
        bytes: FALLBACK_BYTES.to_vec(),
    }
}

fn spotify_totp_key(secret_bytes: &[u8]) -> Vec<u8> {
    secret_bytes
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ ((index % 33) as u8 + 9))
        .flat_map(|byte| byte.to_string().into_bytes())
        .collect()
}

fn unix_seconds() -> Result<u64, SpotifyError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| SpotifyError::Authentication(error.to_string()))
}

fn unix_milliseconds() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn token_is_valid(token: &CachedToken) -> bool {
    unix_milliseconds().is_some_and(|now| now + TOKEN_REFRESH_SKEW_MS < token.expires_at_ms)
}

#[cfg(test)]
mod tests {
    use super::{FALLBACK_BYTES, fallback_secret, spotify_totp_key};
    use totp_rs::{Algorithm, TOTP};

    #[test]
    fn fallback_is_the_current_catalog_version() {
        let secret = fallback_secret();
        assert_eq!(secret.version, 61);
        assert_eq!(secret.bytes, FALLBACK_BYTES);
    }

    #[test]
    fn generates_stable_totp_for_fixed_time() {
        let key = spotify_totp_key(FALLBACK_BYTES);
        let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, key).unwrap();
        assert_eq!(totp.generate(1_785_531_000), "352797");
    }

    #[test]
    fn selects_highest_catalog_version() {
        use std::collections::HashMap;

        let catalog = HashMap::from([
            ("invalid".to_owned(), vec![9]),
            ("2".to_owned(), vec![1, 2]),
            ("10".to_owned(), vec![3, 4]),
            ("11".to_owned(), vec![]),
        ]);
        let secret = super::select_latest(catalog).unwrap();
        assert_eq!(secret.version, 10);
        assert_eq!(secret.bytes, [3, 4]);
    }

    #[test]
    fn discards_only_malformed_catalog_entries() {
        use std::collections::HashMap;

        let catalog = HashMap::from([
            ("61".to_owned(), serde_json::json!([44, 55, 47])),
            ("62".to_owned(), serde_json::json!([1, 999])),
            ("63".to_owned(), serde_json::json!("invalid")),
        ]);

        let secret = super::select_latest(super::normalize_catalog(catalog)).unwrap();
        assert_eq!(secret.version, 61);
        assert_eq!(secret.bytes, [44, 55, 47]);
    }
}
