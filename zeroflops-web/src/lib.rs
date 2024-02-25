#[cfg(feature = "azure")]
use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zeroflops::{Error, ItemMetadata};

pub mod query;
pub mod source;
pub mod user;

#[derive(Clone)]
pub struct UserId(pub String);
pub const ITEM_FIELDS: [&str; 9] = [
    "id",
    "type",
    "name",
    "iframe",
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

#[derive(Debug, Deserialize, Serialize)]
pub struct RawItem {
    pub id: String,
    pub user_id: String,
    pub r#type: String,
    pub name: String,
    pub iframe: Option<String>,
    pub rating: Option<i32>,
    pub user_score: i32,
    pub user_wins: i32,
    pub user_losses: i32,
    pub metadata: String,
    pub hidden: bool,
}

impl From<Item> for RawItem {
    fn from(i: Item) -> RawItem {
        RawItem {
            id: i.id,
            user_id: i.user_id,
            r#type: i.r#type,
            name: i.name,
            iframe: i.iframe,
            rating: i.rating,
            user_score: i.user_score,
            user_wins: i.user_wins,
            user_losses: i.user_losses,
            metadata: serde_json::to_string(&i.metadata).expect("metadata should serialize"),
            hidden: i.hidden,
        }
    }
}

impl TryFrom<RawItem> for Item {
    type Error = Error;
    fn try_from(i: RawItem) -> Result<Item, Error> {
        Ok(Item {
            id: i.id,
            user_id: i.user_id,
            r#type: i.r#type,
            name: i.name,
            iframe: i.iframe,
            rating: i.rating,
            user_score: i.user_score,
            user_wins: i.user_wins,
            user_losses: i.user_losses,
            metadata: serde_json::from_str(&i.metadata)?,
            hidden: i.hidden,
        })
    }
}

#[cfg(feature = "azure")]
impl CosmosEntity for RawItem {
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
