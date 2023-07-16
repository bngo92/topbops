#[cfg(feature = "hyper")]
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
#[cfg(feature = "hyper")]
use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod spotify;

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
    ListItems(String),
}

impl List {
    pub fn get_unique_source(&self) -> Result<(Option<&str>, &Option<Id>), Error> {
        let ListMode::User(external_id) = &self.mode else {
            return Err(Error::client_error(
                "Push is not supported for this list type",
            ));
        };
        let mut iter = self.sources.iter().map(get_source_id);
        let mut source = if let Some(source) = iter.next() {
            if source.is_none() {
                return Err(Error::client_error("Push is not supported for the source"));
            }
            source
        } else {
            return Err(Error::client_error("List has no sources"));
        };
        for s in iter {
            if s != source {
                return Err(Error::client_error("List has multiple sources"));
            }
        }
        if source == Some("list") {
            let mut iter = self
                .items
                .iter()
                .map(|i| i.id.split_once(':').map(|t| t.0).unwrap_or(""));
            let prefix = if let Some(prefix) = iter.next() {
                prefix
            } else {
                return Err(Error::client_error("List has no items"));
            };
            for s in iter {
                if s != prefix {
                    return Err(Error::client_error("List has items from multiple sources"));
                }
            }
            source = Some("spotify")
        }
        Ok((source, external_id))
    }
}

fn get_source_id(source: &Source) -> Option<&str> {
    match source.source_type {
        SourceType::Spotify(_) => Some("spotify"),
        SourceType::Setlist(_) => Some("spotify"),
        SourceType::ListItems(_) => Some("list"),
        _ => None,
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Spotify {
    Playlist(Id),
    Album(Id),
    Track(Id),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Id {
    pub id: String,
    pub raw_id: String,
}

#[cfg(feature = "hyper")]
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
    pub spotify_user: Option<String>,
    pub spotify_url: Option<String>,
    pub google_email: Option<String>,
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
    #[cfg(feature = "hyper")]
    HyperError(hyper::Error),
    #[cfg(feature = "hyper")]
    RequestError(hyper::http::Error),
    JSONError(serde_json::Error),
    CosmosError(azure_core::error::Error),
    IOError(std::io::Error),
    Error(String),
}

#[cfg(feature = "hyper")]
impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Error {
        Error::InternalError(InternalError::HyperError(e))
    }
}

#[cfg(feature = "hyper")]
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

#[cfg(feature = "hyper")]
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
