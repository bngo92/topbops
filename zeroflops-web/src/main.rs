use ::spotify::SpotifyClient;
use axum::{
    body::Bytes,
    extract::{Host, OriginalUri, Path, Query, State},
    response::{IntoResponse, Json, Redirect, Response},
    routing::{get, post},
    Router,
};
use axum_login::{axum_sessions::SessionLayer, AuthLayer};
use azure_data_cosmos::prelude::{AuthorizationToken, CosmosClient};
use base64::prelude::{Engine, BASE64_STANDARD};
use futures::{StreamExt, TryStreamExt};
use hyper::StatusCode;
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
#[cfg(feature = "dev")]
use tower_http::services::ServeFile;
use tower_http::trace::TraceLayer;
use uuid::Uuid;
use zeroflops::{
    spotify::{Playlists, RecentTracks},
    Error, Id, ItemQuery, List, ListMode, Lists,
};
use zeroflops_web::{
    cosmos::{
        CosmosParam, CosmosQuery, CosmosSessionClient, CreateDocumentBuilder,
        DeleteDocumentBuilder, DocumentWriter, GetDocumentBuilder, QueryDocumentsBuilder,
        ReplaceDocumentBuilder, SessionClient,
    },
    query, source,
    source::spotify,
    user,
    user::{CosmosStore, GoogleClient, User},
    Item, UserId,
};

type AuthContext = axum_login::extractors::AuthContext<String, User, CosmosStore>;

fn get_user_or_demo_user(auth: AuthContext) -> UserId {
    if let Some(user) = auth.current_user {
        UserId(user.user_id)
    } else {
        UserId(DEMO_USER.to_owned())
    }
}

fn require_user(auth: AuthContext) -> Result<User, Response> {
    if let Some(user) = auth.current_user {
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
    mut auth: AuthContext,
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
    let user = user::spotify_login(
        &state.client,
        SpotifyClient,
        &auth.current_user,
        &params["code"],
        &origin,
    )
    .await?;
    auth.login(&user).await.unwrap();
    Ok(Redirect::to("/"))
}

// TODO: fix rerender on logout
async fn logout_handler(
    State(state): State<Arc<AppState>>,
    mut auth: AuthContext,
) -> impl IntoResponse {
    if let Some(user) = &mut auth.current_user {
        // Log out of all sessions with axum-login by changing the user secret
        user.secret = zeroflops_web::user::generate_secret();
        state
            .client
            .write_document(DocumentWriter::Create(CreateDocumentBuilder {
                collection_name: "users",
                document: user.clone(),
                is_upsert: true,
            }))
            .await
            .expect("Couldn't reset password");
        auth.logout().await;
    }
    Redirect::to("/")
}

async fn google_login_handler(
    OriginalUri(original_uri): OriginalUri,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    mut auth: AuthContext,
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
    let user = user::google_login(
        &state.client,
        GoogleClient,
        &auth.current_user,
        &params["code"],
        &origin,
    )
    .await?;
    auth.login(&user).await.unwrap();
    Ok(Redirect::to("/"))
}

async fn get_lists(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    auth: AuthContext,
) -> Result<Json<Lists>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let query = if let Some("true") = params.get("favorite").map(String::as_ref) {
        "SELECT * FROM c WHERE c.user_id = @user_id AND c.favorite = true"
    } else {
        "SELECT * FROM c WHERE c.user_id = @user_id"
    };
    Ok(Json(Lists {
        lists: state
            .client
            .query_documents(QueryDocumentsBuilder::new(
                "lists",
                CosmosQuery::with_params(
                    String::from(query),
                    [CosmosParam::new(String::from("@user_id"), user_id.0)],
                ),
            ))
            .await
            .map_err(Error::from)?,
    }))
}

async fn get_list(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthContext,
) -> Result<Json<List>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let list = source::get_list(&state.client, &user_id, &id).await?;
    Ok(Json(list))
}

async fn get_list_query(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthContext,
) -> Result<Json<ItemQuery>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let list = source::get_list(&state.client, &user_id, &id).await?;
    Ok(Json(
        get_list_query_impl(&state.client, &user_id, list).await?,
    ))
}

async fn get_list_query_impl(
    client: &CosmosSessionClient,
    user_id: &UserId,
    list: List,
) -> Result<ItemQuery, Error> {
    if list.items.is_empty() {
        Ok(ItemQuery {
            fields: Vec::new(),
            items: Vec::new(),
        })
    } else {
        let (query, fields, map, ids) = query::rewrite_list_query(&list, user_id)?;
        let mut items: Vec<_> = client
            .query_documents(QueryDocumentsBuilder::new(
                "items",
                CosmosQuery::new(query.to_string()),
            ))
            .await
            .map_err(Error::from)?;
        // Use list item order if an ordering wasn't provided
        if query.order_by.is_empty() {
            let mut item_metadata: HashMap<_, _> = items
                .into_iter()
                .map(|r: Map<String, Value>| (r["id"].to_string(), r))
                .collect();
            items = ids
                .into_iter()
                .filter_map(|id| item_metadata.remove(&id))
                .collect();
        };
        Ok(ItemQuery {
            fields,
            items: items
                .into_iter()
                .map(|r| {
                    let mut iter = r.values();
                    let metadata = if map.is_empty() {
                        None
                    } else {
                        Some(map[iter.next_back().unwrap().as_str().unwrap()].clone())
                    };
                    zeroflops::Item {
                        values: iter.map(format_value).collect(),
                        metadata,
                    }
                })
                .collect(),
        })
    }
}

async fn get_list_items(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthContext,
) -> Result<Json<Vec<Map<String, Value>>>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let list = source::get_list(&state.client, &user_id, &id).await?;
    Ok(Json(
        get_list_items_impl(&state.client, &user_id, list).await?,
    ))
}

async fn get_list_items_impl(
    client: &CosmosSessionClient,
    user_id: &UserId,
    list: List,
) -> Result<Vec<Map<String, Value>>, Error> {
    let query = if let ListMode::View = &list.mode {
        query::rewrite_query(&list.query, user_id)?.0.to_string()
    } else if list.items.is_empty() {
        return Ok(Vec::new());
    } else {
        String::from("SELECT c.id, c.type, c.name, c.rating, c.user_score, c.user_wins, c.user_losses, c.hidden, c.metadata FROM c WHERE c.user_id = @user_id AND ARRAY_CONTAINS(@ids, c.id)")
    };
    client
        .query_documents(QueryDocumentsBuilder::new(
            "items",
            CosmosQuery::with_params(
                query,
                [
                    CosmosParam::new(String::from("@user_id"), user_id.0.clone()),
                    CosmosParam::new(
                        String::from("@ids"),
                        list.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
                    ),
                ],
            ),
        ))
        .await
        .map_err(Error::from)
}

fn format_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_owned(),
        Value::Number(n) => n.to_string(),
        Value::Null => Value::Null.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(a) => a.iter().map(format_value).collect::<Vec<_>>().join(", "),
        _ => todo!(),
    }
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
        query: String::from("SELECT name, user_score FROM c"),
    };
    create_list_doc(&state.client, list.clone(), false).await?;
    Ok((StatusCode::CREATED, Json(list)))
}

async fn update_list(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(mut list): Json<List>,
) -> Result<StatusCode, Response> {
    let user = require_user(auth)?;
    let user_id = UserId(user.user_id);
    let user_id = &user_id;
    let client = &state.client;
    let current_list = source::get_list(client, user_id, &id).await?;
    // Avoid updating sources if they haven't changed
    // TODO: we should also check the snapshot ID
    if current_list
        .sources
        .iter()
        .map(|s| &s.source_type)
        .ne(list.sources.iter().map(|s| &s.source_type))
    {
        list.items.clear();
        let sources = list.sources;
        list.sources = Vec::with_capacity(sources.len());
        for (source, items) in futures::stream::iter(
            sources
                .into_iter()
                .map(|source| source::get_source_and_items(&state.client, user_id, source)),
        )
        .buffered(5)
        .try_collect::<Vec<_>>()
        .await?
        {
            list.sources.push(source);
            list.items.extend(items);
        }
    }
    if let Ok((Some("spotify"), Some(external_id))) = list.get_unique_source() {
        list.iframe = Some(format!(
            "https://open.spotify.com/embed/playlist/{}?utm_source=generator",
            external_id.id
        ));
    }
    update_list_doc(&state.client, user_id, list).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_list_doc(
    client: &CosmosSessionClient,
    user_id: &UserId,
    list: List,
) -> Result<(), Error> {
    client
        .write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
            collection_name: "lists",
            document_name: list.id.clone(),
            partition_key: user_id.0.clone(),
            document: list,
        }))
        .await
        .map_err(Error::from)?;
    Ok(())
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
        .client
        .write_document(DocumentWriter::<List>::Delete(DeleteDocumentBuilder {
            collection_name: "lists",
            document_name: id,
            partition_key: user_id.0,
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

    let (query, _) = query::rewrite_query(query, &user_id)?;
    let values: Vec<Map<String, Value>> = state
        .client
        .query_documents(QueryDocumentsBuilder::new(
            "items",
            CosmosQuery::new(query.to_string()),
        ))
        .await
        .map_err(|e| {
            eprintln!("{}: {:?}", query, e);
            e
        })?;
    Ok(Json(values))
}

async fn handle_action(
    State(state): State<Arc<AppState>>,
    Query(mut params): Query<HashMap<String, String>>,
    auth: AuthContext,
    body: Bytes,
) -> Result<StatusCode, Response> {
    let user = auth.current_user.clone();
    let user_id = get_user_or_demo_user(auth);
    match params.get("action").map(String::as_ref) {
        Some("update") => {
            if let (Some(id), Some(win), Some(lose)) =
                (params.get("list"), params.get("win"), params.get("lose"))
            {
                return Ok(handle_stats_update(state, user_id, id, win, lose).await?);
            }
        }
        Some("push") => {
            if let Some(id) = params.get("list") {
                let Some(mut user) = user else {
                    return Err(StatusCode::UNAUTHORIZED.into_response());
                };
                return Ok(push_list(state, &mut user, id).await?);
            }
        }
        Some("import") => {
            if let (Some(source), Some(id)) = (params.remove("source"), params.remove("id")) {
                return Ok(import_list(state, user_id, &source, id, false).await?);
            }
        }
        Some("updateItems") => {
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
    let client = &state.client;
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
            collection_name: "lists",
            document_name: id.to_owned(),
            partition_key: user_id.0.clone(),
            document: list,
        })),
        client.write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
            collection_name: "items",
            document_name: win_item.id.clone(),
            partition_key: user_id.0.clone(),
            document: win_item,
        })),
        client.write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
            collection_name: "items",
            document_name: lose_item.id.clone(),
            partition_key: user_id.0.clone(),
            document: lose_item,
        })),
    )
    .await?;
    Ok(StatusCode::OK)
}

async fn push_list(state: Arc<AppState>, user: &mut User, id: &str) -> Result<StatusCode, Error> {
    let user_id = UserId(user.user_id.clone());
    let mut list = source::get_list(&state.client, &user_id, id).await?;
    // TODO: create new playlist if one doesn't exist
    let (_, external_id) = list.get_unique_source()?;
    let access_token = spotify::get_access_token(&state.client, user).await?;
    let external_id = if let Some(external_id) = external_id {
        spotify::update_playlist(access_token, &external_id.id, &list.name).await?;
        external_id.id.clone()
    } else {
        let mut playlist = spotify::create_playlist(access_token, &user_id, &list.name).await?;
        let id = Id {
            id: playlist.id,
            raw_id: playlist.external_urls.remove("spotify").unwrap(),
        };
        list.mode = ListMode::User(Some(id.clone()));
        update_list_doc(&state.client, &user_id, list.clone()).await?;
        id.id
    };
    let ids: Vec<_> = get_list_query_impl(&state.client, &user_id, list)
        .await?
        .items
        .into_iter()
        .map(|i| i.metadata.unwrap().id)
        .collect();
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
            import_spotify(&state.client, &user_id, source, id, favorite).await?
        }
        _ => todo!(),
    };
    Ok(StatusCode::CREATED)
}

pub async fn import_spotify(
    client: &CosmosSessionClient,
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
    client: &CosmosSessionClient,
    user_id: &UserId,
    id: &str,
) -> Result<Item, Error> {
    if let Some(item) = client
        .get_document(GetDocumentBuilder::new(
            "items",
            id.to_owned(),
            user_id.0.clone(),
        ))
        .await?
    {
        Ok(item)
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
    client: &CosmosSessionClient,
    list: List,
    is_upsert: bool,
) -> Result<(), Error> {
    client
        .write_document(DocumentWriter::Create(CreateDocumentBuilder {
            collection_name: "lists",
            document: list,
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
    futures::stream::iter(updates.into_iter().map(|(id, update)| async {
        let mut item = get_item_doc(&state.client, user_id, &id).await?;
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
            .client
            .write_document(DocumentWriter::Replace(ReplaceDocumentBuilder {
                collection_name: "items",
                document_name: id,
                partition_key: user_id.0.clone(),
                document: item,
            }))
            .await
            .map_err(Error::from)
    }))
    .buffered(5)
    .try_collect::<()>()
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
    let ids: Vec<_> = params["ids"].split(',').map(ToOwned::to_owned).collect();
    let state = &state;
    let user_id = &user_id;
    futures::stream::iter(ids.into_iter().map(|id| async move {
        match state
            .client
            .write_document(DocumentWriter::<Item>::Delete(DeleteDocumentBuilder {
                collection_name: "items",
                document_name: id.clone(),
                partition_key: user_id.0.clone(),
            }))
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                if let azure_core::StatusCode::NotFound = e.as_http_error().unwrap().status() {
                    Ok(())
                } else {
                    Err(Error::from(e))
                }
            }
        }
    }))
    .buffered(5)
    .try_collect::<()>()
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
    let access_token = spotify::get_access_token(&state.client, &mut user).await?;
    Ok(Json(
        spotify::get_recent_tracks(&state.client, &user_id, access_token).await?,
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
    let access_token = spotify::get_access_token(&state.client, &mut user).await?;
    Ok(Json(spotify::get_playlists(access_token).await?))
}

struct AppState {
    client: CosmosSessionClient,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.
    let master_key =
        std::env::var("COSMOS_MASTER_KEY").expect("Set env variable COSMOS_MASTER_KEY first!");
    let account = std::env::var("COSMOS_ACCOUNT").expect("Set env variable COSMOS_ACCOUNT first!");
    let authorization_token =
        AuthorizationToken::primary_from_base64(&master_key).expect("cosmos config");
    let db = CosmosClient::new(account, authorization_token).database_client("topbops");
    let shared_state = Arc::new(AppState {
        client: CosmosSessionClient::new(db.clone(), Arc::new(RwLock::new(None))),
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
            &shared_state.client,
            List {
                id: String::from("3c16df67-582d-449a-9862-0540f516d6b5"),
                user_id: demo_user.clone(),
                mode: ListMode::View,
                name: String::from("Artists"),
                sources: Vec::new(),
                iframe: None,
                items: Vec::new(),
                favorite: true,
                query: String::from("SELECT artists, AVG(user_score) FROM c GROUP BY artists"),
            },
            true,
        )
        .await
        .unwrap();
        create_list_doc(
            &shared_state.client,
            List {
                id: String::from("4539f893-8471-4e23-b815-cd7c8b722016"),
                user_id: demo_user.clone(),
                mode: ListMode::View,
                name: String::from("Winners"),
                sources: Vec::new(),
                iframe: None,
                items: Vec::new(),
                favorite: true,
                query: String::from("SELECT name, user_score FROM c WHERE user_score >= 1500"),
            },
            true,
        )
        .await
        .unwrap();
        println!("Demo lists were created");
    }

    let secret = BASE64_STANDARD
        .decode(std::env::var("SECRET_KEY").expect("SECRET_KEY is missing"))
        .expect("SECRET_KEY is not base64");

    let session_store = CosmosStore { db };
    let session_layer = SessionLayer::new(session_store.clone(), &secret)
        .with_secure(false)
        .with_session_ttl(Some(Duration::from_secs(31536000)));

    let auth_layer = AuthLayer::new(session_store, &secret);

    let api_router = Router::new()
        .route("/lists", get(get_lists).post(create_list))
        .route(
            "/lists/:id",
            get(get_list).put(update_list).delete(delete_list),
        )
        .route("/lists/:id/items", get(get_list_items))
        .route("/lists/:id/query", get(get_list_query))
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

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
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
