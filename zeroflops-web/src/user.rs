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
use rand::Rng;
use reqwest::Client;
use rusqlite::{Connection, OptionalExtension, Params, Row};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;
use zeroflops::Error;

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
    conn: impl SqlConnection,
    spotify: impl AuthClient<Credentials = SpotifyCredentials>,
    auth: &mut impl Auth,
    code: &str,
    origin: &str,
) -> Result<(), Error> {
    let spotify_credentials = spotify.get_credentials(code, origin).await?;

    // Add Spotify identity to user if a session already exists
    if let Some(user) = &auth.current_user() {
        conn.execute(
            "UPDATE user SET spotify_credentials = ?1 WHERE id = ?2",
            Param::Positional::<()>(&[&serde_json::to_string(&spotify_credentials)?, &user.id]),
        )?;
        return Ok(());
    }

    let user = if let Some(user) = conn
        .query_row(
            "SELECT * FROM user WHERE spotify_credentials->'user_id' = ?1",
            [&spotify_credentials.user_id],
            |row| Ok(serde_rusqlite::from_row::<RawUser>(row)),
        )
        .optional()?
        .transpose()?
    {
        let mut user = User::try_from(user)?;
        // Refresh tokens
        conn.execute(
            "UPDATE user SET spotify_credentials = ?1 WHERE id = ?2",
            Param::Positional::<()>(&[&serde_json::to_string(&spotify_credentials)?, &user.id]),
        )?;
        user.spotify_credentials = Some(spotify_credentials);
        user
    } else {
        let user = User {
            id: Uuid::new_v4().to_hyphenated().to_string(),
            user_id: spotify_credentials.user_id.clone(),
            secret: generate_secret(),
            google_email: None,
            spotify_credentials: Some(spotify_credentials),
        };
        conn.execute(
                "INSERT INTO user (id, user_id, secret, spotify_credentials, google_email) VALUES (:id, :user_id, :secret, :spotify_credentials, :google_email)",
                Param::Named(RawUser::from(user.clone())),
            )?;
        user
    };
    auth.login(&user).await.unwrap();
    Ok(())
}

pub async fn google_login(
    conn: impl SqlConnection,
    auth_client: impl AuthClient<Credentials = GoogleUser>,
    auth: &mut impl Auth,
    code: &str,
    origin: &str,
) -> Result<(), Error> {
    let google_user = auth_client.get_credentials(code, origin).await?;

    // Add Google identity to user if a session already exists
    if let Some(user) = &auth.current_user() {
        conn.execute(
            "UPDATE user SET google_email = ?1 WHERE id = ?2",
            Param::Positional::<()>(&[&google_user.email, &user.id]),
        )?;
        return Ok(());
    }

    let user = if let Some(user) = conn
        .query_row(
            "SELECT * FROM user WHERE google_email = ?1",
            [&google_user.email],
            |row| Ok(serde_rusqlite::from_row::<RawUser>(row)),
        )
        .optional()?
        .transpose()?
    {
        User::try_from(user)?
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
        conn.execute(
                "INSERT INTO user (id, user_id, secret, spotify_credentials, google_email) VALUES (:id, :user_id, :secret, :spotify_credentials, :google_email)",
                Param::Named(RawUser::from(user.clone()))
            )?;
        user
    };
    auth.login(&user).await.unwrap();
    Ok(())
}

pub trait SqlConnection {
    fn execute<T: Serialize>(&self, sql: &str, params: Param<'_, T>) -> Result<usize, Error>;
    fn query_row<T, P, F>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Result<T, serde_rusqlite::Error>>
    where
        T: DeserializeOwned + Send + Sync,
        P: Params + std::fmt::Debug,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<Result<T, serde_rusqlite::Error>>;
}

impl SqlConnection for Connection {
    fn execute<T: Serialize>(&self, sql: &str, params: Param<'_, T>) -> Result<usize, Error> {
        match params {
            Param::Positional(params) => self.execute(sql, params).map_err(Error::from),
            Param::Named(params) => self
                .execute(
                    sql,
                    serde_rusqlite::to_params_named(params)?
                        .to_slice()
                        .as_slice(),
                )
                .map_err(Error::from),
        }
    }

    fn query_row<T, P, F>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Result<T, serde_rusqlite::Error>>
    where
        T: DeserializeOwned + Send + Sync,
        P: Params + std::fmt::Debug,
        F: FnOnce(&Row<'_>) -> rusqlite::Result<Result<T, serde_rusqlite::Error>>,
    {
        self.query_row(sql, params, f)
    }
}

pub enum Param<'a, T> {
    Positional(&'a [&'a str; 2]),
    Named(T),
}

pub struct GoogleClient;

#[async_trait]
impl AuthClient for GoogleClient {
    type Credentials = GoogleUser;

    async fn get_credentials(&self, code: &str, origin: &str) -> Result<Self::Credentials, Error> {
        let client = Client::new();
        let token: GoogleCredentials = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code),
                (
                    "client_id",
                    "1038220726403-n55jha2cvprd8kdb4akdfvo0uiok4p5u.apps.googleusercontent.com",
                ),
                (
                    "client_secret",
                    &std::env::var("GOOGLE_SECRET").expect("GOOGLE_SECRET is missing"),
                ),
                ("redirect_uri", origin),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await?
            .json()
            .await?;

        Ok(client
            .get("https://openidconnect.googleapis.com/v1/userinfo")
            .header("Authorization", format!("Bearer {}", token.access_token))
            .send()
            .await?
            .json()
            .await?)
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
    use super::{Auth, GoogleUser, Param, RawUser, SqlConnection, User};
    use crate::query::test::Mock;
    use async_trait::async_trait;
    use rusqlite::{Params, Row};
    use serde::{de::DeserializeOwned, Serialize};
    use spotify::{AuthClient, SpotifyCredentials};
    use std::sync::{Arc, Mutex};
    use zeroflops::Error;

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

    struct TestConnection {
        execute_mock: Mock<(String, String), usize>,
        query_row_mock: Mock<(String, String), Result<&'static str, rusqlite::Error>>,
    }

    impl SqlConnection for Arc<Mutex<TestConnection>> {
        fn execute<T: Serialize>(&self, sql: &str, params: Param<'_, T>) -> Result<usize, Error> {
            Ok(self.lock().unwrap().execute_mock.call((
                sql.to_owned(),
                match params {
                    Param::Positional(params) => format!("{params:?}"),
                    Param::Named(params) => serde_json::to_string(&params).unwrap(),
                },
            )))
        }

        fn query_row<T, P, F>(
            &self,
            sql: &str,
            params: P,
            _: F,
        ) -> rusqlite::Result<Result<T, serde_rusqlite::Error>>
        where
            T: DeserializeOwned + Send + Sync,
            P: Params + std::fmt::Debug,
            F: FnOnce(&Row<'_>) -> rusqlite::Result<Result<T, serde_rusqlite::Error>>,
        {
            match self
                .lock()
                .unwrap()
                .query_row_mock
                .call((sql.to_owned(), format!("{params:?}")))
            {
                Ok(s) => Ok(Ok(serde_json::from_str(s).unwrap())),
                Err(e) => Err(e),
            }
        }
    }

    #[tokio::test]
    async fn test_spotify_login_new_user() {
        let conn = Arc::new(Mutex::new(TestConnection {
            execute_mock: Mock::new(vec![1]),
            query_row_mock: Mock::new(vec![Err(rusqlite::Error::QueryReturnedNoRows)]),
        }));
        let mut auth = TestAuth::new(None);
        super::spotify_login(
            Arc::clone(&conn),
            TestSpotify {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        let conn = Mutex::into_inner(Arc::into_inner(conn).unwrap()).unwrap();
        let write_mock =
            Mutex::into_inner(Arc::into_inner(conn.execute_mock.call_args).unwrap()).unwrap();
        let RawUser { id, secret, .. } = serde_json::de::from_str(&write_mock[0].1).unwrap();
        assert_eq!(
            write_mock,
            [(
                "INSERT INTO user (id, user_id, secret, spotify_credentials, google_email) VALUES (:id, :user_id, :secret, :spotify_credentials, :google_email)".to_owned(),
                format!(
                    r#"{{"id":"{id}","user_id":"user","secret":"{secret}","spotify_credentials":"{{\"user_id\":\"user\",\"url\":\"\",\"access_token\":\"test\",\"refresh_token\":\"\"}}","google_email":null}}"#
                ),
            )]
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
        let conn = Arc::new(Mutex::new(TestConnection {
            execute_mock: Mock::new(vec![1]),
            query_row_mock: Mock::new(vec![Ok(
                r#"{"id":"","user_id":"","secret":"","spotify_credentials":null,"google_email":null}"#,
            )]),
        }));
        let mut auth = TestAuth::new(None);
        super::spotify_login(
            Arc::clone(&conn),
            TestSpotify {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        let conn = Mutex::into_inner(Arc::into_inner(conn).unwrap()).unwrap();
        assert_eq!(
            *conn.query_row_mock.call_args.lock().unwrap(),
            [(
                "SELECT * FROM user WHERE spotify_credentials->'user_id' = ?1".to_owned(),
                "[\"user\"]".to_owned()
            )]
        );
        assert_eq!(
            *conn.execute_mock.call_args.lock().unwrap(),
            [(
                "UPDATE user SET spotify_credentials = ?1 WHERE id = ?2".to_owned(),
                r#"["{\"user_id\":\"user\",\"url\":\"\",\"access_token\":\"test\",\"refresh_token\":\"\"}", ""]"#.to_owned()
            )]
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
        let conn = Arc::new(Mutex::new(TestConnection {
            execute_mock: Mock::new(vec![1]),
            query_row_mock: Mock::empty(),
        }));
        let mut auth = TestAuth {
            current_user: Some(User::default()),
            expected_user: None,
        };
        super::spotify_login(
            Arc::clone(&conn),
            TestSpotify {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        let conn = Mutex::into_inner(Arc::into_inner(conn).unwrap()).unwrap();
        assert_eq!(
            *conn.execute_mock.call_args.lock().unwrap(),
            [(
                "UPDATE user SET spotify_credentials = ?1 WHERE id = ?2".to_owned(),
                r#"["{\"user_id\":\"user\",\"url\":\"\",\"access_token\":\"test\",\"refresh_token\":\"\"}", ""]"#.to_owned()
            )]
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
        let conn = Arc::new(Mutex::new(TestConnection {
            execute_mock: Mock::new(vec![1]),
            query_row_mock: Mock::new(vec![Err(rusqlite::Error::QueryReturnedNoRows)]),
        }));
        let mut auth = TestAuth {
            current_user: None,
            expected_user: Some(User::default()),
        };
        super::google_login(
            Arc::clone(&conn),
            TestGoogle {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        let conn = Mutex::into_inner(Arc::into_inner(conn).unwrap()).unwrap();
        let write_mock =
            Mutex::into_inner(Arc::into_inner(conn.execute_mock.call_args).unwrap()).unwrap();
        let RawUser { id, secret, .. } = serde_json::de::from_str(&write_mock[0].1).unwrap();
        assert_eq!(
            write_mock,
            [(
                "INSERT INTO user (id, user_id, secret, spotify_credentials, google_email) VALUES (:id, :user_id, :secret, :spotify_credentials, :google_email)".to_owned(),
                format!(
                    r#"{{"id":"{id}","user_id":"user","secret":"{secret}","spotify_credentials":null,"google_email":"user@gmail.com"}}"#
                ),
            )]
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
        let conn = Arc::new(Mutex::new(TestConnection {
            execute_mock: Mock::empty(),
            query_row_mock: Mock::new(vec![Ok(
                r#"{"id":"","user_id":"","secret":"","spotify_credentials":null,"google_email":null}"#,
            )]),
        }));
        let mut auth = TestAuth {
            current_user: None,
            expected_user: Some(User::default()),
        };
        super::google_login(
            Arc::clone(&conn),
            TestGoogle {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        let conn = Mutex::into_inner(Arc::into_inner(conn).unwrap()).unwrap();
        assert_eq!(
            *conn.query_row_mock.call_args.lock().unwrap(),
            [(
                "SELECT * FROM user WHERE google_email = ?1".to_owned(),
                "[\"user@gmail.com\"]".to_owned()
            )]
        );
        assert_eq!(auth.expected_user, Some(User::default()));
    }

    #[tokio::test]
    async fn test_login_add_google_credentials() {
        let conn = Arc::new(Mutex::new(TestConnection {
            execute_mock: Mock::new(vec![1]),
            query_row_mock: Mock::empty(),
        }));
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
            Arc::clone(&conn),
            TestGoogle {
                code: "test".to_owned(),
            },
            &mut auth,
            "test",
            "http://localhost:3000/api/login",
        )
        .await
        .unwrap();
        let conn = Mutex::into_inner(Arc::into_inner(conn).unwrap()).unwrap();
        assert_eq!(
            *conn.execute_mock.call_args.lock().unwrap(),
            [(
                "UPDATE user SET google_email = ?1 WHERE id = ?2".to_owned(),
                r#"["user@gmail.com", ""]"#.to_owned()
            )]
        );
        assert!(auth.expected_user.is_none());
    }
}
