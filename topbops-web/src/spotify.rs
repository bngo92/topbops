use crate::{Error, UserId};
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
    pub href: String,
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
    pub duration_ms: i32,
    pub popularity: i32,
    pub track_number: i32,
}

#[derive(Debug, Deserialize, Serialize)]
struct AlbumTrack {
    pub href: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Album {
    pub href: String,
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

pub async fn import(user_id: &UserId, id: &str) -> Result<(List, Vec<super::Item>), Error> {
    match id.split_once(':') {
        Some(("playlist", id)) => import_playlist(user_id, id).await,
        Some(("album", id)) => import_album(user_id, id).await,
        _ => todo!(),
    }
}

pub async fn import_playlist(
    user_id: &UserId,
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
        user_id: user_id.0.clone(),
        name: playlist.name,
        iframe: Some(format!(
            "https://open.spotify.com/embed/playlist/{}?utm_source=generator",
            playlist_id
        )),
        items: items
            .iter()
            .map(|i| ItemMetadata::new(i.id.clone(), i.name.clone(), i.iframe.clone()))
            .collect(),
        mode: ListMode::External(playlist.href),
        query: String::from("SELECT name, user_score FROM tracks"),
    };
    Ok((list, items))
}

pub async fn import_album(user_id: &UserId, id: &str) -> Result<(List, Vec<super::Item>), Error> {
    let token = get_token().await?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!("https://api.spotify.com/v1/albums/{}", id)
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
    let album: Album = serde_json::from_slice(&got)?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!("https://api.spotify.com/v1/albums/{}/tracks", id)
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
    let album_items: AlbumItems = serde_json::from_slice(&got)?;
    let mut items = Vec::new();
    for item in album_items.items {
        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let uri: Uri = item.href.parse().unwrap();
        let resp = client
            .request(
                Request::builder()
                    .uri(uri)
                    .header("Authorization", format!("Bearer {}", token.access_token))
                    .body(Body::empty())?,
            )
            .await?;
        let got = hyper::body::to_bytes(resp.into_body()).await?;
        let track: Track = serde_json::from_slice(&got)?;
        items.push(new_spotify_item(track, user_id));
    }

    let list = List {
        id: id.to_owned(),
        user_id: user_id.0.clone(),
        name: album.name,
        iframe: Some(format!(
            "https://open.spotify.com/embed/album/{}?utm_source=generator",
            id
        )),
        items: items
            .iter()
            .map(|i| ItemMetadata::new(i.id.clone(), i.name.clone(), i.iframe.clone()))
            .collect(),
        mode: ListMode::External(album.href),
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

fn new_spotify_item(track: Track, user_id: &UserId) -> super::Item {
    let metadata: Map<_, _> = [
        (String::from("album"), Value::String(track.album.name)),
        (
            String::from("artists"),
            Value::String(
                track
                    .artists
                    .into_iter()
                    .map(|a| a.name)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ),
        (
            String::from("duration_ms"),
            Value::Number(track.duration_ms.into()),
        ),
        (
            String::from("popularity"),
            Value::Number(track.popularity.into()),
        ),
        (
            String::from("track_number"),
            Value::Number(track.track_number.into()),
        ),
    ]
    .into_iter()
    .collect();
    super::Item {
        iframe: Some(format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            track.id
        )),
        id: track.id,
        user_id: user_id.0.clone(),
        r#type: String::from("track"),
        name: track.name,
        rating: None,
        user_score: 1500,
        user_wins: 0,
        user_losses: 0,
        metadata,
    }
}
