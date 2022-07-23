use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub mod spotify;

#[derive(Debug, Deserialize, Serialize)]
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
}

impl<'a> CosmosEntity<'a> for Item {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub enum Error {
    HyperError(hyper::Error),
    RequestError(hyper::http::Error),
    JSONError(serde_json::Error),
    CosmosError(azure_data_cosmos::Error),
    IOError(std::io::Error),
}

impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Error {
        Error::HyperError(e)
    }
}

impl From<hyper::http::Error> for Error {
    fn from(e: hyper::http::Error) -> Error {
        Error::RequestError(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::JSONError(e)
    }
}

impl From<azure_data_cosmos::Error> for Error {
    fn from(e: azure_data_cosmos::Error) -> Error {
        Error::CosmosError(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IOError(e)
    }
}
