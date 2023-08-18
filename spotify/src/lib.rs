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

#[async_trait]
pub trait Spotify {
    async fn get_token(&self, code: &str, origin: &str) -> Result<Token, Error>;
    async fn get_current_user(&self, token: &Token) -> Result<User, Error>;
}

pub struct SpotifyClient;

#[async_trait]
impl Spotify for SpotifyClient {
    async fn get_token(&self, code: &str, origin: &str) -> Result<Token, Error> {
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
        Ok(serde_json::from_slice(&got)?)
    }

    async fn get_current_user(&self, token: &Token) -> Result<User, Error> {
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
        Ok(serde_json::from_slice(&got)?)
    }
}
