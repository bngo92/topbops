use async_trait::async_trait;
use azure_data_cosmos::{
    prelude::{
        ConsistencyLevel, CreateDocumentBuilder, DatabaseClient, GetDocumentBuilder,
        GetDocumentResponse, QueryDocumentsBuilder, ReplaceDocumentBuilder,
    },
    CosmosEntity,
};
use futures::TryStreamExt;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{Arc, RwLock};

pub struct SessionClient {
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
}

impl SessionClient {
    pub fn new(db: DatabaseClient, session: Arc<RwLock<Option<ConsistencyLevel>>>) -> Self {
        Self { db, session }
    }

    /// Use the existing session token if it exists
    pub async fn get_document<F, T>(&self, f: F) -> Result<Option<T>, azure_core::error::Error>
    where
        F: FnOnce(&DatabaseClient) -> Result<GetDocumentBuilder<T>, azure_core::error::Error>,
        T: DeserializeOwned + Send + Sync,
    {
        let session = self.session.read().unwrap().clone();
        let f = if let Some(session) = session {
            f(&self.db)?.consistency_level(session)
        } else {
            f(&self.db)?
        };
        let (t, session_token) = match f.into_future().await? {
            GetDocumentResponse::Found(resp) => (Some(resp.document.document), resp.session_token),
            GetDocumentResponse::NotFound(resp) => (None, resp.session_token),
        };
        *self.session.write().unwrap() = Some(ConsistencyLevel::Session(session_token));
        Ok(t)
    }

    pub async fn query_documents<F, T>(&self, f: F) -> Result<Vec<T>, azure_core::error::Error>
    where
        F: FnOnce(&DatabaseClient) -> QueryDocumentsBuilder,
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
    pub async fn write_document<F, T1, T2>(&self, f: F) -> Result<(), azure_core::error::Error>
    where
        F: FnOnce(&DatabaseClient) -> Result<T1, azure_core::error::Error>,
        T1: IntoSessionToken<T2>,
        T2: Serialize + CosmosEntity + Send + 'static,
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
pub trait IntoSessionToken<T> {
    fn consistency_level(self, consistency_level: ConsistencyLevel) -> Self;
    async fn into_session_token(self) -> Result<String, azure_core::error::Error>;
}

#[async_trait]
impl<T: Serialize + CosmosEntity + Send + 'static> IntoSessionToken<T>
    for CreateDocumentBuilder<T>
{
    fn consistency_level(self, consistency_level: ConsistencyLevel) -> Self {
        self.consistency_level(consistency_level)
    }

    async fn into_session_token(self) -> Result<String, azure_core::error::Error> {
        self.into_future().await.map(|r| r.session_token)
    }
}

#[async_trait]
impl<T: Serialize + CosmosEntity + Send + 'static> IntoSessionToken<T>
    for ReplaceDocumentBuilder<T>
{
    fn consistency_level(self, consistency_level: ConsistencyLevel) -> Self {
        self.consistency_level(consistency_level)
    }

    async fn into_session_token(self) -> Result<String, azure_core::error::Error> {
        self.into_future().await.map(|r| r.session_token)
    }
}
