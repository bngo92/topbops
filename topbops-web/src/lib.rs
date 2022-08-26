#![feature(let_else)]
use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub mod query;
pub mod spotify;

#[derive(Clone)]
pub struct UserId(pub String);
pub const ITEM_FIELDS: [&str; 7] = [
    "id",
    "name",
    "rating",
    "user_score",
    "user_wins",
    "user_losses",
    "hidden",
];

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
    pub hidden: bool,
}

impl CosmosEntity for Item {
    type Entity = String;

    fn partition_key(&self) -> Self::Entity {
        self.user_id.clone()
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub enum Error {
    HyperError(hyper::Error),
    RequestError(hyper::http::Error),
    JSONError(serde_json::Error),
    CosmosError(azure_core::error::Error),
    IOError(std::io::Error),
    SqlError(String),
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

impl From<azure_core::error::Error> for Error {
    fn from(e: azure_core::error::Error) -> Error {
        Error::CosmosError(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IOError(e)
    }
}

impl From<sqlparser::parser::ParserError> for Error {
    fn from(e: sqlparser::parser::ParserError) -> Error {
        Error::SqlError(match e {
            sqlparser::parser::ParserError::TokenizerError(e) => e,
            sqlparser::parser::ParserError::ParserError(e) => e,
        })
    }
}

impl From<&'static str> for Error {
    fn from(e: &'static str) -> Error {
        Error::SqlError(e.to_owned())
    }
}
