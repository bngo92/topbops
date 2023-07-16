use crate::{cosmos::SessionClient, UserId};
use futures::{StreamExt, TryStreamExt};
use serde_json::{Map, Value};
use zeroflops::{Error, ItemMetadata, List, Source, SourceType, Spotify};

pub mod setlist;
pub mod spotify;

pub async fn get_source_and_items(
    client: &SessionClient,
    user_id: &UserId,
    mut source: Source,
) -> Result<(Source, Vec<ItemMetadata>), Error> {
    let (source, items) = match source.source_type {
        SourceType::Custom(ref value) => {
            let items = get_custom_items(user_id, value)?;
            source.name = "Custom".to_owned();
            (source, items)
        }
        SourceType::Spotify(Spotify::Playlist(id)) => spotify::get_playlist(user_id, id).await?,
        SourceType::Spotify(Spotify::Album(id)) => spotify::get_album(user_id, id).await?,
        SourceType::Spotify(Spotify::Track(id)) => spotify::get_track(user_id, id).await?,
        SourceType::Setlist(id) => setlist::get_setlist(user_id, id).await?,
        SourceType::ListItems(ref id) => {
            let list = get_list(client, user_id, id).await?;
            source.name = list.name;
            return Ok((source, list.items));
        }
    };
    let list_items = crate::convert_items(&items);
    create_items(client, items, false).await?;
    Ok((source, list_items))
}

// TODO: support arbitrary input
fn get_custom_items(user_id: &UserId, value: &Value) -> Result<Vec<super::Item>, Error> {
    let Value::Array(a) = value else {
        return Err(Error::client_error("invalid custom type"));
    };
    a.iter()
        .map(|i| match i {
            Value::String(s) => Ok(new_custom_item(s, user_id, s.to_owned(), Map::new())),
            Value::Object(o) => {
                let mut o = o.clone();
                let Some(Value::String(id)) = o.remove("id") else {
                    return Err(Error::client_error("invalid id"));
                };
                let Some(Value::String(name)) = o.remove("name") else {
                    return Err(Error::client_error("invalid name"));
                };
                Ok(new_custom_item(&id, user_id, name, o))
            }
            _ => Err(Error::client_error("invalid custom type")),
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

pub async fn get_list(client: &SessionClient, user_id: &UserId, id: &str) -> Result<List, Error> {
    if let Some(list) = client
        .get_document(|db| {
            Ok(db
                .collection_client("lists")
                .document_client(id, &user_id.0)?
                .get_document())
        })
        .await?
    {
        Ok(list)
    } else {
        todo!()
    }
}

pub async fn create_items(
    client: &SessionClient,
    items: Vec<super::Item>,
    is_upsert: bool,
) -> Result<(), Error> {
    futures::stream::iter(items.into_iter().map(|item| async move {
        match client
            .write_document(|db| {
                Ok(db
                    .collection_client("items")
                    .create_document(item)
                    .is_upsert(is_upsert))
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                if let azure_core::StatusCode::Conflict = e.as_http_error().unwrap().status() {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }))
    .buffered(5)
    .try_collect()
    .await
    .map_err(Error::from)
}
