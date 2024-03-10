use crate::query::IntoQuery;
use futures::{StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use zeroflops::{
    spotify::{Playlist, Playlists, RecentTrack},
    storage::{
        CosmosParam, CosmosQuery, QueryDocumentsBuilder, SessionClient, SqlSessionClient, View,
    },
    Error, Id, List, ListMode, Source, SourceType, Spotify, UserId,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatePlaylist {
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

// TODO: include album and artist metadata
#[derive(Debug, Deserialize, Serialize)]
struct Track {
    pub id: String,
    pub name: String,
    pub album: Album,
    pub artists: Vec<Artist>,
    pub duration_ms: i32,
    pub external_urls: HashMap<String, String>,
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

#[derive(Debug, Deserialize, Serialize)]
pub struct RecentTracks {
    items: Vec<PlayHistory>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlayHistory {
    track: Track,
}

pub async fn get_playlist(
    user_id: &UserId,
    playlist_id: Id,
) -> Result<(Source, Vec<crate::Item>), Error> {
    let token = get_token().await?;
    let client = Client::new();

    let playlist: Playlist = client
        .get(format!(
            "https://api.spotify.com/v1/playlists/{}?limit=50",
            playlist_id.id
        ))
        .header("Authorization", format!("Bearer {}", token.access_token))
        .send()
        .await?
        .json()
        .await?;

    let mut playlist_items: PlaylistItems = client
        .get(format!(
            "https://api.spotify.com/v1/playlists/{}/tracks",
            playlist_id.id
        ))
        .header("Authorization", format!("Bearer {}", token.access_token))
        .send()
        .await?
        .json()
        .await?;
    let mut items: Vec<_> = playlist_items
        .items
        .into_iter()
        .map(|i| new_spotify_item(i.track, user_id))
        .collect();
    while let Some(uri) = playlist_items.next {
        playlist_items = client
            .get(uri)
            .header("Authorization", format!("Bearer {}", token.access_token))
            .send()
            .await?
            .json()
            .await?;
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
        query: String::from("SELECT name, user_score FROM item"),
    };
    Ok((list, items))
}

pub async fn get_album(user_id: &UserId, id: Id) -> Result<(Source, Vec<crate::Item>), Error> {
    let token = get_token().await?;
    let client = Client::new();
    let client = &client;

    let album: Album = client
        .get(format!("https://api.spotify.com/v1/albums/{}", id.id))
        .header("Authorization", format!("Bearer {}", token.access_token))
        .send()
        .await?
        .json()
        .await?;

    let album_items: AlbumItems = client
        .get(format!(
            "https://api.spotify.com/v1/albums/{}/tracks?limit=50",
            id.id
        ))
        .header("Authorization", format!("Bearer {}", token.access_token))
        .send()
        .await?
        .json()
        .await?;
    let items: Vec<_> = futures::stream::iter(
        album_items
            .items
            .into_iter()
            .map(|i| (i, token.access_token.clone()))
            .map(move |(item, access_token)| async move {
                let track = client
                    .get(item.href)
                    .header("Authorization", format!("Bearer {}", access_token))
                    .send()
                    .await?
                    .json()
                    .await?;
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
        query: String::from("SELECT name, user_score FROM item"),
    };
    Ok((list, items))
}

pub async fn get_track(user_id: &UserId, id: Id) -> Result<(Source, Vec<crate::Item>), Error> {
    let token = get_token().await?;

    let track: Track = Client::new()
        .get(format!("https://api.spotify.com/v1/tracks/{}", id.id))
        .header("Authorization", format!("Bearer {}", token.access_token))
        .send()
        .await?
        .json()
        .await?;
    Ok((
        Source {
            source_type: SourceType::Spotify(Spotify::Track(id)),
            name: track.name.clone(),
        },
        vec![new_spotify_item(track, user_id)],
    ))
}

pub async fn create_playlist(
    access_token: &str,
    user_id: &UserId,
    name: &str,
) -> Result<Playlist, Error> {
    // TODO: error handling
    let playlist = CreatePlaylist {
        name: name.to_owned(),
    };
    Ok(Client::new()
        .post(format!(
            "https://api.spotify.com/v1/users/{}/playlists",
            user_id.0
        ))
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&playlist)
        .send()
        .await?
        .json()
        .await?)
}

pub async fn update_playlist(
    access_token: &str,
    playlist_id: &str,
    name: &str,
) -> Result<(), Error> {
    // TODO: error handling
    let playlist = UpdatePlaylist {
        name: name.to_owned(),
    };
    Client::new()
        .put(format!(
            "https://api.spotify.com/v1/playlists/{playlist_id}"
        ))
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&playlist)
        .send()
        .await?;
    Ok(())
}

pub async fn update_list(
    access_token: &str,
    playlist_id: &str,
    ids: &[String],
) -> Result<(), Error> {
    let client = Client::new();
    let mut chunks = ids.chunks(100);
    let uri = format!(
        "https://api.spotify.com/v1/playlists/{}/tracks?uris={}",
        playlist_id,
        // Clear playlist if empty
        chunks.next().unwrap_or(&[]).join(",")
    );
    // TODO: error handling
    let resp = client
        .put(uri)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Length", "0")
        .send()
        .await?;
    if resp.status().is_client_error() || resp.status().is_server_error() {
        let error = format!(
            "Spotify update playlist items error: {}",
            resp.text().await?
        );
        return Err(Error::internal_error(error));
    }
    for ids in chunks {
        let uri = format!(
            "https://api.spotify.com/v1/playlists/{}/tracks?uris={}",
            playlist_id,
            ids.join(",")
        );
        let resp = client
            .post(uri)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Length", "0")
            .send()
            .await?;
        if resp.status().is_client_error() || resp.status().is_server_error() {
            let error = format!(
                "Spotify update playlist items error: {}",
                resp.text().await?
            );
            return Err(Error::internal_error(error));
        }
    }
    Ok(())
}

pub async fn get_token() -> Result<crate::Token, Error> {
    Ok(Client::new()
        .post("https://accounts.spotify.com/api/token")
        .header(
            "Authorization",
            &format!(
                "Basic {}",
                std::env::var("SPOTIFY_TOKEN").expect("SPOTIFY_TOKEN is missing")
            ),
        )
        .form(&[("grant_type", "client_credentials")])
        .send()
        .await?
        .json()
        .await?)
}

pub async fn get_access_token<'a>(
    _client: &impl SessionClient,
    user: &'a mut crate::user::User,
) -> Result<&'a str, Error> {
    let Some(credentials) = &mut user.spotify_credentials else {
        return Err(Error::client_error("User hasn't set up Spotify auth"));
    };
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
    Ok(Client::new()
        .post("https://accounts.spotify.com/api/token")
        .header(
            "Authorization",
            &format!(
                "Basic {}",
                std::env::var("SPOTIFY_TOKEN").expect("SPOTIFY_TOKEN is missing")
            ),
        )
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?
        .json()
        .await?)
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
    let uri = if let Some(artist) = artist {
        format!(
            "https://api.spotify.com/v1/search?q=track:{}%20artist:{}&type=track",
            urlencoding::encode(&name),
            urlencoding::encode(&artist)
        )
    } else {
        format!(
            "https://api.spotify.com/v1/search?q=track:{}&type=track",
            urlencoding::encode(&name),
        )
    };
    let result: Search = Client::new()
        .get(&uri)
        .header("Authorization", format!("Bearer {}", token.access_token))
        .send()
        .await?
        .json()
        .await?;
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

pub async fn get_recent_tracks(
    cosmos_client: &SqlSessionClient,
    user_id: &UserId,
    access_token: &str,
) -> Result<zeroflops::spotify::RecentTracks, Error> {
    let recent_tracks: RecentTracks = Client::new()
        .get("https://api.spotify.com/v1/me/player/recently-played?limit=50")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?
        .json()
        .await?;
    let ids: Vec<_> = recent_tracks
        .items
        .iter()
        .map(|i| i.track.uri.to_owned())
        .collect();
    let query = format!(
        "SELECT id, rating, user_score FROM item WHERE id IN ({})",
        &"?,".repeat(ids.len())[..ids.len() * 2 - 1]
    );
    let items = cosmos_client
        .query_documents(QueryDocumentsBuilder::new(
            "item",
            View::User(user_id.clone()),
            CosmosQuery::with_params(
                query.into_query()?,
                ids.into_iter()
                    .map(|i| CosmosParam::new(String::from("@ids"), i))
                    .collect::<Vec<_>>(),
            ),
        ))
        .await
        .map_err(Error::from)?;
    let map: HashMap<_, _> = items
        .into_iter()
        .map(|r: Map<String, Value>| (r["id"].as_str().expect("string id").to_owned(), r))
        .collect();
    Ok(zeroflops::spotify::RecentTracks {
        tracks: recent_tracks
            .items
            .into_iter()
            .map(|mut i| {
                let id = i.track.uri;
                if let Some(m) = map.get(&id) {
                    RecentTrack {
                        id,
                        name: i.track.name,
                        url: i.track.external_urls.remove("spotify").unwrap(),
                        added: true,
                        rating: m.get("rating").and_then(|v| v.as_i64()).map(|i| i as i32),
                        user_score: m
                            .get("user_score")
                            .and_then(|v| v.as_i64())
                            .map(|i| i as i32),
                    }
                } else {
                    RecentTrack {
                        id,
                        name: i.track.name,
                        url: i.track.external_urls.remove("spotify").unwrap(),
                        added: false,
                        rating: None,
                        user_score: None,
                    }
                }
            })
            .collect(),
    })
}

pub async fn get_playlists(access_token: &str) -> Result<Playlists, Error> {
    Ok(Client::new()
        .get("https://api.spotify.com/v1/me/playlists")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?
        .json()
        .await?)
}
