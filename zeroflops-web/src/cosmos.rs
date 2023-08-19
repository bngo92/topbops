use async_trait::async_trait;
use azure_data_cosmos::{
    prelude::{
        self as cosmos, ConsistencyLevel, CreateDocumentBuilder, DatabaseClient,
        DeleteDocumentBuilder, GetDocumentResponse, Param, Query, ReplaceDocumentBuilder,
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
    async fn write_document<F, T>(&self, f: F) -> Result<(), azure_core::error::Error>
    where
        F: FnOnce(&DatabaseClient) -> Result<T, azure_core::error::Error> + Send,
        T: IntoSessionToken + Send;
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
    async fn write_document<F, T>(&self, f: F) -> Result<(), azure_core::error::Error>
    where
        F: FnOnce(&DatabaseClient) -> Result<T, azure_core::error::Error> + Send,
        T: IntoSessionToken + Send,
    {
        let builder = if let Some(session) = self.session.read().unwrap().clone() {
            println!("{:?}", session);
            f(&self.db)?.consistency_level(session)
        } else {
            f(&self.db)?
        };
        let session_token = builder.into_session_token().await?;
        *self.session.write().unwrap() = Some(ConsistencyLevel::Session(session_token));
        Ok(())
    }
}

#[async_trait]
pub trait IntoSessionToken {
    fn consistency_level(self, consistency_level: ConsistencyLevel) -> Self;
    async fn into_session_token(self) -> Result<String, azure_core::error::Error>;
}

#[async_trait]
impl<T: Serialize + CosmosEntity + Send + 'static> IntoSessionToken for CreateDocumentBuilder<T> {
    fn consistency_level(self, consistency_level: ConsistencyLevel) -> Self {
        self.consistency_level(consistency_level)
    }

    async fn into_session_token(self) -> Result<String, azure_core::error::Error> {
        self.into_future().await.map(|r| r.session_token)
    }
}

#[async_trait]
impl<T: Serialize + CosmosEntity + Send + 'static> IntoSessionToken for ReplaceDocumentBuilder<T> {
    fn consistency_level(self, consistency_level: ConsistencyLevel) -> Self {
        self.consistency_level(consistency_level)
    }

    async fn into_session_token(self) -> Result<String, azure_core::error::Error> {
        self.into_future().await.map(|r| r.session_token)
    }
}

#[async_trait]
impl IntoSessionToken for DeleteDocumentBuilder {
    fn consistency_level(self, consistency_level: ConsistencyLevel) -> Self {
        self.consistency_level(consistency_level)
    }

    async fn into_session_token(self) -> Result<String, azure_core::error::Error> {
        self.into_future().await.map(|r| r.session_token)
    }
}
