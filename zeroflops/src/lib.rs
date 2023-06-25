#[cfg(feature = "azure")]
use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
pub struct Lists {
    pub lists: Vec<List>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct List {
    pub id: String,
    pub user_id: String,
    pub mode: ListMode,
    // This is not editable for external lists
    pub name: String,
    // External lists can only have one data source that must match id
    // Views have no data sources
    pub sources: Vec<Source>,

    pub iframe: Option<String>,
    pub items: Vec<ItemMetadata>,
    pub favorite: bool,
    // For external lists, query is only used to select fields (not filter)
    pub query: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemMetadata {
    pub id: String,
    pub name: String,
    pub iframe: Option<String>,
    pub score: i32,
    pub wins: i32,
    pub losses: i32,
    pub rank: Option<i32>,
}

impl ItemMetadata {
    pub fn new(id: String, name: String, iframe: Option<String>) -> ItemMetadata {
        ItemMetadata {
            id,
            name,
            iframe,
            score: 1500,
            wins: 0,
            losses: 0,
            rank: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ListMode {
    /// User defined lists that can pull from multiple sources
    /// User lists can also be pushed to an external source
    User(Option<Id>),
    /// Lists that are pulled from an external source
    External,
    /// Read only lists
    View,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Source {
    pub source_type: SourceType,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SourceType {
    Custom(Value),
    Spotify(Spotify),
    Setlist(Id),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Spotify {
    Playlist(Id),
    Album(Id),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Id {
    pub id: String,
    pub raw_id: String,
}

#[cfg(feature = "azure")]
impl CosmosEntity for List {
    type Entity = String;

    fn partition_key(&self) -> Self::Entity {
        self.user_id.clone()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemQuery {
    pub fields: Vec<String>,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Item {
    pub values: Vec<String>,
    pub metadata: Option<ItemMetadata>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct User {
    pub user_id: String,
    pub spotify_url: Option<String>,
    pub google_email: Option<String>,
}
