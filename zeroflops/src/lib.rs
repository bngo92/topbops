#[cfg(feature = "full")]
use arrow_schema::ArrowError;
#[cfg(feature = "full")]
use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
};
#[cfg(feature = "azure")]
use azure_data_cosmos::prelude::CosmosEntity;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod spotify;
#[cfg(feature = "full")]
pub mod storage;

#[derive(Clone, Debug, PartialEq)]
pub struct UserId(pub String);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    pub public: bool,
}

impl List {
    pub fn new(
        id: String,
        user_id: &UserId,
        mode: ListMode,
        name: String,
        sources: Vec<Source>,
        iframe: Option<String>,
        items: Vec<ItemMetadata>,
    ) -> List {
        List {
            id,
            user_id: user_id.0.clone(),
            mode,
            name,
            sources,
            iframe,
            items,
            favorite: false,
            query: String::from("SELECT name, user_score FROM item"),
            public: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RawList {
    pub id: String,
    pub user_id: String,
    pub mode: String,
    // This is not editable for external lists
    pub name: String,
    // External lists can only have one data source that must match id
    // Views have no data sources
    pub sources: String,
    pub iframe: Option<String>,
    pub items: String,
    pub favorite: bool,
    // For external lists, query is only used to select fields (not filter)
    pub query: String,
    pub public: Option<bool>,
}

impl From<List> for RawList {
    fn from(l: List) -> RawList {
        RawList {
            id: l.id,
            user_id: l.user_id,
            mode: serde_json::to_string(&l.mode).expect("mode should serialize"),
            name: l.name,
            sources: serde_json::to_string(&l.sources).expect("sources should serialize"),
            iframe: l.iframe,
            items: serde_json::to_string(&l.items).expect("items should serialize"),
            favorite: l.favorite,
            query: l.query,
            public: Some(l.public),
        }
    }
}

impl TryFrom<RawList> for List {
    type Error = Error;
    fn try_from(l: RawList) -> Result<List, Error> {
        Ok(List {
            id: l.id,
            user_id: l.user_id,
            mode: serde_json::from_str(&l.mode)?,
            name: l.name,
            sources: serde_json::from_str(&l.sources)?,
            iframe: l.iframe,
            items: serde_json::from_str(&l.items)?,
            favorite: l.favorite,
            query: l.query,
            public: l.public.unwrap_or_default(),
        })
    }
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
    View(Option<Id>),
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
        // TODO: validate view lists
        if let ListMode::View(external_id) = &self.mode {
            return Ok((Some("spotify"), external_id));
        }
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
    pub fn update_iframe(&mut self) {
        if let Ok((Some("spotify"), Some(external_id))) = self.get_unique_source() {
            self.iframe = Some(format!(
                "https://open.spotify.com/embed/playlist/{}?utm_source=generator",
                external_id.id
            ));
        }
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

#[cfg(feature = "azure")]
impl CosmosEntity for RawList {
    type Entity = String;

    fn partition_key(&self) -> Self::Entity {
        self.user_id.clone()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Items {
    pub items: Vec<Option<ItemMetadata>>,
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
    NotFound,
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
    #[cfg(feature = "full")]
    RequestError(reqwest::Error),
    JSONError(serde_json::Error),
    #[cfg(feature = "azure")]
    CosmosError(azure_core::error::Error),
    IOError(std::io::Error),
    #[cfg(feature = "full")]
    SqlError(rusqlite::Error),
    #[cfg(feature = "full")]
    SerdeError(serde_rusqlite::Error),
    #[cfg(feature = "full")]
    ArrowError(ArrowError),
    #[cfg(feature = "full")]
    SerdeArrowError(serde_arrow::Error),
    Error(String),
}

#[cfg(feature = "full")]
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Error {
        Error::InternalError(InternalError::RequestError(e))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::InternalError(InternalError::JSONError(e))
    }
}

#[cfg(feature = "azure")]
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

#[cfg(feature = "full")]
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

#[cfg(feature = "full")]
impl From<Error> for Response<Body> {
    fn from(e: Error) -> Response<Body> {
        match e {
            Error::ClientError(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            Error::InternalError(e) => {
                eprintln!("server error: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Error::NotFound => StatusCode::NOT_FOUND.into_response(),
        }
    }
}

#[cfg(feature = "full")]
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Error {
        Error::InternalError(InternalError::SqlError(e))
    }
}

#[cfg(feature = "full")]
impl From<serde_rusqlite::Error> for Error {
    fn from(e: serde_rusqlite::Error) -> Error {
        Error::InternalError(InternalError::SerdeError(e))
    }
}

#[cfg(feature = "full")]
impl From<ArrowError> for Error {
    fn from(e: ArrowError) -> Error {
        Error::InternalError(InternalError::ArrowError(e))
    }
}

#[cfg(feature = "full")]
impl From<serde_arrow::Error> for Error {
    fn from(e: serde_arrow::Error) -> Error {
        Error::InternalError(InternalError::SerdeArrowError(e))
    }
}
