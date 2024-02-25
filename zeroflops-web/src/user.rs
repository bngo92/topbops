use ::spotify::{AuthClient, SpotifyCredentials};
use async_trait::async_trait;
use axum_login::{
    tower_sessions::{
        session::{Id, Record},
        session_store, SessionStore,
    },
    AuthUser, AuthnBackend,
};
#[cfg(feature = "azure")]
use azure_data_cosmos::CosmosEntity;
use base64::prelude::{Engine, BASE64_STANDARD};
use hyper::{Body, Client, Method, Request, Uri};
use hyper_tls::HttpsConnector;
use rand::Rng;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use zeroflops::{
    storage::{
        CosmosParam, CosmosQuery, CreateDocumentBuilder, DocumentWriter, GetDocumentBuilder,
        QueryDocumentsBuilder, ReplaceDocumentBuilder, SessionClient,
    },
    Error,
};

#[async_trait]
pub trait Auth {
    fn current_user(&self) -> &Option<User>;
    async fn login(&mut self, user: &User) -> Result<(), Error>;
    async fn logout(&mut self);
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct User {
    pub id: String,
    pub user_id: String,
    pub secret: String,
    pub spotify_credentials: Option<SpotifyCredentials>,
    pub google_email: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RawUser {
    pub id: String,
    pub user_id: String,
    pub secret: String,
    pub spotify_credentials: Option<String>,
    pub google_email: Option<String>,
}

impl From<User> for RawUser {
    fn from(value: User) -> Self {
        RawUser {
            id: value.id,
            user_id: value.user_id,
            secret: value.secret,
            spotify_credentials: value
                .spotify_credentials
                .map(|s| serde_json::to_string(&s).expect("spotify credentials should serialize")),
            google_email: value.google_email,
        }
    }
}

impl TryFrom<RawUser> for User {
    type Error = Error;
    fn try_from(value: RawUser) -> Result<Self, Self::Error> {
        Ok(User {
            id: value.id,
            user_id: value.user_id,
            secret: value.secret,
            spotify_credentials: value
                .spotify_credentials
                .map(|s| serde_json::from_str(&s))
                .transpose()?,
            google_email: value.google_email,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleCredentials {
    pub access_token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleUser {
    pub email: String,
}

#[cfg(feature = "azure")]
impl CosmosEntity for RawUser {
    type Entity = String;

    fn partition_key(&self) -> Self::Entity {
        self.id.clone()
    }
}

pub async fn spotify_login(
    session_client: &impl SessionClient,
    spotify: impl AuthClient<Credentials = SpotifyCredentials>,
    auth: &mut impl Auth,
    code: &str,
    origin: &str,
) -> Result<(), Error> {
    let spotify_credentials = spotify.get_credentials(code, origin).await?;

    // Add Spotify identity to user if a session already exists
    if let Some(user) = &auth.current_user() {
        let mut user = user.clone();
        user.spotify_credentials = Some(spotify_credentials);
        session_client
            .write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "user",
                document_name: user.id.clone(),
                partition_key: user.id.clone(),
                document: RawUser::from(user.clone()),
            }))
            .await?;
        return Ok(());
    }

    let query = CosmosQuery::with_params(
        String::from("SELECT id, secret FROM user WHERE spotify_credentials->'user_id' = ?1"),
        [CosmosParam::new(
            String::from("@user_id"),
            spotify_credentials.user_id.clone(),
        )],
    );
    let mut results: Vec<HashMap<String, String>> = session_client
        .query_documents({
            let mut builder = QueryDocumentsBuilder::new("user", query);
            builder.query_cross_partition = true;
            builder.parallelize_cross_partition_query = true;
            builder
        })
        .await?;
    let user = if let Some(map) = results.pop() {
        let id = &map["id"];
        let mut user: User = session_client
            .get_document(GetDocumentBuilder::new("user", id.clone(), id.clone()))
            .await?
            .ok_or(Error::internal_error(format!(
                "User doesn't exist for {id}"
            )))?;
        // Refresh tokens
        user.spotify_credentials = Some(spotify_credentials);
        session_client
            .write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "user",
                document_name: user.id.clone(),
                partition_key: user.id.clone(),
                document: RawUser::from(user.clone()),
            }))
            .await?;
        user
    } else {
        let user = User {
            id: Uuid::new_v4().to_hyphenated().to_string(),
            user_id: spotify_credentials.user_id.clone(),
            secret: generate_secret(),
            google_email: None,
            spotify_credentials: Some(spotify_credentials),
        };
        session_client
            .write_document(DocumentWriter::Create(CreateDocumentBuilder {
                collection_name: "user",
                document: RawUser::from(user.clone()),
                is_upsert: false,
            }))
            .await?;
        user
    };
    auth.login(&user).await.unwrap();
    Ok(())
}

pub async fn google_login(
    session_client: &impl SessionClient,
    auth_client: impl AuthClient<Credentials = GoogleUser>,
    auth: &mut impl Auth,
    code: &str,
    origin: &str,
) -> Result<(), Error> {
    let google_user = auth_client.get_credentials(code, origin).await?;

    // Add Google identity to user if a session already exists
    if let Some(user) = &auth.current_user() {
        let mut user = user.clone();
        user.google_email = Some(google_user.email);
        session_client
            .write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "user",
                document_name: user.id.clone(),
                partition_key: user.id.clone(),
                document: RawUser::from(user.clone()),
            }))
            .await?;
        return Ok(());
    }

    let query = CosmosQuery::with_params(
        String::from("SELECT id FROM user WHERE google_email = ?1"),
        [CosmosParam::new(
            String::from("@google_email"),
            google_user.email.clone(),
        )],
    );
    let mut results: Vec<HashMap<String, String>> = session_client
        .query_documents({
            let mut builder = QueryDocumentsBuilder::new("user", query);
            builder.query_cross_partition = true;
            builder.parallelize_cross_partition_query = true;
            builder
        })
        .await?;
    let user = if let Some(map) = results.pop() {
        let id = &map["id"];
        session_client
            .get_document(GetDocumentBuilder::new("user", id.clone(), id.clone()))
            .await?
            .ok_or(Error::internal_error(format!(
                "User doesn't exist for {id}"
            )))?
    } else {
        let user = User {
            id: Uuid::new_v4().to_hyphenated().to_string(),
            user_id: google_user
                .email
                .split_once('@')
                .ok_or(Error::internal_error(format!(
                    "Received invalid email: {}",
                    google_user.email
                )))?
                .0
                .to_owned(),
            secret: generate_secret(),
            google_email: Some(google_user.email),
            spotify_credentials: None,
        };
        session_client
            .write_document(DocumentWriter::Create(CreateDocumentBuilder {
                collection_name: "user",
                document: RawUser::from(user.clone()),
                is_upsert: false,
            }))
            .await?;
        user
    };
    auth.login(&user).await.unwrap();
    Ok(())
}

pub struct GoogleClient;

#[async_trait]
impl AuthClient for GoogleClient {
    type Credentials = GoogleUser;

    async fn get_credentials(&self, code: &str, origin: &str) -> Result<Self::Credentials, Error> {
        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let uri: Uri = "https://oauth2.googleapis.com/token".parse().unwrap();
        let resp = client
            .request(
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "code={}&client_id=1038220726403-n55jha2cvprd8kdb4akdfvo0uiok4p5u.apps.googleusercontent.com&client_secret={}&redirect_uri={}&grant_type=authorization_code",
                        code,
                        std::env::var("GOOGLE_SECRET").expect("GOOGLE_SECRET is missing"),
                        origin
                    )))?,
            )
            .await?;
        let got = hyper::body::to_bytes(resp.into_body()).await?;
        let token: GoogleCredentials = serde_json::from_slice(&got)?;

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let uri: Uri = "https://openidconnect.googleapis.com/v1/userinfo"
            .parse()
            .unwrap();
        let resp = client
            .request(
                Request::builder()
                    .uri(uri)
                    .header("Authorization", format!("Bearer {}", token.access_token))
                    .body(Body::empty())?,
            )
            .await?;
        let got = hyper::body::to_bytes(resp.into_body()).await?;
        Ok(serde_json::from_slice(&got)?)
    }
}

impl AuthUser for User {
    type Id = String;

    fn id(&self) -> String {
        self.id.clone()
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.secret.as_bytes()
    }
}

#[derive(Clone, Debug)]
pub struct SqlStore {
    pub path: &'static str,
}

#[async_trait]
impl SessionStore for SqlStore {
    async fn load(&self, cookie_value: &Id) -> session_store::Result<Option<Record>> {
        let id = cookie_value.to_string();
        let conn = Connection::open(self.path)
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT data FROM session WHERE id = ?1")
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        match stmt.query_row([&id], |row| row.get::<_, String>(0)) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(record) => Ok(Some(record)),
                Err(e) => Err(session_store::Error::Decode(e.to_string())),
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(session_store::Error::Backend(e.to_string())),
        }
    }

    async fn save(&self, session: &Record) -> session_store::Result<()> {
        let conn = Connection::open(self.path)
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "INSERT INTO session (id, data) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            )
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        match stmt.execute([
            session.id.to_string(),
            serde_json::to_string(&session)
                .map_err(|e| session_store::Error::Encode(e.to_string()))?,
        ]) {
            Ok(_) => Ok(()),
            Err(e) => Err(session_store::Error::Backend(e.to_string())),
        }
    }

    async fn delete(&self, session: &Id) -> session_store::Result<()> {
        let id = session.to_string();
        let conn = Connection::open(self.path)
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        let mut stmt = conn
            .prepare("DELETE FROM session WHERE id = ?1")
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        match stmt.execute([&id]) {
            Ok(_) => Ok(()),
            Err(e) => Err(session_store::Error::Backend(e.to_string())),
        }
    }
}

#[async_trait]
impl AuthnBackend for SqlStore {
    type User = User;
    type Credentials = User;
    type Error = Error;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user: Option<Self::User> = self.get_user(&creds.user_id).await?;

        Ok(user.filter(|user| {
            password_auth::verify_password(creds.secret, &user.secret)
                .ok()
                .is_some() // We're using password-based authentication--this
                           // works by comparing our form input with an argon2
                           // password hash.
        }))
    }

    async fn get_user(&self, user_id: &String) -> Result<Option<Self::User>, Error> {
        let conn = Connection::open(self.path)?;
        let mut stmt = conn.prepare("SELECT * FROM user WHERE id = ?1")?;
        match stmt.query_row([&user_id], |row| {
            Ok(serde_rusqlite::from_row::<RawUser>(row).unwrap())
        }) {
            Ok(user) => user.try_into().map(Some),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::from(e)),
        }
    }
}

pub fn generate_secret() -> String {
    BASE64_STANDARD.encode(rand::thread_rng().gen::<[u8; 64]>())
}

#[cfg(test)]
mod test {
    use super::{Auth, GoogleUser, RawUser, User};
    use crate::query::test::{Mock, TestSessionClient};
    use async_trait::async_trait;
    use spotify::{AuthClient, SpotifyCredentials};
    use std::sync::{Arc, Mutex};
    use zeroflops::{
        storage::{
            CosmosParam, CosmosQuery, CreateDocumentBuilder, DocumentWriter, GetDocumentBuilder,
            QueryDocumentsBuilder, ReplaceDocumentBuilder,
        },
        Error,
    };

    struct TestAuth {
        current_user: Option<User>,
        expected_user: Option<User>,
    }

    impl TestAuth {
        fn new(current_user: Option<User>) -> TestAuth {
            TestAuth {
                current_user,
                expected_user: None,
            }
        }
    }

    #[async_trait]
    impl Auth for TestAuth {
        fn current_user(&self) -> &Option<User> {
            &self.current_user
        }

        async fn login(&mut self, user: &User) -> Result<(), Error> {
            self.expected_user = Some(user.clone());
            Ok(())
        }

        async fn logout(&mut self) {}
    }

    impl User {
        fn default() -> User {
            User {
                id: String::new(),
                user_id: String::new(),
                secret: String::new(),
                spotify_credentials: None,
                google_email: None,
            }
        }
    }

    struct TestSpotify {
        code: String,
    }

    #[async_trait]
    impl AuthClient for TestSpotify {
        type Credentials = SpotifyCredentials;

        async fn get_credentials(&self, code: &str, _: &str) -> Result<SpotifyCredentials, Error> {
            assert_eq!(self.code, code);
            Ok(SpotifyCredentials {
                user_id: "user".to_owned(),
                url: String::new(),
                access_token: code.to_owned(),
                refresh_token: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn test_spotify_login_new_user() {
        let client = TestSessionClient {
            get_mock: Mock::empty(),
            query_mock: Mock::new(vec!["[]"]),
            write_mock: Mock::new(vec![()]),
        };
        let mut auth = TestAuth::new(None);
        super::spotify_login(
            &client,
            TestSpotify {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        assert_eq!(
            *client.query_mock.call_args.lock().unwrap(),
            [QueryDocumentsBuilder {
                collection_name: "user",
                query: CosmosQuery::with_params(
                    "SELECT id, secret FROM user WHERE spotify_credentials->'user_id' = ?1"
                        .to_owned(),
                    vec![CosmosParam::new("@user_id".to_owned(), "user".to_owned())],
                ),
                query_cross_partition: true,
                parallelize_cross_partition_query: true,
            }]
        );
        let write_mock =
            Mutex::into_inner(Arc::into_inner(client.write_mock.call_args).unwrap()).unwrap();
        let DocumentWriter::Create(builder) = &write_mock[0] else {
            unreachable!()
        };
        let RawUser { id, secret, .. } = serde_json::de::from_str(&builder.document).unwrap();
        assert_eq!(
            write_mock,
            [DocumentWriter::Create(CreateDocumentBuilder {
                collection_name: "user",
                document: format!(
                    r#"{{"id":"{id}","user_id":"user","secret":"{secret}","spotify_credentials":"{{\"user_id\":\"user\",\"url\":\"\",\"access_token\":\"test\",\"refresh_token\":\"\"}}","google_email":null}}"#
                ),
                is_upsert: false,
            })]
        );
        assert_eq!(
            auth.expected_user,
            Some(User {
                user_id: "user".to_owned(),
                spotify_credentials: Some(SpotifyCredentials {
                    user_id: "user".to_owned(),
                    url: String::new(),
                    access_token: "test".to_owned(),
                    refresh_token: String::new(),
                }),
                google_email: None,
                ..auth.expected_user.clone().unwrap()
            }),
        );
    }

    #[tokio::test]
    async fn test_spotify_login_existing_user() {
        let client = TestSessionClient {
            get_mock: Mock::new(vec![r#"{"id":"","user_id":"","secret":""}"#]),
            query_mock: Mock::new(vec![r#"[{"id":"user"}]"#]),
            write_mock: Mock::new(vec![()]),
        };
        let mut auth = TestAuth::new(None);
        super::spotify_login(
            &client,
            TestSpotify {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        assert_eq!(
            *client.get_mock.call_args.lock().unwrap(),
            [GetDocumentBuilder {
                collection_name: "user",
                document_name: "user".to_owned(),
                partition_key: "user".to_owned(),
            }],
        );
        assert_eq!(
            *client.query_mock.call_args.lock().unwrap(),
            [QueryDocumentsBuilder {
                collection_name: "user",
                query: CosmosQuery::with_params(
                    "SELECT id, secret FROM user WHERE spotify_credentials->'user_id' = ?1"
                        .to_owned(),
                    vec![CosmosParam::new("@user_id".to_owned(), "user".to_owned())],
                ),
                query_cross_partition: true,
                parallelize_cross_partition_query: true,
            }]
        );
        assert_eq!(
            *client.write_mock.call_args.lock().unwrap(),
            [DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "user",
                document_name: "".to_owned(),
                partition_key: "".to_owned(),
                document: r#"{"id":"","user_id":"","secret":"","spotify_credentials":"{\"user_id\":\"user\",\"url\":\"\",\"access_token\":\"test\",\"refresh_token\":\"\"}","google_email":null}"#.to_owned(),
            })]
        );
        assert_eq!(
            auth.expected_user,
            Some(User {
                id: String::new(),
                user_id: String::new(),
                secret: String::new(),
                spotify_credentials: Some(SpotifyCredentials {
                    user_id: "user".to_owned(),
                    url: String::new(),
                    access_token: "test".to_owned(),
                    refresh_token: String::new(),
                }),
                google_email: None,
            }),
        );
    }

    #[tokio::test]
    async fn test_login_add_spotify_credentials() {
        let client = TestSessionClient {
            get_mock: Mock::empty(),
            query_mock: Mock::empty(),
            write_mock: Mock::new(vec![()]),
        };
        let mut auth = TestAuth {
            current_user: Some(User::default()),
            expected_user: None,
        };
        super::spotify_login(
            &client,
            TestSpotify {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        assert_eq!(
            *client.write_mock.call_args.lock().unwrap(),
            [DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "user",
                document_name: "".to_owned(),
                partition_key: "".to_owned(),
                document: r#"{"id":"","user_id":"","secret":"","spotify_credentials":"{\"user_id\":\"user\",\"url\":\"\",\"access_token\":\"test\",\"refresh_token\":\"\"}","google_email":null}"#.to_owned(),
            })]
        );
        assert!(auth.expected_user.is_none());
    }

    struct TestGoogle {
        code: String,
    }

    #[async_trait]
    impl AuthClient for TestGoogle {
        type Credentials = GoogleUser;

        async fn get_credentials(&self, code: &str, _: &str) -> Result<GoogleUser, Error> {
            assert_eq!(self.code, code);
            Ok(GoogleUser {
                email: "user@gmail.com".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn test_google_login_new_user() {
        let client = TestSessionClient {
            get_mock: Mock::empty(),
            query_mock: Mock::new(vec!["[]"]),
            write_mock: Mock::new(vec![()]),
        };
        let mut auth = TestAuth {
            current_user: None,
            expected_user: Some(User::default()),
        };
        super::google_login(
            &client,
            TestGoogle {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        assert_eq!(
            *client.query_mock.call_args.lock().unwrap(),
            [QueryDocumentsBuilder {
                collection_name: "user",
                query: CosmosQuery::with_params(
                    "SELECT id FROM user WHERE google_email = ?1".to_owned(),
                    vec![CosmosParam::new(
                        "@google_email".to_owned(),
                        "user@gmail.com".to_owned()
                    )],
                ),
                query_cross_partition: true,
                parallelize_cross_partition_query: true,
            }]
        );
        let write_mock =
            Mutex::into_inner(Arc::into_inner(client.write_mock.call_args).unwrap()).unwrap();
        let DocumentWriter::Create(builder) = &write_mock[0] else {
            unreachable!()
        };
        let User { id, secret, .. } = serde_json::de::from_str(&builder.document).unwrap();
        assert_eq!(
            write_mock,
            [DocumentWriter::Create(CreateDocumentBuilder {
                collection_name: "user",
                document: format!(
                    r#"{{"id":"{id}","user_id":"user","secret":"{secret}","spotify_credentials":null,"google_email":"user@gmail.com"}}"#
                ),
                is_upsert: false,
            })]
        );
        assert_eq!(
            auth.expected_user,
            Some(User {
                user_id: "user".to_owned(),
                spotify_credentials: None,
                google_email: Some("user@gmail.com".to_owned()),
                ..auth.expected_user.clone().unwrap()
            }),
        );
    }

    #[tokio::test]
    async fn test_google_login_existing_user() {
        let client = TestSessionClient {
            get_mock: Mock::new(vec![r#"{"id":"","user_id":"","secret":""}"#]),
            query_mock: Mock::new(vec![r#"[{"id":"user"}]"#]),
            write_mock: Mock::empty(),
        };
        let mut auth = TestAuth {
            current_user: None,
            expected_user: Some(User::default()),
        };
        super::google_login(
            &client,
            TestGoogle {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        assert_eq!(
            *client.get_mock.call_args.lock().unwrap(),
            [GetDocumentBuilder {
                collection_name: "user",
                document_name: "user".to_owned(),
                partition_key: "user".to_owned(),
            }],
        );
        assert_eq!(
            *client.query_mock.call_args.lock().unwrap(),
            [QueryDocumentsBuilder {
                collection_name: "user",
                query: CosmosQuery::with_params(
                    "SELECT id FROM user WHERE google_email = ?1".to_owned(),
                    vec![CosmosParam::new(
                        "@google_email".to_owned(),
                        "user@gmail.com".to_owned()
                    )],
                ),
                query_cross_partition: true,
                parallelize_cross_partition_query: true,
            }]
        );
        assert_eq!(auth.expected_user, Some(User::default()));
    }

    #[tokio::test]
    async fn test_login_add_google_credentials() {
        let client = TestSessionClient {
            get_mock: Mock::empty(),
            query_mock: Mock::empty(),
            write_mock: Mock::new(vec![()]),
        };
        let mut auth = TestAuth {
            current_user: Some(User {
                user_id: String::new(),
                id: String::new(),
                secret: String::new(),
                spotify_credentials: None,
                google_email: None,
            }),
            expected_user: None,
        };
        super::google_login(
            &client,
            TestGoogle {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        assert_eq!(
            *client.write_mock.call_args.lock().unwrap(),
            [DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "user",
                document_name: "".to_owned(),
                partition_key: "".to_owned(),
                document: r#"{"id":"","user_id":"","secret":"","spotify_credentials":null,"google_email":"user@gmail.com"}"#.to_owned(),
            })]
        );
        assert!(auth.expected_user.is_none());
    }
}
