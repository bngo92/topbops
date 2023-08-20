use async_trait::async_trait;
use azure_data_cosmos::{
    prelude::{
        self as cosmos, ConsistencyLevel, DatabaseClient, GetDocumentResponse, Param, Query,
    },
    CosmosEntity,
};
use futures::TryStreamExt;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::sync::{Arc, RwLock};
use zeroflops::Error;

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

    fn into_query(self) -> Query {
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

    fn into_cosmos<T: DeserializeOwned + Send + Sync>(
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

    fn into_cosmos(self, db: &DatabaseClient) -> Result<cosmos::QueryDocumentsBuilder, Error> {
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

pub struct CosmosSessionClient {
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
}

impl CosmosSessionClient {
    pub fn new(db: DatabaseClient, session: Arc<RwLock<Option<ConsistencyLevel>>>) -> Self {
        Self { db, session }
    }
}

#[async_trait]
impl SessionClient for CosmosSessionClient {
    /// Use the existing session token if it exists
    async fn get_document<T>(&self, builder: GetDocumentBuilder) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned + Send + Sync,
    {
        let session = self.session.read().unwrap().clone();
        let f = if let Some(session) = session {
            builder.into_cosmos(&self.db)?.consistency_level(session)
        } else {
            builder.into_cosmos(&self.db)?
        };
        let (t, session_token) = match f.into_future().await? {
            GetDocumentResponse::Found(resp) => (Some(resp.document.document), resp.session_token),
            GetDocumentResponse::NotFound(resp) => (None, resp.session_token),
        };
        *self.session.write().unwrap() = Some(ConsistencyLevel::Session(session_token));
        Ok(t)
    }

    async fn query_documents<T>(&self, builder: QueryDocumentsBuilder) -> Result<Vec<T>, Error>
    where
        T: DeserializeOwned + Send + Sync,
    {
        let session = self.session.read().unwrap().clone();
        let (stream, results) = if let Some(session) = session {
            println!("{:?}", session);
            let mut stream = builder
                .into_cosmos(&self.db)?
                .consistency_level(session)
                .into_stream();
            let resp = stream.try_next().await?.map(|r| r.results);
            (stream, resp)
        } else {
            let mut stream = builder.into_cosmos(&self.db)?.into_stream();
            let resp = if let Some(r) = stream.try_next().await? {
                *self.session.write().unwrap() = Some(ConsistencyLevel::Session(r.session_token));
                Some(r.results)
            } else {
                None
            };
            (stream, resp)
        };
        Ok(results
            .into_iter()
            .chain(
                stream
                    .try_collect::<Vec<_>>()
                    .await?
                    .into_iter()
                    .map(|r| r.results),
            )
            .flatten()
            .map(|(d, _)| d)
            .collect())
    }

    /// CosmosDB creates new session tokens after writes
    async fn write_document<T>(
        &self,
        builder: DocumentWriter<T>,
    ) -> Result<(), azure_core::error::Error>
    where
        T: Serialize + CosmosEntity + Send + 'static,
    {
        let session = self.session.read().unwrap().clone();
        let session_token = if let Some(session) = session {
            let session: ConsistencyLevel = session;
            println!("{:?}", session);
            match builder {
                DocumentWriter::Create(builder) => builder
                    .into_cosmos(&self.db)?
                    .consistency_level(session)
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
                DocumentWriter::Replace(builder) => builder
                    .into_cosmos(&self.db)?
                    .consistency_level(session)
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
                DocumentWriter::Delete(builder) => builder
                    .into_cosmos(&self.db)?
                    .consistency_level(session)
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
            }
        } else {
            match builder {
                DocumentWriter::Create(builder) => builder
                    .into_cosmos(&self.db)?
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
                DocumentWriter::Replace(builder) => builder
                    .into_cosmos(&self.db)?
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
                DocumentWriter::Delete(builder) => builder
                    .into_cosmos(&self.db)?
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
            }
        };
        *self.session.write().unwrap() = Some(ConsistencyLevel::Session(session_token));
        Ok(())
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

impl<T: Serialize + CosmosEntity + Send + 'static> CreateDocumentBuilder<T> {
    fn into_cosmos(
        self,
        db: &DatabaseClient,
    ) -> Result<cosmos::CreateDocumentBuilder<T>, azure_core::error::Error> {
        let mut builder = db
            .collection_client(self.collection_name)
            .create_document(self.document);
        if self.is_upsert {
            builder = builder.is_upsert(true)
        }
        Ok(builder)
    }
}

#[derive(Debug, PartialEq)]
pub struct ReplaceDocumentBuilder<T> {
    pub collection_name: &'static str,
    pub document_name: String,
    pub partition_key: String,
    pub document: T,
}

impl<T: Serialize + CosmosEntity + Send + 'static> ReplaceDocumentBuilder<T> {
    fn into_cosmos(
        self,
        db: &DatabaseClient,
    ) -> Result<cosmos::ReplaceDocumentBuilder<T>, azure_core::error::Error> {
        Ok(db
            .collection_client(self.collection_name)
            .document_client(&self.document_name, &self.partition_key)?
            .replace_document(self.document))
    }
}

#[derive(Debug, PartialEq)]
pub struct DeleteDocumentBuilder {
    pub collection_name: &'static str,
    pub document_name: String,
    pub partition_key: String,
}

impl DeleteDocumentBuilder {
    fn into_cosmos(
        self,
        db: &DatabaseClient,
    ) -> Result<cosmos::DeleteDocumentBuilder, azure_core::error::Error> {
        Ok(db
            .collection_client(self.collection_name)
            .document_client(&self.document_name, &self.partition_key)?
            .delete_document())
    }
}
