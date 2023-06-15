use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
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

pub fn convert_items(items: &[Item]) -> Vec<ItemMetadata> {
    items
        .iter()
        .map(|i| ItemMetadata::new(i.id.clone(), i.name.clone(), i.iframe.clone()))
        .collect()
}

#[derive(Debug)]
pub enum Error {
    ClientError(String),
    InternalError(InternalError),
}

impl Error {
    pub fn client_error(e: impl Into<String>) -> Self {
        Self::ClientError(e.into())
    }

    pub fn internal_error(e: impl Into<String>) -> Self {
        Self::InternalError(InternalError::Error(e.into()))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {}

#[derive(Debug)]
pub enum InternalError {
    HyperError(hyper::Error),
    RequestError(hyper::http::Error),
    JSONError(serde_json::Error),
    CosmosError(azure_core::error::Error),
    IOError(std::io::Error),
    Error(String),
}

impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Error {
        Error::InternalError(InternalError::HyperError(e))
    }
}

impl From<hyper::http::Error> for Error {
    fn from(e: hyper::http::Error) -> Error {
        Error::InternalError(InternalError::RequestError(e))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::InternalError(InternalError::JSONError(e))
    }
}

impl From<azure_core::error::Error> for Error {
    fn from(e: azure_core::error::Error) -> Error {
        Error::InternalError(InternalError::CosmosError(e))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::InternalError(InternalError::IOError(e))
    }
}

impl From<sqlparser::parser::ParserError> for Error {
    fn from(e: sqlparser::parser::ParserError) -> Error {
        Error::ClientError(match e {
            sqlparser::parser::ParserError::TokenizerError(e) => e,
            sqlparser::parser::ParserError::ParserError(e) => e,
            sqlparser::parser::ParserError::RecursionLimitExceeded => {
                "Query is too long".to_owned()
            }
        })
    }
}

impl From<Error> for Response {
    fn from(e: Error) -> Response {
        match e {
            Error::ClientError(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            Error::InternalError(e) => {
                eprintln!("server error: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}
