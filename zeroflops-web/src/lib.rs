use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zeroflops::ItemMetadata;

pub mod cosmos;
pub mod query;
pub mod source;
pub mod user;

#[derive(Clone)]
pub struct UserId(pub String);
pub const ITEM_FIELDS: [&str; 8] = [
    "id",
    "type",
    "name",
    "rating",
    "user_score",
    "user_wins",
    "user_losses",
    "hidden",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Item {
    pub id: String,
    pub user_id: String,
    pub r#type: String,
    pub name: String,
    pub iframe: Option<String>,
    pub rating: Option<i32>,
    pub user_score: i32,
    pub user_wins: i32,
    pub user_losses: i32,
    pub metadata: Map<String, Value>,
    pub hidden: bool,
}

impl CosmosEntity for Item {
    type Entity = String;

    fn partition_key(&self) -> Self::Entity {
        self.user_id.clone()
    }
}

pub fn convert_items(items: &[Item]) -> Vec<ItemMetadata> {
    items
        .iter()
        .map(|i| ItemMetadata::new(i.id.clone(), i.name.clone(), i.iframe.clone()))
        .collect()
}
