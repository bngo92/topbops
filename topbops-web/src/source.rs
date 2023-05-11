use crate::{Error, UserId};
use topbops::{Source, Spotify};

pub mod spotify;

pub async fn get_source_and_items(
    user_id: &UserId,
    source: &Spotify,
) -> Result<(Source, Vec<super::Item>), Error> {
    match source {
        Spotify::Playlist(id) => spotify::get_playlist(user_id, id).await,
        Spotify::Album(id) => spotify::get_album(user_id, id).await,
    }
}
