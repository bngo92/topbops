use futures::{StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use time::Date;
use zeroflops::{Error, Id, Source, SourceType, UserId};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Setlist {
    pub event_date: String,
    pub artist: Artist,
    pub venue: Venue,
    pub sets: Sets,
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Artist {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Venue {
    pub name: String,
    pub city: City,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct City {
    pub name: String,
    pub state_code: String,
    pub country: Country,
}

#[derive(Debug, Deserialize, Serialize)]
struct Country {
    pub code: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Sets {
    pub set: Vec<Set>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Set {
    pub song: Vec<Song>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Song {
    pub name: String,
    pub tape: Option<bool>,
    pub cover: Option<Artist>,
}

pub async fn get_setlist(user_id: &UserId, id: Id) -> Result<(Source, Vec<crate::Item>), Error> {
    let token = crate::source::spotify::get_token().await?;
    let token = &token;

    let setlist: Setlist = Client::new()
        .get(format!("https://api.setlist.fm/rest/1.0/setlist/{}", id.id))
        .header("Accept", "application/json")
        .header(
            "x-api-key",
            std::env::var("SETLIST_KEY").expect("SETLIST_KEY is missing"),
        )
        .send()
        .await?
        .json()
        .await?;

    let songs: Vec<_> = setlist
        .sets
        .set
        .into_iter()
        .flat_map(|s| {
            s.song.into_iter().filter_map(|s| match s.tape {
                None => Some((
                    s.name.clone(),
                    Some(s.cover.as_ref().unwrap_or(&setlist.artist).name.clone()),
                )),
                Some(_) => None,
            })
        })
        .collect();
    let items: Vec<_> =
        futures::stream::iter(songs.into_iter().map(|(song, artist)| {
            crate::source::spotify::search_song(token, song, artist, user_id)
        }))
        .buffered(5)
        .try_collect()
        .await?;
    let read_format = time::format_description::parse("[day]-[month]-[year]").unwrap();
    let write_format = time::format_description::parse("[month repr:short] [day], [year]").unwrap();
    let date = Date::parse(&setlist.event_date, &read_format)
        .map_err(|e| Error::internal_error(format!("Unexpected date from setlist.fm: {e}")))?;
    let name = format!(
        "{} {} at {}, {}, {}, {}",
        date.format(&write_format).unwrap(),
        setlist.artist.name,
        setlist.venue.name,
        setlist.venue.city.name,
        setlist.venue.city.state_code,
        setlist.venue.city.country.code
    );
    Ok((
        Source {
            source_type: SourceType::Setlist(id),
            name,
        },
        items,
    ))
}
