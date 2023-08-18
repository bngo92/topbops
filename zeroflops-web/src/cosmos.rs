use async_trait::async_trait;
use azure_data_cosmos::{
    prelude::{
        self as cosmos, ConsistencyLevel, CreateDocumentBuilder, DatabaseClient,
        DeleteDocumentBuilder, GetDocumentResponse, QueryDocumentsBuilder, ReplaceDocumentBuilder,
    },
    CosmosEntity,
};
use futures::TryStreamExt;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{Arc, RwLock};
use zeroflops::Error;

#[derive(Debug, PartialEq)]
pub struct GetDocumentBuilder<'a> {
    pub collection_name: &'static str,
    pub document_name: &'a str,
    pub partition_key: &'a str,
}

impl GetDocumentBuilder<'_> {
    pub fn new<'a>(
        collection_name: &'static str,
        document_name: &'a str,
        partition_key: &'a str,
    ) -> GetDocumentBuilder<'a> {
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

#[async_trait]
pub trait SessionClient {
    /// Use the existing session token if it exists
    async fn get_document<'a, T>(
        &self,
        builder: GetDocumentBuilder<'a>,
    ) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned + Send + Sync;

    async fn query_documents<F, T>(&self, f: F) -> Result<Vec<T>, azure_core::error::Error>
    where
        F: FnOnce(&DatabaseClient) -> QueryDocumentsBuilder + Send,
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
    async fn get_document<'a, T>(&self, builder: GetDocumentBuilder<'a>) -> Result<Option<T>, Error>
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

    async fn query_documents<F, T>(&self, f: F) -> Result<Vec<T>, azure_core::error::Error>
    where
        F: FnOnce(&DatabaseClient) -> QueryDocumentsBuilder + Send,
        T: DeserializeOwned + Send + Sync,
    {
        let session = self.session.read().unwrap().clone();
        let (stream, results) = if let Some(session) = session {
            println!("{:?}", session);
            let mut stream = f(&self.db).consistency_level(session).into_stream();
            let resp = stream.try_next().await?.map(|r| r.results);
            (stream, resp)
        } else {
            let mut stream = f(&self.db).into_stream();
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
