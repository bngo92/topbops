use super::Error;
use hyper::{Body, Client, Method, Request, Uri};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use topbops::{ItemMetadata, List, ListMode};

#[derive(Debug, Deserialize, Serialize)]
struct Playlists {
    pub items: Vec<Playlist>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Playlist {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaylistItems {
    pub items: Vec<Item>,
    pub next: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Item {
    pub track: Track,
}

#[derive(Debug, Deserialize, Serialize)]
struct AlbumItems {
    pub items: Vec<AlbumTrack>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Track {
    pub id: String,
    pub name: String,
    pub album: Album,
    pub artists: Vec<Artist>,
    pub preview_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AlbumTrack {
    pub id: String,
    pub name: String,
    pub artists: Vec<Artist>,
    pub preview_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Album {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Artist {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct User {
    pub id: String,
}

pub async fn import_playlist(
    user_id: &String,
    playlist_id: &str,
) -> Result<(List, Vec<super::Item>), Error> {
    let token = get_token().await?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!("https://api.spotify.com/v1/playlists/{}", playlist_id)
        .parse()
        .unwrap();
    let resp = client
        .request(
            Request::builder()
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token.access_token))
                .body(Body::empty())?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let playlist: Playlist = serde_json::from_slice(&got)?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!(
        "https://api.spotify.com/v1/playlists/{}/tracks",
        playlist_id
    )
    .parse()
    .unwrap();
    let resp = client
        .request(
            Request::builder()
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token.access_token))
                .body(Body::empty())?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let mut playlist_items: PlaylistItems = serde_json::from_slice(&got)?;
    let mut items: Vec<_> = playlist_items
        .items
        .into_iter()
        .map(|i| new_spotify_item(i.track, user_id))
        .collect();
    while let Some(uri) = playlist_items.next {
        let uri: Uri = uri.parse().unwrap();
        let resp = client
            .request(
                Request::builder()
                    .uri(uri)
                    .header("Authorization", format!("Bearer {}", token.access_token))
                    .body(Body::empty())?,
            )
            .await?;
        let got = hyper::body::to_bytes(resp.into_body()).await?;
        playlist_items = serde_json::from_slice(&got)?;
        items.extend(
            playlist_items
                .items
                .into_iter()
                .map(|i| new_spotify_item(i.track, user_id)),
        );
    }
    let list = List {
        id: playlist_id.to_owned(),
        user_id: user_id.clone(),
        name: playlist.name,
        items: items
            .iter()
            .map(|i| ItemMetadata::new(i.id.clone(), i.name.clone(), i.iframe.clone()))
            .collect(),
        mode: ListMode::External,
        query: String::from("SELECT name, user_score FROM tracks"),
    };
    Ok((list, items))
}

async fn get_token() -> Result<super::Token, Error> {
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
                .body(Body::from("grant_type=client_credentials"))?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    serde_json::from_slice(&got).map_err(Error::from)
}

fn new_spotify_item(track: Track, user_id: &String) -> super::Item {
    let mut metadata = Map::new();
    metadata.insert(String::from("album"), Value::String(track.album.name));
    metadata.insert(
        String::from("artists"),
        Value::String(
            track
                .artists
                .into_iter()
                .map(|a| a.name)
                .collect::<Vec<_>>()
                .join(", "),
        ),
    );
    super::Item {
        iframe: Some(format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            track.id
        )),
        id: track.id,
        user_id: user_id.clone(),
        r#type: String::from("track"),
        name: track.name,
        rating: None,
        user_score: 1500,
        user_wins: 0,
        user_losses: 0,
        metadata,
    }
}
