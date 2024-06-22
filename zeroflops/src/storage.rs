use crate::{Error, UserId};
use async_trait::async_trait;
#[cfg(feature = "azure")]
use azure_data_cosmos::prelude::{self as cosmos, DatabaseClient, Param, Query as AzureQuery};
use rusqlite::{config::DbConfig, limits::Limit, Connection, OpenFlags, OptionalExtension, ToSql};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use sqlparser::ast::Query;

#[derive(Debug, PartialEq)]
pub struct CosmosQuery {
    pub query: Query,
    parameters: Vec<CosmosParam>,
}

impl CosmosQuery {
    pub fn new(query: Query) -> CosmosQuery {
        CosmosQuery {
            query,
            parameters: Vec::new(),
        }
    }

    pub fn with_params<T: Into<Vec<CosmosParam>>>(query: Query, parameters: T) -> CosmosQuery {
        CosmosQuery {
            query,
            parameters: parameters.into(),
        }
    }

    #[cfg(feature = "azure")]
    pub fn into_query(self) -> AzureQuery {
        AzureQuery::with_params(
            self.query.to_string(),
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
    pub partition_key: View,
}

impl GetDocumentBuilder {
    pub fn new(
        collection_name: &'static str,
        document_name: String,
        partition_key: View,
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
    User(UserId),
    List(UserId, Vec<String>),
    Public,
    PublicList(Vec<String>),
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
        match builder.partition_key {
            View::User(user_id) => {
                conn.execute_batch(&format!(
                    "CREATE TEMP VIEW list AS SELECT * FROM _list WHERE user_id = '{user_id}';
                        CREATE TEMP VIEW item AS SELECT * FROM _item WHERE user_id = '{user_id}';",
                    user_id = user_id.0
                ))?;
            }
            View::Public => {
                conn.execute_batch(
                    "CREATE TEMP VIEW list AS SELECT * FROM _list WHERE public = true;
                    CREATE TEMP VIEW item AS SELECT _item.* FROM _list, json_each(_list.items) JOIN _item ON _item.id=json_each.value->>'id' WHERE public = true;",
                )?;
            }
            _ => return Err(Error::internal_error("unsupported view")),
        }
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
        let query = builder.query.query.to_string();
        if query.contains("sqlite_schema") {
            return Err(Error::client_error("no such table: sqlite_schema"));
        }
        if query.contains("sqlite_master") {
            return Err(Error::client_error("no such table: sqlite_master"));
        }
        if query.contains("_list") {
            return Err(Error::client_error("no such table: _list"));
        }
        if query.contains("_item") {
            return Err(Error::client_error("no such table: _item"));
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
        let conn = Connection::open_with_flags(self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        // https://www.sqlite.org/security.html
        conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
        conn.set_limit(Limit::SQLITE_LIMIT_LENGTH, 1_000_000);
        conn.set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 100_000);
        conn.set_limit(Limit::SQLITE_LIMIT_COLUMN, 100);
        conn.set_limit(Limit::SQLITE_LIMIT_EXPR_DEPTH, 10);
        conn.set_limit(Limit::SQLITE_LIMIT_COMPOUND_SELECT, 3);
        conn.set_limit(Limit::SQLITE_LIMIT_VDBE_OP, 25_000);
        conn.set_limit(Limit::SQLITE_LIMIT_FUNCTION_ARG, 8);
        conn.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0);
        conn.set_limit(Limit::SQLITE_LIMIT_LIKE_PATTERN_LENGTH, 50);
        conn.set_limit(Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 10);
        // Emulate partitions with views
        match builder.partition_key {
            View::User(user_id) => {
                conn.execute_batch(&format!(
                    "CREATE TEMP VIEW list AS SELECT * FROM _list WHERE user_id = '{user_id}';
                        CREATE TEMP VIEW item AS SELECT * FROM _item WHERE user_id = '{user_id}';",
                    user_id = user_id.0
                ))?;
            }
            View::List(user_id, ids) => {
                conn.execute_batch(
                    &format!(
                        "CREATE TEMP VIEW list AS SELECT * FROM _list WHERE user_id = '{user_id}';
                        CREATE TEMP VIEW item AS SELECT * FROM _item WHERE user_id = '{user_id}' AND id IN ({});",
                        ids.iter()
                            .map(|id| format!("'{id}'"))
                            .collect::<Vec<_>>()
                            .join(","),
                        user_id=user_id.0
                    ),
                )?;
            }
            View::Public => {
                conn.execute_batch(
                    "CREATE TEMP VIEW list AS SELECT * FROM _list WHERE public = true;
                    CREATE TEMP VIEW item AS SELECT _item.* FROM _list, json_each(_list.items) JOIN _item ON _item.id=json_each.value->>'id' WHERE public = true;",
                )?;
            }
            View::PublicList(ids) => {
                conn.execute_batch(
                    &format!(
                        "CREATE TEMP VIEW list AS SELECT * FROM _list WHERE public = true;
                        CREATE TEMP VIEW item AS SELECT _item.* FROM _list, json_each(_list.items) JOIN _item ON _item.id=json_each.value->>'id' WHERE public = true AND _item.id IN ({});",
                        ids.iter()
                            .map(|id| format!("'{id}'"))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                )?;
            }
        }
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
        let conn = Connection::open(self.path)?;
        match builder {
            DocumentWriter::Create(builder) => {
                conn.execute(
                    get_insert_stmt(builder.collection_name, builder.is_upsert),
                    serde_rusqlite::to_params_named(&builder.document)?
                        .to_slice()
                        .as_slice(),
                )?;
            }
            DocumentWriter::Replace(builder) => {
                let (stmt, fields) = get_update_stmt(builder.collection_name);
                conn.execute(
                    stmt,
                    serde_rusqlite::to_params_named_with_fields(&builder.document, fields)?
                        .to_slice()
                        .as_slice(),
                )?;
            }
            DocumentWriter::Delete(builder) => {
                conn.execute(
                    &format!("DELETE FROM _{} WHERE id = ?1", builder.collection_name),
                    [builder.document_name],
                )?;
            }
        }
        Ok(())
    }
}

fn get_insert_stmt(collection_name: &str, is_upsert: bool) -> &str {
    match (collection_name, is_upsert) {
        ("item", false) => "INSERT INTO _item (id, user_id, type, name, iframe, rating, user_score, user_wins, user_losses, metadata, hidden) VALUES (:id, :user_id, :type, :name, :iframe, :rating, :user_score, :user_wins, :user_losses, :metadata, :hidden)",
        ("list", false) => "INSERT INTO _list (id, user_id, mode, name, sources, iframe, items, favorite, query, public) VALUES (:id, :user_id, :mode, :name, :sources, :iframe, :items, :favorite, :query, :public)",
        // is_upsert is currently only used to reset demo lists and items
        ("item", true) => "INSERT INTO _item (id, user_id, type, name, iframe, rating, user_score, user_wins, user_losses, metadata, hidden) VALUES (:id, :user_id, :type, :name, :iframe, :rating, :user_score, :user_wins, :user_losses, :metadata, :hidden) ON CONFLICT(id, user_id) DO UPDATE SET rating=excluded.rating, user_score=excluded.user_score, user_wins=excluded.user_wins, user_losses=excluded.user_losses",
        ("list", true) => "INSERT INTO _list (id, user_id, mode, name, sources, iframe, items, favorite, query, public) VALUES (:id, :user_id, :mode, :name, :sources, :iframe, :items, :favorite, :query, :public) ON CONFLICT(id, user_id) DO UPDATE SET items=excluded.items, query=excluded.query, public=excluded.public",
        _ => unreachable!()
    }
}

fn get_update_stmt(collection_name: &str) -> (&str, &[&str]) {
    match collection_name {
        "item" => ("UPDATE _item SET rating = :rating, user_score = :user_score, user_wins = :user_wins, user_losses = :user_losses WHERE id = :id AND user_id = :user_id", &["id", "user_id", "rating", "user_score", "user_wins", "user_losses"]),
        "list" => ("UPDATE _list SET mode = :mode, name = :name, sources = :sources, iframe = :iframe, items = :items, favorite = :favorite, query = :query, public = :public WHERE id = :id AND user_id = :user_id", &["id", "user_id", "mode", "name", "sources", "iframe", "items", "favorite", "query", "public"]),
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
    pub partition_key: UserId,
    pub document: T,
}

#[derive(Debug, PartialEq)]
pub struct DeleteDocumentBuilder {
    pub collection_name: &'static str,
    pub document_name: String,
    pub partition_key: UserId,
}
