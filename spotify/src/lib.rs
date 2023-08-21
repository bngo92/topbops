use async_trait::async_trait;
use hyper::{Body, Client, Method, Request, Uri};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zeroflops::Error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    pub external_urls: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpotifyCredentials {
    pub user_id: String,
    pub url: String,
    pub access_token: String,
    pub refresh_token: String,
}

#[async_trait]
pub trait AuthClient {
    type Credentials;
    async fn get_credentials(&self, code: &str, origin: &str) -> Result<Self::Credentials, Error>;
}

pub struct SpotifyClient;

#[async_trait]
impl AuthClient for SpotifyClient {
    type Credentials = SpotifyCredentials;

    async fn get_credentials(&self, code: &str, origin: &str) -> Result<Self::Credentials, Error> {
        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let uri: Uri = "https://accounts.spotify.com/api/token".parse().unwrap();
        let resp = client
            .request(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header(
                        "Authorization",
                        &format!(
                            "Basic {}",
                            std::env::var("SPOTIFY_TOKEN").expect("SPOTIFY_TOKEN is missing")
                        ),
                    )
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "grant_type=authorization_code&code={}&redirect_uri={}",
                        code, origin
                    )))?,
            )
            .await?;
        let got = hyper::body::to_bytes(resp.into_body()).await?;
        let token: Token = serde_json::from_slice(&got)?;

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let uri: Uri = "https://api.spotify.com/v1/me".parse().unwrap();
        let resp = client
            .request(
                Request::builder()
                    .uri(uri)
                    .header("Authorization", format!("Bearer {}", token.access_token))
                    .body(Body::empty())?,
            )
            .await?;
        let got = hyper::body::to_bytes(resp.into_body()).await?;
        let spotify_user: User = serde_json::from_slice(&got)?;
        Ok(SpotifyCredentials {
            user_id: spotify_user.id.clone(),
            url: spotify_user.external_urls["spotify"].clone(),
            access_token: token.access_token,
            refresh_token: token.refresh_token.ok_or(Error::internal_error(
                "Spotify did not return refresh_token",
            ))?,
        })
    }
}
