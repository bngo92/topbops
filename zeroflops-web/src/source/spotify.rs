use crate::{cosmos::SessionClient, UserId};
use futures::{StreamExt, TryStreamExt};
use hyper::{Body, Client, Method, Request, Uri};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use zeroflops::{Error, Id, List, ListMode, Source, SourceType, Spotify};

#[derive(Debug, Deserialize, Serialize)]
struct Playlists {
    pub items: Vec<Playlist>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatePlaylist {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Playlist {
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
pub struct UpdatePlaylist {
    pub name: String,
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
    pub uri: String,
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
struct Search {
    pub tracks: SearchTracks,
}

#[derive(Debug, Deserialize, Serialize)]
struct SearchTracks {
    pub items: Vec<Track>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    pub external_urls: HashMap<String, String>,
}

pub async fn import(
    user_id: &UserId,
    source: &str,
    id: String,
) -> Result<(List, Vec<crate::Item>), Error> {
    match source {
        "playlist" => import_playlist(user_id, id).await,
        "album" => import_album(user_id, id).await,
        _ => todo!(),
    }
}

pub async fn get_playlist(
    user_id: &UserId,
    playlist_id: Id,
) -> Result<(Source, Vec<crate::Item>), Error> {
    let token = get_token().await?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!(
        "https://api.spotify.com/v1/playlists/{}?limit=50",
        playlist_id.id
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
    let playlist: Playlist = serde_json::from_slice(&got)?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!(
        "https://api.spotify.com/v1/playlists/{}/tracks",
        playlist_id.id
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
    Ok((
        Source {
            source_type: SourceType::Spotify(Spotify::Playlist(playlist_id)),
            name: playlist.name,
        },
        items,
    ))
}

pub async fn import_playlist(
    user_id: &UserId,
    playlist_id: String,
) -> Result<(List, Vec<crate::Item>), Error> {
    let id = Id {
        raw_id: format!(
            "https://open.spotify.com/embed/playlist/{}?utm_source=generator",
            playlist_id
        ),
        id: playlist_id,
    };
    let (source, items) = get_playlist(user_id, id.clone()).await?;
    let list = List {
        id: id.id,
        user_id: user_id.0.clone(),
        mode: ListMode::External,
        name: source.name.clone(),
        sources: vec![source],
        iframe: Some(id.raw_id),
        items: crate::convert_items(&items),
        favorite: false,
        query: String::from("SELECT name, user_score FROM tracks"),
    };
    Ok((list, items))
}

pub async fn get_album(user_id: &UserId, id: Id) -> Result<(Source, Vec<crate::Item>), Error> {
    let token = get_token().await?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!("https://api.spotify.com/v1/albums/{}", id.id)
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
    let uri: Uri = format!(
        "https://api.spotify.com/v1/albums/{}/tracks?limit=50",
        id.id
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
    let album_items: AlbumItems = serde_json::from_slice(&got)?;
    let items: Vec<_> = futures::stream::iter(
        album_items
            .items
            .into_iter()
            .map(|i| (i, token.access_token.clone()))
            .map(move |(item, access_token)| async move {
                let https = HttpsConnector::new();
                let client = Client::builder().build::<_, hyper::Body>(https);
                let uri: Uri = item.href.parse().unwrap();
                let resp = client
                    .request(
                        Request::builder()
                            .uri(uri)
                            .header("Authorization", format!("Bearer {}", access_token))
                            .body(Body::empty())?,
                    )
                    .await?;
                let got = hyper::body::to_bytes(resp.into_body()).await?;
                let track = serde_json::from_slice(&got)?;
                Ok::<_, Error>(new_spotify_item(track, user_id))
            }),
    )
    .buffered(1)
    .try_collect()
    .await?;
    Ok((
        Source {
            source_type: SourceType::Spotify(Spotify::Album(id)),
            name: album.name,
        },
        items,
    ))
}

pub async fn import_album(user_id: &UserId, id: String) -> Result<(List, Vec<crate::Item>), Error> {
    let id = Id {
        id: id.clone(),
        raw_id: format!(
            "https://open.spotify.com/embed/album/{}?utm_source=generator",
            id
        ),
    };
    let (source, items) = get_album(user_id, id.clone()).await?;
    let list = List {
        id: id.id,
        user_id: user_id.0.clone(),
        mode: ListMode::External,
        name: source.name.clone(),
        sources: vec![source],
        iframe: Some(id.raw_id),
        items: crate::convert_items(&items),
        favorite: false,
        query: String::from("SELECT name, user_score FROM tracks"),
    };
    Ok((list, items))
}

pub async fn create_playlist(access_token: &str, user_id: &UserId, name: &str) -> Result<Playlist, Error> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!("https://api.spotify.com/v1/users/{}/playlists", user_id.0)
        .parse()
        .unwrap();
    // TODO: error handling
    let playlist = CreatePlaylist {
        name: name.to_owned()
    };
    let body = serde_json::to_string(&playlist)?;
    let resp = client
        .request(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header("Authorization", format!("Bearer {}", access_token))
                .header("Content-Type", "application/json")
                .header("Content-Length", body.len().to_string())
                .body(Body::from(body))?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    Ok(serde_json::from_slice(&got)?)
}

pub async fn update_playlist(access_token: &str, playlist_id: &str, name: &str) -> Result<(), Error> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!("https://api.spotify.com/v1/playlists/{playlist_id}")
        .parse()
        .unwrap();
    // TODO: error handling
    let playlist = UpdatePlaylist {
        name: name.to_owned()
    };
    let body = serde_json::to_string(&playlist)?;
    client
        .request(
            Request::builder()
                .method(Method::PUT)
                .uri(uri)
                .header("Authorization", format!("Bearer {}", access_token))
                .header("Content-Type", "application/json")
                .header("Content-Length", body.len().to_string())
                .body(Body::from(body))?,
        )
        .await?;
    Ok(())
}

pub async fn update_list(
    access_token: &str,
    playlist_id: &str,
    ids: &[String],
) -> Result<(), Error> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let mut chunks = ids.chunks(100);
    let uri: Uri = format!(
        "https://api.spotify.com/v1/playlists/{}/tracks?uris={}",
        playlist_id,
        // Clear playlist if empty
        chunks.next().unwrap_or(&[]).join(",")
    )
    .parse()
    .unwrap();
    // TODO: error handling
    let resp = client
        .request(
            Request::builder()
                .method(Method::PUT)
                .uri(uri)
                .header("Authorization", format!("Bearer {}", access_token))
                .header("Content-Length", "0")
                .body(Body::empty())?,
        )
        .await?;
    if resp.status().is_client_error() || resp.status().is_server_error() {
        let got = hyper::body::to_bytes(resp.into_body()).await?;
        let error = format!(
            "Spotify update playlist items error: {}",
            String::from_utf8(got.to_vec())
                .unwrap_or_else(|_| "Spotify response should be ASCII".to_owned())
        );
        return Err(Error::internal_error(error));
    }
    for ids in chunks {
        let uri: Uri = format!(
            "https://api.spotify.com/v1/playlists/{}/tracks?uris={}",
            playlist_id,
            ids.join(",")
        )
        .parse()
        .unwrap();
        let resp = client
            .request(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header("Authorization", format!("Bearer {}", access_token))
                    .header("Content-Length", "0")
                    .body(Body::empty())?,
            )
            .await?;
        if resp.status().is_client_error() || resp.status().is_server_error() {
            let got = hyper::body::to_bytes(resp.into_body()).await?;
            let error = format!(
                "Spotify update playlist items error: {}",
                String::from_utf8(got.to_vec())
                    .unwrap_or_else(|_| "Spotify response should be ASCII".to_owned())
            );
            return Err(Error::internal_error(error));
        }
    }
    Ok(())
}

pub async fn get_token() -> Result<crate::Token, Error> {
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

pub async fn get_access_token<'a>(
    _client: &'_ SessionClient,
    user: &'a mut crate::user::User,
) -> Result<&'a str, Error> {
    let Some(credentials) = &mut user.spotify_credentials else { return Err(Error::client_error("User hasn't set up Spotify auth")) };
    let token = get_user_token(&credentials.refresh_token).await?;
    credentials.access_token = token.access_token;
    // TODO: reuse existing access token if it hasn't expired
    /*client
    .write_document(|db| {
        Ok(db
            .collection_client("users")
            .document_client(&user.id, &user.id)?
            .replace_document(user.clone()))
    })
    .await?;*/
    Ok(&credentials.access_token)
}

async fn get_user_token(refresh_token: &str) -> Result<crate::Token, Error> {
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
                    "grant_type=refresh_token&refresh_token={}",
                    refresh_token
                )))?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    serde_json::from_slice(&got).map_err(Error::from)
}

fn new_spotify_item(track: Track, user_id: &UserId) -> crate::Item {
    let metadata: Map<_, _> = [
        (String::from("album"), Value::String(track.album.name)),
        (
            String::from("artists"),
            Value::Array(
                track
                    .artists
                    .into_iter()
                    .map(|a| Value::String(a.name))
                    .collect::<Vec<_>>(),
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
    crate::Item {
        iframe: Some(format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            track.id
        )),
        id: track.uri,
        user_id: user_id.0.clone(),
        r#type: String::from("track"),
        name: track.name,
        rating: None,
        user_score: 1500,
        user_wins: 0,
        user_losses: 0,
        metadata,
        hidden: false,
    }
}

pub async fn search_song(
    token: &crate::Token,
    name: String,
    artist: Option<String>,
    user_id: &UserId,
) -> Result<crate::Item, Error> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = if let Some(artist) = artist {
        format!(
            "https://api.spotify.com/v1/search?q=track:{}%20artist:{}&type=track",
            urlencoding::encode(&name),
            urlencoding::encode(&artist)
        )
        .parse()
        .map_err(|e| Error::internal_error(format!("Invalid track or artist from setlist.fm: {e}")))
    } else {
        format!(
            "https://api.spotify.com/v1/search?q=track:{}&type=track",
            urlencoding::encode(&name),
        )
        .parse()
        .map_err(|e| Error::internal_error(format!("Invalid track from setlist.fm: {e}")))
    }?;
    let resp = client
        .request(
            Request::builder()
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token.access_token))
                .body(Body::empty())?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let result: Search = serde_json::from_slice(&got)?;
    Ok(new_spotify_item(
        result
            .tracks
            .items
            .into_iter()
            .next()
            .ok_or(Error::client_error("Couldn't find song for query"))?,
        user_id,
    ))
}
