use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct RecentTracks {
    pub tracks: Vec<RecentTrack>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RecentTrack {
    pub id: String,
    pub name: String,
    pub url: String,
    pub added: bool,
    pub rating: Option<i32>,
    pub user_score: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Playlists {
    pub items: Vec<Playlist>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub external_urls: HashMap<String, String>,
}
