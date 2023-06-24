use async_trait::async_trait;
use axum_login::{
    axum_sessions::async_session::{Session, SessionStore},
    secrecy::SecretVec,
    AuthUser, UserStore,
};
use azure_data_cosmos::{
    prelude::{DatabaseClient, GetDocumentResponse},
    CosmosEntity,
};
use base64::prelude::{Engine, BASE64_STANDARD};
use rand::Rng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    pub user_id: String,
    pub secret: String,
    pub spotify_credentials: Option<SpotifyCredentials>,
    pub google_email: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpotifyCredentials {
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleCredentials {
    pub access_token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleUser {
    pub email: String,
}

impl CosmosEntity for User {
    type Entity = String;

    fn partition_key(&self) -> Self::Entity {
        self.id.clone()
    }
}

impl AuthUser<String> for User {
    fn get_id(&self) -> String {
        self.id.clone()
    }

    fn get_password_hash(&self) -> SecretVec<u8> {
        SecretVec::new(self.secret.clone().into())
    }
}

#[derive(Clone, Debug)]
pub struct CosmosStore {
    pub db: DatabaseClient,
}

#[async_trait]
impl SessionStore for CosmosStore {
    async fn load_session(&self, cookie_value: String) -> anyhow::Result<Option<Session>> {
        let id = Session::id_from_cookie_value(&cookie_value)?;
        let client = self
            .db
            .collection_client("sessions")
            .document_client(id.clone(), &id)?;
        if let GetDocumentResponse::Found(list) = client.get_document().into_future().await? {
            Ok(Some(list.document.document))
        } else {
            Ok(None)
        }
    }

    async fn store_session(&self, session: Session) -> anyhow::Result<Option<String>> {
        self.db
            .collection_client("sessions")
            .create_document(CosmosSession(session.clone()))
            .is_upsert(true)
            .into_future()
            .await?;

        session.reset_data_changed();
        Ok(session.into_cookie_value())
    }

    async fn destroy_session(&self, session: Session) -> anyhow::Result<()> {
        self.db
            .collection_client("sessions")
            .document_client(session.id(), &session.id())?
            .delete_document()
            .into_future()
            .await?;
        Ok(())
    }

    async fn clear_store(&self) -> anyhow::Result<()> {
        todo!()
    }
}

#[derive(Serialize)]
struct CosmosSession(Session);

impl CosmosEntity for CosmosSession {
    type Entity = String;

    fn partition_key(&self) -> Self::Entity {
        self.0.id().to_owned()
    }
}

#[async_trait]
impl<Role> UserStore<String, Role> for CosmosStore
where
    Role: PartialOrd + PartialEq + Clone + Send + Sync + 'static,
    User: AuthUser<String, Role> + DeserializeOwned,
{
    type User = User;

    async fn load_user(&self, user_id: &String) -> Result<Option<Self::User>, eyre::Report> {
        let client = self
            .db
            .collection_client("users")
            .document_client(user_id, user_id)?;
        if let GetDocumentResponse::Found(user) = client.get_document().into_future().await? {
            Ok(Some(user.document.document))
        } else {
            Ok(None)
        }
    }
}

pub fn generate_secret() -> String {
    BASE64_STANDARD.encode(rand::thread_rng().gen::<[u8; 64]>())
}
