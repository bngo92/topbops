use crate::Error;
use async_trait::async_trait;
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
    pub query: CosmosQuery,
    pub query_cross_partition: bool,
    pub parallelize_cross_partition_query: bool,
}

impl QueryDocumentsBuilder {
    pub fn new(collection_name: &'static str, query: CosmosQuery) -> QueryDocumentsBuilder {
        QueryDocumentsBuilder {
            collection_name,
            query,
            query_cross_partition: false,
            parallelize_cross_partition_query: false,
        }
    }

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
    async fn write_document<T>(
        &self,
        builder: DocumentWriter<T>,
    ) -> Result<(), azure_core::error::Error>
    where
        T: Serialize + CosmosEntity + Send + 'static;
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
        let mut stmt = conn.prepare(&format!(
            "SELECT * FROM {} WHERE id = ?1 AND user_id = ?2",
            builder.collection_name
        ))?;
        stmt.query_row([&builder.document_name, &builder.partition_key], |row| {
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
        let mut stmt = conn.prepare(&builder.query.query)?;
        let query = stmt.query(rusqlite::params_from_iter(params))?;
        serde_rusqlite::from_rows(query)
            .collect::<Result<_, _>>()
            .map_err(Error::from)
    }

    /// CosmosDB creates new session tokens after writes
    async fn write_document<T>(
        &self,
        _builder: DocumentWriter<T>,
    ) -> Result<(), azure_core::error::Error>
    where
        T: Serialize + CosmosEntity + Send + 'static,
    {
        todo!()
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
