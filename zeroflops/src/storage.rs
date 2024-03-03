use crate::Error;
use async_trait::async_trait;
#[cfg(feature = "azure")]
use azure_data_cosmos::{
    prelude::{self as cosmos, DatabaseClient, Param, Query},
    CosmosEntity,
};
use rusqlite::{Connection, OptionalExtension, ToSql};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

#[derive(Debug, PartialEq)]
pub struct CosmosQuery {
    query: String,
    parameters: Vec<CosmosParam>,
}

impl CosmosQuery {
    pub fn new(query: String) -> CosmosQuery {
        CosmosQuery {
            query,
            parameters: Vec::new(),
        }
    }

    pub fn with_params<T: Into<Vec<CosmosParam>>>(query: String, parameters: T) -> CosmosQuery {
        CosmosQuery {
            query,
            parameters: parameters.into(),
        }
    }

    #[cfg(feature = "azure")]
    pub fn into_query(self) -> Query {
        Query::with_params(
            self.query,
            self.parameters
                .into_iter()
                .map(|param| Param::new(param.name, param.value))
                .collect::<Vec<_>>(),
        )
    }
}

#[derive(Debug, PartialEq)]
pub struct CosmosParam {
    name: String,
    value: Value,
}

impl CosmosParam {
    pub fn new<T: Into<Value>>(name: String, value: T) -> CosmosParam {
        CosmosParam {
            name,
            value: value.into(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct GetDocumentBuilder {
    pub collection_name: &'static str,
    pub document_name: String,
    pub partition_key: String,
}

impl GetDocumentBuilder {
    pub fn new(
        collection_name: &'static str,
        document_name: String,
        partition_key: String,
    ) -> GetDocumentBuilder {
        GetDocumentBuilder {
            collection_name,
            document_name,
            partition_key,
        }
    }

    #[cfg(feature = "azure")]
    pub fn into_cosmos<T: DeserializeOwned + Send + Sync>(
        self,
        db: &DatabaseClient,
    ) -> Result<cosmos::GetDocumentBuilder<T>, Error> {
        Ok(db
            .collection_client(self.collection_name)
            .document_client(self.document_name, &self.partition_key)?
            .get_document())
    }
}

#[derive(Debug, PartialEq)]
pub struct QueryDocumentsBuilder {
    pub collection_name: &'static str,
    pub partition_key: View,
    pub query: CosmosQuery,
    pub query_cross_partition: bool,
    pub parallelize_cross_partition_query: bool,
}

impl QueryDocumentsBuilder {
    pub fn new(
        collection_name: &'static str,
        partition_key: View,
        query: CosmosQuery,
    ) -> QueryDocumentsBuilder {
        QueryDocumentsBuilder {
            collection_name,
            partition_key,
            query,
            query_cross_partition: false,
            parallelize_cross_partition_query: false,
        }
    }

    #[cfg(feature = "azure")]
    pub fn into_cosmos(self, db: &DatabaseClient) -> Result<cosmos::QueryDocumentsBuilder, Error> {
        let mut builder = db
            .collection_client(self.collection_name)
            .query_documents(self.query.into_query());
        if self.query_cross_partition {
            builder = builder.query_cross_partition(true)
        }
        if self.parallelize_cross_partition_query {
            builder = builder.parallelize_cross_partition_query(true)
        }
        Ok(builder)
    }
}

#[derive(Debug, PartialEq)]
pub enum View {
    User(String),
}

#[async_trait]
pub trait SessionClient {
    /// Use the existing session token if it exists
    async fn get_document<T>(&self, builder: GetDocumentBuilder) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned + Send + Sync;

    async fn query_documents<T>(&self, builder: QueryDocumentsBuilder) -> Result<Vec<T>, Error>
    where
        T: DeserializeOwned + Send + Sync;

    /// CosmosDB creates new session tokens after writes
    async fn write_document<T>(&self, builder: DocumentWriter<T>) -> Result<(), Error>
    where
        T: Serialize + Send + 'static;
}

pub struct SqlSessionClient {
    pub path: &'static str,
}

#[async_trait]
impl SessionClient for SqlSessionClient {
    async fn get_document<T>(&self, builder: GetDocumentBuilder) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned + Send + Sync,
    {
        let conn = Connection::open(self.path)?;
        let user_id = builder.partition_key;
        conn.execute(
            &format!("CREATE TEMP VIEW list AS SELECT * FROM _list WHERE user_id = '{user_id}'"),
            [],
        )?;
        conn.execute(
            &format!("CREATE TEMP VIEW item AS SELECT * FROM _item WHERE user_id = '{user_id}'"),
            [],
        )?;
        let mut stmt = conn.prepare(&format!(
            "SELECT * FROM {} WHERE id = ?1",
            builder.collection_name
        ))?;
        stmt.query_row([&builder.document_name], |row| {
            Ok(serde_rusqlite::from_row(row))
        })
        .optional()?
        .transpose()
        .map_err(Error::from)
    }

    async fn query_documents<T>(&self, builder: QueryDocumentsBuilder) -> Result<Vec<T>, Error>
    where
        T: DeserializeOwned + Send + Sync,
    {
        let query = builder.query.query;
        if query.contains("_list") {
            return Err(Error::client_error("Parse error: no such table: _list"));
        }
        if query.contains("_item") {
            return Err(Error::client_error("Parse error: no such table: _item"));
        }
        let params: Vec<_> = builder
            .query
            .parameters
            .into_iter()
            .map(|p| {
                if let Some(s) = p.value.as_str() {
                    Box::new(s.to_owned()) as Box<dyn ToSql>
                } else {
                    Box::new(p.value) as Box<dyn ToSql>
                }
            })
            .collect();
        let conn = Connection::open(self.path)?;
        // Emulate partitions with views
        let View::User(user_id) = &builder.partition_key;
        conn.execute(
            &format!("CREATE TEMP VIEW list AS SELECT * FROM _list WHERE user_id = '{user_id}'"),
            [],
        )?;
        conn.execute(
            &format!("CREATE TEMP VIEW item AS SELECT * FROM _item WHERE user_id = '{user_id}'"),
            [],
        )?;
        let mut stmt = conn.prepare(&query)?;
        let query = stmt.query(rusqlite::params_from_iter(params))?;
        serde_rusqlite::from_rows(query)
            .collect::<Result<_, _>>()
            .map_err(Error::from)
    }

    /// CosmosDB creates new session tokens after writes
    async fn write_document<T>(&self, builder: DocumentWriter<T>) -> Result<(), Error>
    where
        T: Serialize + Send + 'static,
    {
        let conn = Connection::open(self.path).unwrap();
        match builder {
            DocumentWriter::Create(builder) => {
                conn.execute(
                    get_insert_stmt(builder.collection_name, builder.is_upsert),
                    serde_rusqlite::to_params_named(&builder.document)
                        .unwrap()
                        .to_slice()
                        .as_slice(),
                )
                .unwrap();
            }
            DocumentWriter::Replace(builder) => {
                let (stmt, fields) = get_update_stmt(builder.collection_name);
                conn.execute(
                    stmt,
                    serde_rusqlite::to_params_named_with_fields(&builder.document, fields)
                        .unwrap()
                        .to_slice()
                        .as_slice(),
                )
                .unwrap();
            }
            DocumentWriter::Delete(builder) => {
                conn.execute(
                    &format!("DELETE FROM _{} WHERE id = ?1", builder.collection_name),
                    [builder.document_name],
                )
                .unwrap();
            }
        }
        Ok(())
    }
}

fn get_insert_stmt(collection_name: &str, is_upsert: bool) -> &str {
    match (collection_name, is_upsert) {
        ("item", false) => "INSERT INTO _item (id, user_id, type, name, iframe, rating, user_score, user_wins, user_losses, metadata, hidden) VALUES (:id, :user_id, :type, :name, :iframe, :rating, :user_score, :user_wins, :user_losses, :metadata, :hidden)",
        ("item", true) => "INSERT INTO _item (id, user_id, type, name, iframe, rating, user_score, user_wins, user_losses, metadata, hidden) VALUES (:id, :user_id, :type, :name, :iframe, :rating, :user_score, :user_wins, :user_losses, :metadata, :hidden) ON CONFLICT(id, user_id) DO UPDATE SET rating=excluded.rating, user_score=excluded.user_score, user_wins=excluded.user_wins, user_losses=excluded.user_losses",
        ("list", false) => "INSERT INTO _list (id, user_id, mode, name, sources, iframe, items, favorite, query) VALUES (:id, :user_id, :mode, :name, :sources, :iframe, :items, :favorite, :query)",
        ("list", true) => "INSERT INTO _list (id, user_id, mode, name, sources, iframe, items, favorite, query) VALUES (:id, :user_id, :mode, :name, :sources, :iframe, :items, :favorite, :query) ON CONFLICT(id, user_id) DO UPDATE SET items=excluded.items, query=excluded.query",
        _ => unreachable!()
    }
}

fn get_update_stmt(collection_name: &str) -> (&str, &[&str]) {
    match collection_name {
        "item" => ("UPDATE _item SET rating = :rating, user_score = :user_score, user_wins = :user_wins, user_losses = :user_losses WHERE id = :id AND user_id = :user_id", &["id", "user_id", "rating", "user_score", "user_wins", "user_losses"]),
        "list" => ("UPDATE _list SET name = :name, sources = :sources, items = :items, favorite = :favorite, query = :query WHERE id = :id AND user_id = :user_id", &["id", "user_id", "name", "sources", "items", "favorite", "query"]),
        _ => unreachable!()
    }
}

#[derive(Debug, PartialEq)]
pub enum DocumentWriter<T> {
    Create(CreateDocumentBuilder<T>),
    Replace(ReplaceDocumentBuilder<T>),
    Delete(DeleteDocumentBuilder),
}

#[derive(Debug, PartialEq)]
pub struct CreateDocumentBuilder<T> {
    pub collection_name: &'static str,
    pub document: T,
    pub is_upsert: bool,
}

#[derive(Debug, PartialEq)]
pub struct ReplaceDocumentBuilder<T> {
    pub collection_name: &'static str,
    pub document_name: String,
    pub partition_key: String,
    pub document: T,
}

#[derive(Debug, PartialEq)]
pub struct DeleteDocumentBuilder {
    pub collection_name: &'static str,
    pub document_name: String,
    pub partition_key: String,
}
