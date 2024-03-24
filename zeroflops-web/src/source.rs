use crate::RawItem;
use futures::{stream::FuturesUnordered, StreamExt, TryStreamExt};
use serde_json::{Map, Value};
use zeroflops::{
    storage::{
        CreateDocumentBuilder, DocumentWriter, GetDocumentBuilder, ReplaceDocumentBuilder,
        SessionClient,
    },
    Error, InternalError, ItemMetadata, List, RawList, Source, SourceType, Spotify, UserId,
};

pub mod setlist;
pub mod spotify;

pub async fn update_list_items(
    client: &impl SessionClient,
    user_id: &UserId,
    mut list: List,
) -> Result<(), Error> {
    let current_list = get_list(client, user_id, &list.id).await?;
    // Avoid updating sources if they haven't changed
    // TODO: we should also check the snapshot ID
    if current_list
        .sources
        .iter()
        .map(|s| &s.source_type)
        .ne(list.sources.iter().map(|s| &s.source_type))
    {
        list.items.clear();
        let sources = list.sources;
        list.sources = Vec::with_capacity(sources.len());
        for (source, items) in futures::stream::iter(
            sources
                .into_iter()
                .map(|source| get_source_and_items(client, user_id, source)),
        )
        .buffered(5)
        .try_collect::<Vec<_>>()
        .await?
        {
            list.sources.push(source);
            list.items.extend(items);
        }
    }
    if let Ok((Some("spotify"), Some(external_id))) = list.get_unique_source() {
        list.iframe = Some(format!(
            "https://open.spotify.com/embed/playlist/{}?utm_source=generator",
            external_id.id
        ));
    }
    update_list(client, user_id, list).await?;
    Ok(())
}

pub async fn update_list(
    client: &impl SessionClient,
    user_id: &UserId,
    list: List,
) -> Result<(), Error> {
    client
        .write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
            collection_name: "list",
            document_name: list.id.clone(),
            partition_key: user_id.clone(),
            document: RawList::from(list),
        }))
        .await
        .map_err(Error::from)
}

async fn get_source_and_items(
    client: &impl SessionClient,
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
        // TODO: inherit data sources
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

pub async fn get_list(
    client: &impl SessionClient,
    user_id: &UserId,
    id: &str,
) -> Result<List, Error> {
    if let Some(list) = client
        .get_document::<RawList>(GetDocumentBuilder::new(
            "list",
            id.to_owned(),
            user_id.clone(),
        ))
        .await?
    {
        list.try_into()
    } else {
        Err(Error::NotFound)
    }
}

pub async fn create_items(
    client: &impl SessionClient,
    items: Vec<super::Item>,
    is_upsert: bool,
) -> Result<(), Error> {
    items
        .into_iter()
        .map(|item| async move {
            match client
                .write_document(DocumentWriter::Create(CreateDocumentBuilder {
                    collection_name: "item",
                    document: RawItem::from(item),
                    is_upsert,
                }))
                .await
            {
                Ok(_)
                // If an error is returned because the item already exists, ignore the error
                | Err(Error::InternalError(InternalError::SqlError(
                    rusqlite::Error::SqliteFailure(
                        libsqlite3_sys::Error {
                            code: libsqlite3_sys::ErrorCode::ConstraintViolation,
                            ..
                        },
                        _,
                    ),
                ))) => Ok(()),
                Err(e) => Err(e),
            }
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect()
        .await
        .map_err(Error::from)
}

#[cfg(test)]
mod test {
    use crate::query::test::{Mock, TestSessionClient};
    use zeroflops::{
        storage::{DocumentWriter, ReplaceDocumentBuilder},
        List, ListMode, Source, SourceType, UserId,
    };

    #[tokio::test]
    async fn test_update_empty_list_items() {
        let client = TestSessionClient {
            get_mock: Mock::new(vec![
                r#"{"id":"","user_id":"","mode":"{\"User\":null}","name":"","sources":"[]","items":"[]","favorite":false,"query":"SELECT name, user_score FROM c"}"#,
            ]),
            query_mock: Mock::empty(),
            write_mock: Mock::new(vec![()]),
        };
        super::update_list_items(
            &client,
            &UserId(String::new()),
            List {
                id: String::new(),
                user_id: String::new(),
                mode: ListMode::User(None),
                name: String::from("New List"),
                sources: Vec::new(),
                iframe: None,
                items: Vec::new(),
                favorite: false,
                query: String::from("SELECT name, user_score FROM c"),
                public: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            *client.write_mock.call_args.lock().unwrap(),
            vec![DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "list",
                document_name: "".to_owned(),
                partition_key: UserId("".to_owned()),
                document: r#"{"id":"","user_id":"","mode":"{\"User\":null}","name":"New List","sources":"[]","iframe":null,"items":"[]","favorite":false,"query":"SELECT name, user_score FROM c","public":false}"#.to_owned(),
            })]
        );
    }

    #[tokio::test]
    async fn test_update_list_items_with_empty_source() {
        let client = TestSessionClient {
            get_mock: Mock::new(vec![
                r#"{"id":"","user_id":"","mode":"{\"User\":null}","name":"","sources":"[]","items":"[]","favorite":false,"query":"SELECT name, user_score FROM c"}"#,
                r#"{"id":"","user_id":"","mode":"{\"User\":null}","name":"source","sources":"[]","items":"[]","favorite":false,"query":"SELECT name, user_score FROM c"}"#,
            ]),
            query_mock: Mock::empty(),
            write_mock: Mock::new(vec![()]),
        };
        super::update_list_items(
            &client,
            &UserId(String::new()),
            List {
                id: String::new(),
                user_id: String::new(),
                mode: ListMode::User(None),
                name: String::from("New List"),
                sources: vec![Source {
                    source_type: SourceType::ListItems("".to_owned()),
                    name: String::new(),
                }],
                iframe: None,
                items: Vec::new(),
                favorite: false,
                query: String::from("SELECT name, user_score FROM c"),
                public: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            *client.write_mock.call_args.lock().unwrap(),
            vec![DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "list",
                document_name: "".to_owned(),
                partition_key: UserId("".to_owned()),
                document: r#"{"id":"","user_id":"","mode":"{\"User\":null}","name":"New List","sources":"[{\"source_type\":{\"ListItems\":\"\"},\"name\":\"source\"}]","iframe":null,"items":"[]","favorite":false,"query":"SELECT name, user_score FROM c","public":false}"#.to_owned(),
            })]
        );
    }

    #[tokio::test]
    async fn test_update_list_items_with_source() {
        let client = TestSessionClient {
            get_mock: Mock::new(vec![
                r#"{"id":"","user_id":"","mode":"{\"User\":null}","name":"","sources":"[]","items":"[]","favorite":false,"query":"SELECT name, user_score FROM c"}"#,
                r#"{"id":"","user_id":"","mode":"{\"User\":null}","name":"source","sources":"[]","items":"[{\"id\":\"\",\"name\":\"item\",\"score\":0,\"wins\":0,\"losses\":0}]","favorite":false,"query":"SELECT name, user_score FROM c"}"#,
            ]),
            query_mock: Mock::empty(),
            write_mock: Mock::new(vec![()]),
        };
        super::update_list_items(
            &client,
            &UserId(String::new()),
            List {
                id: String::new(),
                user_id: String::new(),
                mode: ListMode::User(None),
                name: String::from("New List"),
                sources: vec![Source {
                    source_type: SourceType::ListItems("".to_owned()),
                    name: String::new(),
                }],
                iframe: None,
                items: Vec::new(),
                favorite: false,
                query: String::from("SELECT name, user_score FROM c"),
                public: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            *client.write_mock.call_args.lock().unwrap(),
            vec![DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "list",
                document_name: "".to_owned(),
                partition_key: UserId("".to_owned()),
                document: r#"{"id":"","user_id":"","mode":"{\"User\":null}","name":"New List","sources":"[{\"source_type\":{\"ListItems\":\"\"},\"name\":\"source\"}]","iframe":null,"items":"[{\"id\":\"\",\"name\":\"item\",\"iframe\":null,\"score\":0,\"wins\":0,\"losses\":0,\"rank\":null}]","favorite":false,"query":"SELECT name, user_score FROM c","public":false}"#.to_owned(),
            })]
        );
    }
}
