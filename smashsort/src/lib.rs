use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Lists {
    pub lists: Vec<List>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct List {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub items: Vec<ItemMetadata>,
    pub mode: ListMode,
    // For external lists, query is only used to select fields (not filter)
    pub query: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ItemMetadata {
    pub id: String,
    pub name: String,
    pub iframe: Option<String>,
    pub rating: Option<i32>,
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
            rating: None,
            score: 1500,
            wins: 0,
            losses: 0,
            rank: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ListMode {
    User,
    External,
}

impl<'a> CosmosEntity<'a> for List {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ItemQuery {
    pub fields: Vec<String>,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Item {
    pub values: Vec<String>,
    pub metadata: Option<ItemMetadata>,
}
