use ::spotify::SpotifyClient;
use arrow::{
    array::RecordBatch,
    datatypes::{Field, Schema},
    ipc::writer::FileWriter,
};
use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{Host, OriginalUri, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Redirect, Response},
    routing::{get, post},
    Router,
};
use axum_login::{
    tower_sessions::{Expiry, SessionManagerLayer},
    AuthManagerLayerBuilder,
};
use futures::{stream::FuturesUnordered, TryStreamExt};
use rusqlite::Connection;
use serde_arrow::schema::{SchemaLike, TracingOptions};
use serde_json::{Map, Value};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use time::Duration;
#[cfg(feature = "dev")]
use tower_http::services::ServeFile;
use tower_http::trace::TraceLayer;
use uuid::Uuid;
use zeroflops::{
    spotify::{Playlists, RecentTracks},
    storage::{
        CosmosQuery, CreateDocumentBuilder, DeleteDocumentBuilder, DocumentWriter,
        GetDocumentBuilder, QueryDocumentsBuilder, ReplaceDocumentBuilder, SessionClient,
        SqlSessionClient, View,
    },
    Error, Id, InternalError, Items, List, ListMode, Lists, RawList, UserId,
};
use zeroflops_web::{
    query::{self, IntoQuery},
    source::{self, spotify},
    user::{self, Auth, GoogleClient, SqlStore, User},
    Item, RawItem,
};

type AuthContext = axum_login::AuthSession<SqlStore>;
struct AuthWrapper(AuthContext);

fn get_user_or_demo_user(auth: AuthContext) -> UserId {
    if let Some(user) = auth.user {
        UserId(user.user_id)
    } else {
        UserId(DEMO_USER.to_owned())
    }
}

fn require_user(auth: AuthContext) -> Result<User, Response> {
    if let Some(user) = auth.user {
        Ok(user)
    } else {
        Err(StatusCode::UNAUTHORIZED.into_response())
    }
}

const DEMO_USER: &str = "demo";

async fn login_handler(
    OriginalUri(original_uri): OriginalUri,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    auth: AuthContext,
    Host(host): Host,
) -> Result<impl IntoResponse, Response> {
    let origin;
    #[cfg(feature = "dev")]
    {
        origin = format!("http://{}{}", host, original_uri.path());
    }
    #[cfg(not(feature = "dev"))]
    {
        origin = format!("https://{}{}", host, original_uri.path());
    }
    user::spotify_login(
        Connection::open(state.sql_store.path).map_err(Error::from)?,
        SpotifyClient,
        &mut AuthWrapper(auth),
        &params["code"],
        &origin,
    )
    .await?;
    Ok(Redirect::to("/"))
}

// TODO: fix rerender on logout
async fn logout_handler(
    State(state): State<Arc<AppState>>,
    mut auth: AuthContext,
) -> impl IntoResponse {
    if let Some(user) = &mut auth.user {
        // Log out of all sessions with axum-login by changing the user secret
        user.secret = zeroflops_web::user::generate_secret();
        let conn = Connection::open(state.sql_store.path).expect("Couldn't reset password");
        conn.execute(
            "UPDATE user SET secret = ?1 WHERE id = ?2",
            [&user.secret, &user.id],
        )
        .expect("Couldn't reset password");
        auth.logout().await.unwrap();
    }
    Redirect::to("/")
}

async fn google_login_handler(
    OriginalUri(original_uri): OriginalUri,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    auth: AuthContext,
    Host(host): Host,
) -> Result<impl IntoResponse, Response> {
    let origin;
    #[cfg(feature = "dev")]
    {
        origin = format!("http://{}{}", host, original_uri.path());
    }
    #[cfg(not(feature = "dev"))]
    {
        origin = format!("https://{}{}", host, original_uri.path());
    }
    user::google_login(
        Connection::open(state.sql_store.path).map_err(Error::from)?,
        GoogleClient,
        &mut AuthWrapper(auth),
        &params["code"],
        &origin,
    )
    .await?;
    Ok(Redirect::to("/"))
}

#[async_trait]
impl Auth for AuthWrapper {
    fn current_user(&self) -> &Option<User> {
        &self.0.user
    }

    async fn login(&mut self, user: &User) -> Result<(), Error> {
        self.0.login(user).await.unwrap();
        Ok(())
    }

    async fn logout(&mut self) {
        self.0.logout().await.unwrap();
    }
}

async fn get_lists(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    auth: AuthContext,
) -> Result<Json<Lists>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let query = if let Some("true") = params.get("favorite").map(String::as_ref) {
        "SELECT * FROM list WHERE favorite = true"
    } else {
        "SELECT * FROM list"
    };
    Ok(Json(Lists {
        lists: state
            .sql_client
            .query_documents::<RawList>(QueryDocumentsBuilder::new(
                "list",
                View::User(user_id.clone()),
                CosmosQuery::new(query.into_query()?),
            ))
            .await
            .map_err(Error::from)?
            .into_iter()
            .map(RawList::try_into)
            .collect::<Result<_, _>>()?,
    }))
}

async fn get_list(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthContext,
) -> Result<Json<List>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let mut list = source::get_list(&state.sql_client, &user_id, &id).await?;
    if let ListMode::View(_) = list.mode {
        list.items = query::get_view_items(&state.sql_client, &user_id, &list)
            .await?
            .collect();
    }
    Ok(Json(list))
}

async fn get_list_items(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthContext,
) -> Result<Json<Items>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let list = source::get_list(&state.sql_client, &user_id, &id).await?;
    Ok(Json(
        query::get_list_items(&state.sql_client, &user_id, list).await?,
    ))
}

async fn query_list(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    auth: AuthContext,
) -> Result<Vec<u8>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let list = source::get_list(&state.sql_client, &user_id, &id).await?;
    let records = query::query_list(&state.sql_client, &user_id, list, params.get("query")).await?;
    Ok(serialize_arrow(records)?)
}

fn serialize_arrow(mut records: Vec<Map<String, Value>>) -> Result<Vec<u8>, Error> {
    records = records
        .into_iter()
        .map(|mut m| {
            if let Some(Value::String(metadata)) = m.remove("metadata") {
                let metadata = serde_json::from_str(&metadata)?;
                if let Value::Object(mut metadata) = metadata {
                    m.append(&mut metadata);
                }
            }
            Ok(m)
        })
        .collect::<Result<_, Error>>()?;
    let fields = match Vec::<Field>::from_samples(
        &records,
        TracingOptions::default()
            .allow_null_fields(true)
            .coerce_numbers(true),
    ) {
        Ok(fields) => fields.to_vec(),
        Err(e) => {
            if e.message() == "No records found to determine schema" {
                return Ok(Vec::new());
            }
            return Err(Error::from(e));
        }
    };
    let buf = Vec::new();
    let schema = Schema::new(fields.clone());
    let arrays = RecordBatch::try_new(
        Arc::new(schema.clone()),
        serde_arrow::to_arrow(&fields, &records)?,
    )?;
    let mut writer = FileWriter::try_new(buf, &schema)?;
    writer.write(&arrays)?;
    writer.finish()?;
    writer.into_inner().map_err(Error::from)
}

async fn create_list(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<impl IntoResponse, Response> {
    let user = require_user(auth)?;
    let list = List {
        id: Uuid::new_v4().to_hyphenated().to_string(),
        user_id: user.user_id,
        mode: ListMode::User(None),
        name: String::from("New List"),
        sources: Vec::new(),
        iframe: None,
        items: Vec::new(),
        favorite: false,
        query: String::from("SELECT name, user_score FROM item"),
    };
    create_list_doc(&state.sql_client, list.clone(), false).await?;
    Ok((StatusCode::CREATED, Json(list)))
}

async fn update_list(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(list): Json<List>,
) -> Result<StatusCode, Response> {
    let user = require_user(auth)?;
    let user_id = UserId(user.user_id);
    if list.id != id {
        return Err(Error::client_error("list id doesn't match").into());
    }
    source::update_list_items(&state.sql_client, &user_id, list).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Does not delete items
async fn delete_list(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<StatusCode, Response> {
    let user = require_user(auth)?;
    let user_id = UserId(user.user_id);
    state
        .sql_client
        .write_document(DocumentWriter::<RawList>::Delete(DeleteDocumentBuilder {
            collection_name: "list",
            document_name: id,
            partition_key: user_id,
        }))
        .await
        .map_err(Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn find_items(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    auth: AuthContext,
) -> Result<impl IntoResponse, Response> {
    let user_id = get_user_or_demo_user(auth);
    let Some(query) = params.get("query") else {
        return Err(Error::client_error("invalid finder").into());
    };

    let (query, _) = query::rewrite_query(query)?;
    let values: Vec<Map<String, Value>> = state
        .sql_client
        .query_documents(QueryDocumentsBuilder::new(
            "item",
            View::User(user_id.clone()),
            CosmosQuery::new(query.clone()),
        ))
        .await
        .map_err(|e| {
            eprintln!("{}: {:?}", query, e);
            match e {
                Error::InternalError(InternalError::SqlError(e)) => Error::client_error(e.to_string()),
                e => e,
            }
        })?;
    Ok(serialize_arrow(values)?)
}

async fn handle_action(
    State(state): State<Arc<AppState>>,
    Query(mut params): Query<HashMap<String, String>>,
    auth: AuthContext,
    body: Bytes,
) -> Result<StatusCode, Response> {
    match params.get("action").map(String::as_ref) {
        Some("update") => {
            if let (Some(id), Some(win), Some(lose)) =
                (params.get("list"), params.get("win"), params.get("lose"))
            {
                let user_id = get_user_or_demo_user(auth);
                return Ok(handle_stats_update(state, user_id, id, win, lose).await?);
            }
        }
        Some("push") => {
            if let Some(id) = params.get("list") {
                let mut user = require_user(auth)?;
                return Ok(push_list(state, &mut user, id).await?);
            }
        }
        Some("import") => {
            if let (Some(source), Some(id)) = (params.remove("source"), params.remove("id")) {
                let user = require_user(auth)?;
                let user_id = UserId(user.user_id.clone());
                return Ok(import_list(state, user_id, &source, id, false).await?);
            }
        }
        Some("updateItems") => {
            let user_id = get_user_or_demo_user(auth);
            return Ok(update_items(state, user_id, body).await?);
        }
        _ => {}
    }
    Err((StatusCode::BAD_REQUEST, "action does not exist").into_response())
}

// TODO: handle spaces in IDs
async fn handle_stats_update(
    state: Arc<AppState>,
    user_id: UserId,
    id: &str,
    win: &str,
    lose: &str,
) -> Result<StatusCode, Error> {
    let client = &state.sql_client;
    let (list, win_item, lose_item) = futures::future::join3(
        source::get_list(client, &user_id, id),
        get_item_doc(client, &user_id, win),
        get_item_doc(client, &user_id, lose),
    )
    .await;
    let mut list = list?;
    let mut win_item = win_item?;
    let mut lose_item = lose_item?;

    let mut win_metadata = None;
    let mut lose_metadata = None;
    for i in &mut list.items {
        if i.id == win {
            win_metadata = Some(i);
        } else if i.id == lose {
            lose_metadata = Some(i);
        }
    }
    let win_metadata = win_metadata.unwrap();
    let lose_metadata = lose_metadata.unwrap();
    update_stats(
        &mut win_metadata.score,
        &mut win_metadata.wins,
        &mut lose_metadata.score,
        &mut lose_metadata.losses,
    );
    update_stats(
        &mut win_item.user_score,
        &mut win_item.user_wins,
        &mut lose_item.user_score,
        &mut lose_item.user_losses,
    );

    futures::future::try_join3(
        client.write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
            collection_name: "list",
            document_name: id.to_owned(),
            partition_key: user_id.clone(),
            document: RawList::from(list),
        })),
        client.write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
            collection_name: "item",
            document_name: win_item.id.clone(),
            partition_key: user_id.clone(),
            document: RawItem::from(win_item),
        })),
        client.write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
            collection_name: "item",
            document_name: lose_item.id.clone(),
            partition_key: user_id.clone(),
            document: RawItem::from(lose_item),
        })),
    )
    .await?;
    Ok(StatusCode::OK)
}

async fn push_list(state: Arc<AppState>, user: &mut User, id: &str) -> Result<StatusCode, Error> {
    let user_id = UserId(user.user_id.clone());
    let mut list = source::get_list(&state.sql_client, &user_id, id).await?;
    let (_, external_id) = list.get_unique_source()?;
    let access_token = spotify::get_access_token(&state.sql_client, user).await?;
    let external_id = if let Some(external_id) = external_id {
        spotify::update_playlist(access_token, &external_id.id, &list.name).await?;
        external_id.id.clone()
    } else {
        let mut playlist = spotify::create_playlist(access_token, &user_id, &list.name).await?;
        let id = Id {
            id: playlist.id,
            raw_id: playlist.external_urls.remove("spotify").unwrap(),
        };
        list.mode = match list.mode {
            ListMode::User(_) => ListMode::User(Some(id.clone())),
            ListMode::View(_) => ListMode::View(Some(id.clone())),
            _ => unreachable!(),
        };
        source::update_list(&state.sql_client, &user_id, list.clone()).await?;
        id.id
    };
    let ids: Vec<_> = match list.mode {
        ListMode::User(_) => query::get_list_items(&state.sql_client, &user_id, list)
            .await?
            .items
            .into_iter()
            .map(|i| i.unwrap().id)
            .collect(),
        ListMode::View(_) => query::get_view_items(&state.sql_client, &user_id, &list)
            .await?
            .map(|i| i.id)
            .collect(),
        _ => unreachable!(),
    };
    spotify::update_list(access_token, &external_id, &ids).await?;
    Ok(StatusCode::OK)
}

async fn import_list(
    state: Arc<AppState>,
    user_id: UserId,
    source: &str,
    id: String,
    favorite: bool,
) -> Result<StatusCode, Error> {
    match source.split_once(':') {
        Some(("spotify", source)) => {
            import_spotify(&state.sql_client, &user_id, source, id, favorite).await?
        }
        _ => todo!(),
    };
    Ok(StatusCode::CREATED)
}

pub async fn import_spotify(
    client: &SqlSessionClient,
    user_id: &UserId,
    source: &str,
    id: String,
    favorite: bool,
) -> Result<(), Error> {
    let is_upsert = user_id.0 == DEMO_USER;
    let items = match source {
        "playlist" => {
            let (mut list, items) = spotify::import_playlist(user_id, id).await?;
            list.favorite = favorite;
            create_list_doc(client, list, is_upsert).await?;
            items
        }
        "album" => {
            let (mut list, items) = spotify::import_album(user_id, id).await?;
            list.favorite = favorite;
            create_list_doc(client, list, is_upsert).await?;
            items
        }
        "track" => {
            let id = Id {
                id: id.clone(),
                raw_id: format!(
                    "https://open.spotify.com/embed/album/{}?utm_source=generator",
                    id
                ),
            };
            let (_, items) = spotify::get_track(user_id, id).await?;
            items
        }
        _ => todo!(),
    };
    source::create_items(client, items, is_upsert).await?;
    Ok(())
}

async fn get_item_doc(
    client: &SqlSessionClient,
    user_id: &UserId,
    id: &str,
) -> Result<Item, Error> {
    if let Some(item) = client
        .get_document::<RawItem>(GetDocumentBuilder::new(
            "item",
            id.to_owned(),
            user_id.clone(),
        ))
        .await?
    {
        item.try_into()
    } else {
        todo!()
    }
}

fn update_stats(
    win_score: &mut i32,
    win_wins: &mut i32,
    lose_score: &mut i32,
    lose_losses: &mut i32,
) {
    let diff = (32. / (1. + 10f64.powf((*win_score - *lose_score) as f64 / 400.))) as i32;
    *win_score += diff;
    *lose_score -= diff;
    *win_wins += 1;
    *lose_losses += 1;
}

async fn create_list_doc(
    client: &SqlSessionClient,
    list: List,
    is_upsert: bool,
) -> Result<(), Error> {
    client
        .write_document(DocumentWriter::Create(CreateDocumentBuilder {
            collection_name: "list",
            document: RawList::from(list),
            is_upsert,
        }))
        .await
        .map_err(Error::from)
}

async fn update_items(
    state: Arc<AppState>,
    user_id: UserId,
    body: Bytes,
) -> Result<StatusCode, Error> {
    let updates: HashMap<String, HashMap<String, Value>> = serde_json::from_slice(&body)?;
    let user_id = &user_id;
    updates
        .into_iter()
        .map(|(id, update)| async {
            let mut item = get_item_doc(&state.sql_client, user_id, &id).await?;
            for (k, v) in update {
                match k.as_str() {
                    "rating" => {
                        item.rating = serde_json::from_value(v)?;
                    }
                    "hidden" => {
                        item.hidden = serde_json::from_value(v)?;
                    }
                    _ => {}
                }
            }
            state
                .sql_client
                .write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
                    collection_name: "item",
                    document_name: id,
                    partition_key: user_id.clone(),
                    document: RawItem::from(item),
                }))
                .await
                .map_err(Error::from)
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect()
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_items(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    auth: AuthContext,
) -> Result<StatusCode, Response> {
    let user = require_user(auth)?;
    let user_id = UserId(user.user_id);
    let state = &state;
    let user_id = &user_id;
    params["ids"]
        .split(',')
        .map(|id| async move {
            state
                .sql_client
                .write_document(DocumentWriter::<RawItem>::Delete(DeleteDocumentBuilder {
                    collection_name: "item",
                    document_name: id.to_owned(),
                    partition_key: user_id.clone(),
                }))
                .await
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect()
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_handler(auth: AuthContext) -> Result<Json<zeroflops::User>, Response> {
    let user = require_user(auth)?;
    Ok(Json(zeroflops::User {
        user_id: user.user_id,
        spotify_user: user.spotify_credentials.as_ref().map(|c| c.user_id.clone()),
        spotify_url: user.spotify_credentials.map(|c| c.url),
        google_email: user.google_email,
    }))
}

async fn get_spotify_recent_tracks(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<RecentTracks>, Response> {
    let mut user = require_user(auth)?;
    let user_id = UserId(user.user_id.clone());
    if user.spotify_credentials.is_none() {
        return Err(Error::client_error("Spotify integration is required").into());
    };
    let access_token = spotify::get_access_token(&state.sql_client, &mut user).await?;
    Ok(Json(
        spotify::get_recent_tracks(&state.sql_client, &user_id, access_token).await?,
    ))
}

async fn get_spotify_playlists(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<Playlists>, Response> {
    let mut user = require_user(auth)?;
    if user.spotify_credentials.is_none() {
        return Err(Error::client_error("Spotify integration is required").into());
    };
    let access_token = spotify::get_access_token(&state.sql_client, &mut user).await?;
    Ok(Json(spotify::get_playlists(access_token).await?))
}

struct AppState {
    sql_store: SqlStore,
    sql_client: SqlSessionClient,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.
    let session_store = SqlStore { path: "zeroflops" };
    let shared_state = Arc::new(AppState {
        sql_store: session_store.clone(),
        sql_client: SqlSessionClient { path: "data" },
    });

    // Reset demo user data during startup in production
    if cfg!(not(feature = "dev")) {
        let demo_user = String::from(DEMO_USER);
        import_list(
            Arc::clone(&shared_state),
            UserId(demo_user.clone()),
            "spotify:playlist",
            "5MztFbRbMpyxbVYuOSfQV9".to_owned(),
            true,
        )
        .await
        .unwrap();
        // Generate IDs using random but constant UUIDs
        create_list_doc(
            &shared_state.sql_client,
            List {
                id: String::from("3c16df67-582d-449a-9862-0540f516d6b5"),
                user_id: demo_user.clone(),
                mode: ListMode::View(None),
                name: String::from("Artists"),
                sources: Vec::new(),
                iframe: None,
                items: Vec::new(),
                favorite: true,
                query: String::from("SELECT artists, AVG(user_score) FROM item GROUP BY artists"),
            },
            true,
        )
        .await
        .unwrap();
        create_list_doc(
            &shared_state.sql_client,
            List {
                id: String::from("4539f893-8471-4e23-b815-cd7c8b722016"),
                user_id: demo_user.clone(),
                mode: ListMode::View(None),
                name: String::from("Winners"),
                sources: Vec::new(),
                iframe: None,
                items: Vec::new(),
                favorite: true,
                query: String::from("SELECT name, user_score FROM item WHERE user_score >= 1500"),
            },
            true,
        )
        .await
        .unwrap();
        println!("Demo lists were created");
    }

    let session_layer = SessionManagerLayer::new(session_store.clone())
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::seconds(31536000)));

    let auth_layer = AuthManagerLayerBuilder::new(session_store, session_layer.clone()).build();

    let api_router = Router::new()
        .route("/lists", get(get_lists).post(create_list))
        .route(
            "/lists/:id",
            get(get_list).put(update_list).delete(delete_list),
        )
        .route("/lists/:id/items", get(get_list_items))
        .route("/lists/:id/query", get(query_list))
        .route("/items", get(find_items).delete(delete_items))
        .route("/", post(handle_action))
        .route("/login", get(login_handler))
        .route("/login/google", get(google_login_handler))
        .route("/logout", get(logout_handler))
        .route("/user", get(user_handler))
        .route("/spotify/recentTracks", get(get_spotify_recent_tracks))
        .route("/spotify/playlists", get(get_spotify_playlists))
        .with_state(shared_state);

    let app = Router::new()
        .nest("/api/", api_router)
        .layer(auth_layer)
        .layer(session_layer)
        .layer(TraceLayer::new_for_http());
    #[cfg(feature = "dev")]
    let app = {
        app.route_service(
            "/zeroflops_wasm.js",
            ServeFile::new("../zeroflops-wasm/pkg/zeroflops_wasm.js"),
        )
        .route_service(
            "/zeroflops_wasm_bg.wasm",
            ServeFile::new("../zeroflops-wasm/pkg/zeroflops_wasm_bg.wasm"),
        )
        .route_service(
            "/bootstrap.min.css",
            ServeFile::new("../zeroflops-wasm/www/bootstrap.min.css"),
        )
        .fallback_service(ServeFile::new("../zeroflops-wasm/www/index.html"))
    };

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(test)]
mod test {

    #[test]
    fn test_update_stats() {
        let mut first_score = 1500;
        let mut first_wins = 0;
        let mut first_losses = 0;
        let mut second_score = 1500;
        let mut second_wins = 0;
        let mut second_losses = 0;
        crate::update_stats(
            &mut first_score,
            &mut first_wins,
            &mut second_score,
            &mut second_losses,
        );
        assert_eq!(
            (
                first_score,
                first_wins,
                first_losses,
                second_score,
                second_wins,
                second_losses
            ),
            (1516, 1, 0, 1484, 0, 1)
        );

        crate::update_stats(
            &mut first_score,
            &mut first_wins,
            &mut second_score,
            &mut second_losses,
        );
        assert_eq!(
            (
                first_score,
                first_wins,
                first_losses,
                second_score,
                second_wins,
                second_losses
            ),
            (1530, 2, 0, 1470, 0, 2)
        );

        crate::update_stats(
            &mut second_score,
            &mut second_wins,
            &mut first_score,
            &mut first_losses,
        );
        assert_eq!(
            (
                first_score,
                first_wins,
                first_losses,
                second_score,
                second_wins,
                second_losses
            ),
            (1512, 2, 1, 1488, 1, 2)
        );
    }
}
