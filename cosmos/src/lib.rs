use async_trait::async_trait;
use azure_data_cosmos::{
    prelude::{self as cosmos, ConsistencyLevel, DatabaseClient, GetDocumentResponse},
    CosmosEntity,
};
use futures::TryStreamExt;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{Arc, RwLock};
use zeroflops::{
    storage::{
        CreateDocumentBuilder, DeleteDocumentBuilder, DocumentWriter, GetDocumentBuilder,
        QueryDocumentsBuilder, ReplaceDocumentBuilder, SessionClient,
    },
    Error,
};

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
                DocumentWriter::Create(builder) => create_cosmos(builder, &self.db)?
                    .consistency_level(session)
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
                DocumentWriter::Replace(builder) => replace_cosmos(builder, &self.db)?
                    .consistency_level(session)
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
                DocumentWriter::Delete(builder) => delete_cosmos(builder, &self.db)?
                    .consistency_level(session)
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
            }
        } else {
            match builder {
                DocumentWriter::Create(builder) => create_cosmos(builder, &self.db)?
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
                DocumentWriter::Replace(builder) => replace_cosmos(builder, &self.db)?
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
                DocumentWriter::Delete(builder) => delete_cosmos(builder, &self.db)?
                    .into_future()
                    .await
                    .map(|r| r.session_token)?,
            }
        };
        *self.session.write().unwrap() = Some(ConsistencyLevel::Session(session_token));
        Ok(())
    }
}

fn create_cosmos<T: Serialize + CosmosEntity + Send + 'static>(
    builder: CreateDocumentBuilder<T>,
    db: &DatabaseClient,
) -> Result<cosmos::CreateDocumentBuilder<T>, azure_core::error::Error> {
    let mut b = db
        .collection_client(builder.collection_name)
        .create_document(builder.document);
    if builder.is_upsert {
        b = b.is_upsert(true)
    }
    Ok(b)
}

fn replace_cosmos<T: Serialize + CosmosEntity + Send + 'static>(
    builder: ReplaceDocumentBuilder<T>,
    db: &DatabaseClient,
) -> Result<cosmos::ReplaceDocumentBuilder<T>, azure_core::error::Error> {
    Ok(db
        .collection_client(builder.collection_name)
        .document_client(&builder.document_name, &builder.partition_key)?
        .replace_document(builder.document))
}

fn delete_cosmos(
    builder: DeleteDocumentBuilder,
    db: &DatabaseClient,
) -> Result<cosmos::DeleteDocumentBuilder, azure_core::error::Error> {
    Ok(db
        .collection_client(builder.collection_name)
        .document_client(&builder.document_name, &builder.partition_key)?
        .delete_document())
}
