use crate::{Error, UserId};
use serde_json::{Map, Value};
use topbops::{Source, SourceType, Spotify};

pub mod spotify;

pub async fn get_source_and_items(
    user_id: &UserId,
    source: &Source,
) -> Result<(Source, Vec<super::Item>), Error> {
    match &source.source_type {
        SourceType::Custom(value) => Ok((source.clone(), get_custom_items(user_id, value)?)),
        SourceType::Spotify(Spotify::Playlist(id)) => spotify::get_playlist(user_id, id).await,
        SourceType::Spotify(Spotify::Album(id)) => spotify::get_album(user_id, id).await,
    }
}

fn get_custom_items(user_id: &UserId, value: &Value) -> Result<Vec<super::Item>, Error> {
    let Value::Array(a) = value else { return Err("invalid custom type".into()); };
    a.iter()
        .map(|i| match i {
            Value::String(s) => Ok(new_custom_item(s, user_id, s.to_owned(), Map::new())),
            Value::Object(o) => {
                let mut o = o.clone();
                let Some(Value::String(id)) = o.remove("id") else { return Err("invalid id".into()) };
                let Some(Value::String(name)) = o.remove("name") else { return Err("invalid name".into()) };
                Ok(new_custom_item(&id, user_id, name, o))
            }
            _ => Err("invalid custom type".into()),
        })
        .collect()
}

fn new_custom_item(
    id: &str,
    user_id: &UserId,
    name: String,
    metadata: Map<String, Value>,
) -> super::Item {
    super::Item {
        id: format!("custom:{}", &id),
        user_id: user_id.0.clone(),
        r#type: String::from("custom"),
        name,
        iframe: None,
        rating: None,
        user_score: 1500,
        user_wins: 0,
        user_losses: 0,
        metadata,
        hidden: false,
    }
}
