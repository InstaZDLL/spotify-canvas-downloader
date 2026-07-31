use std::time::Duration;

use prost::Message;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::proto::{EntityCanvazRequest, EntityCanvazResponse, entity_canvaz_request::Entity};

use super::{SpotifyError, SpotifySettings, auth::AuthClient, retry_after};

#[derive(Clone)]
pub struct SpotifyClient {
    http: reqwest::Client,
    auth: AuthClient,
    canvas_url: String,
}

impl SpotifyClient {
    pub fn new(settings: SpotifySettings) -> Result<Self, SpotifyError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        let auth = AuthClient::new(http.clone(), &settings)?;
        Ok(Self {
            http,
            auth,
            canvas_url: settings.canvas_url,
        })
    }

    pub async fn canvas_for_track(&self, track_id: &str) -> Result<Option<String>, SpotifyError> {
        let request = EntityCanvazRequest {
            entities: vec![Entity {
                entity_uri: format!("spotify:track:{track_id}"),
                etag: String::new(),
            }],
        }
        .encode_to_vec();

        let token = self.auth.access_token().await?;
        let mut response = self.post_canvas(&request, &token).await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            let token = self.auth.refresh_after_unauthorized(&token).await?;
            response = self.post_canvas(&request, &token).await?;
        }

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(SpotifyError::RateLimited(retry_after(&response)));
        }
        if !response.status().is_success() {
            return Err(SpotifyError::Upstream(response.status()));
        }

        let body = response.bytes().await?;
        decode_canvas_response(&body)
    }

    async fn post_canvas(
        &self,
        request: &[u8],
        token: &str,
    ) -> Result<reqwest::Response, SpotifyError> {
        Ok(self
            .http
            .post(&self.canvas_url)
            .header(CONTENT_TYPE, "application/x-protobuf")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(request.to_vec())
            .send()
            .await?)
    }
}

fn decode_canvas_response(body: &[u8]) -> Result<Option<String>, SpotifyError> {
    let response = EntityCanvazResponse::decode(body)?;
    Ok(response
        .canvases
        .into_iter()
        .map(|canvas| canvas.url)
        .find(|url| !url.is_empty()))
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use crate::proto::{EntityCanvazResponse, entity_canvaz_response::Canvaz};

    use super::decode_canvas_response;

    #[test]
    fn decodes_first_non_empty_canvas_url() {
        let body = EntityCanvazResponse {
            canvases: vec![
                Canvaz::default(),
                Canvaz {
                    url: "https://canvas.scdn.co/example.mp4".into(),
                    ..Default::default()
                },
            ],
            ttl_in_seconds: 60,
        }
        .encode_to_vec();

        assert_eq!(
            decode_canvas_response(&body).unwrap().as_deref(),
            Some("https://canvas.scdn.co/example.mp4")
        );
    }

    #[test]
    fn rejects_malformed_protobuf() {
        assert!(decode_canvas_response(&[0xff, 0xff]).is_err());
    }

    #[test]
    fn returns_none_without_a_usable_canvas_url() {
        let body = EntityCanvazResponse {
            canvases: vec![Canvaz::default()],
            ttl_in_seconds: 0,
        }
        .encode_to_vec();

        assert_eq!(decode_canvas_response(&body).unwrap(), None);
    }
}
