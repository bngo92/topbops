use async_trait::async_trait;
use reqwest::Client;
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
        let client = Client::new();
        let token: Token = client
            .post("https://accounts.spotify.com/api/token")
            .header(
                "Authorization",
                &format!(
                    "Basic {}",
                    std::env::var("SPOTIFY_TOKEN").expect("SPOTIFY_TOKEN is missing")
                ),
            )
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", origin),
            ])
            .send()
            .await?
            .json()
            .await?;

        let spotify_user: User = client
            .get("https://api.spotify.com/v1/me")
            .header("Authorization", format!("Bearer {}", token.access_token))
            .send()
            .await?
            .json()
            .await?;
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
