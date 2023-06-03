use axum::{
    body::Bytes,
    extract::{Host, OriginalUri, Path, Query, State},
    http::header,
    response::{IntoResponse, Json, Redirect, Response},
    routing::{get, post},
    Router,
};
use axum_login::{axum_sessions::SessionLayer, AuthLayer};
use azure_data_cosmos::prelude::{AuthorizationToken, CosmosClient, Param, Query as CosmosQuery};
use base64::prelude::{Engine, BASE64_STANDARD};
use futures::{StreamExt, TryStreamExt};
use hyper::{Body, Client, Method, Request, StatusCode, Uri};
use hyper_tls::HttpsConnector;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use topbops::{ItemQuery, List, ListMode, Lists, Source, SourceType};
use topbops_web::{
    cosmos::SessionClient,
    user::{CosmosStore, User},
};
use topbops_web::{query, source, Error, Item, Token, UserId};
#[cfg(feature = "dev")]
use tower_http::services::ServeFile;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

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
    let user = login(&state, &params["code"], &origin).await?;
    auth.login(&user).await.unwrap();
    Ok((
        [(
            header::SET_COOKIE,
            format!("user={}; Max-Age=31536000; Path=/", user.user_id),
        )],
        Redirect::to("/"),
    ))
}

// TODO: fix rerender on logout
async fn logout_handler(mut auth: AuthContext) -> impl IntoResponse {
    auth.logout().await;
    (
        // TODO: also clear cookie if there was an invalid session
        [(header::SET_COOKIE, "user=; Max-Age=0; Path=/")],
        Redirect::to("/"),
    )
}

async fn login(state: &Arc<AppState>, auth: &str, origin: &str) -> Result<User, Error> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = "https://accounts.spotify.com/api/token".parse().unwrap();
    let resp = client
        .request(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(
                    "Authorization",
                    &format!(
                        "Basic {}",
                        std::env::var("SPOTIFY_TOKEN").expect("SPOTIFY_TOKEN is missing")
                    ),
                )
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "grant_type=authorization_code&code={}&redirect_uri={}",
                    auth, origin
                )))?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let token: Token = serde_json::from_slice(&got)?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = "https://api.spotify.com/v1/me".parse().unwrap();
    let resp = client
        .request(
            Request::builder()
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token.access_token))
                .body(Body::empty())?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    let spotify_user: source::spotify::User = serde_json::from_slice(&got)?;

    let query = CosmosQuery::with_params(
        String::from("SELECT c.id FROM c WHERE c.user_id = @user_id"),
        [Param::new(
            String::from("@user_id"),
            spotify_user.id.clone(),
        )],
    );
    let mut results: Vec<HashMap<String, String>> = state
        .client
        .query_documents(move |db| {
            db.collection_client("users")
                .query_documents(query)
                .query_cross_partition(true)
                .parallelize_cross_partition_query(true)
        })
        .await?;
    let id = if let Some(mut map) = results.pop() {
        map.remove("id").expect("id should be returned by DB")
    } else {
        Uuid::new_v4().to_hyphenated().to_string()
    };

    let user = User {
        id,
        user_id: spotify_user.id,
        access_token: token.access_token.clone(),
        refresh_token: token
            .refresh_token
            .expect("Spotify should return refresh token"),
    };
    state
        .client
        .write_document(|db| {
            Ok(db
                .collection_client("users")
                .create_document(user.clone())
                .is_upsert(true))
        })
        .await?;
    Ok(user)
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
            .query_documents(|db| {
                db.collection_client("lists")
                    .query_documents(CosmosQuery::with_params(
                        String::from(query),
                        [Param::new(String::from("@user_id"), user_id.0)],
                    ))
            })
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
    let list = get_list_doc(&state.client, &user_id, &id).await?;
    Ok(Json(list))
}

async fn get_list_items(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    auth: AuthContext,
) -> Result<Json<ItemQuery>, Response> {
    let user_id = get_user_or_demo_user(auth);
    let list = get_list_doc(&state.client, &user_id, &id).await?;
    if list.items.is_empty() {
        Ok(Json(ItemQuery {
            fields: Vec::new(),
            items: Vec::new(),
        }))
    } else {
        let (query, fields, map, ids) = query::rewrite_list_query(&list, &user_id)?;
        let items: HashMap<_, _> = state
            .client
            .query_documents(|db| {
                db.collection_client("items")
                    .query_documents(CosmosQuery::new(query.to_string()))
            })
            .await
            .map_err(Error::from)?
            .into_iter()
            .map(|r: Map<String, Value>| (r["id"].to_string(), r))
            .collect();
        Ok(Json(ItemQuery {
            fields,
            items: ids
                .into_iter()
                .map(|id| {
                    let mut iter = items[&id].values();
                    let metadata = if map.is_empty() {
                        None
                    } else {
                        Some(map[iter.next_back().unwrap().as_str().unwrap()].clone())
                    };
                    topbops::Item {
                        values: iter.map(format_value).collect(),
                        metadata,
                    }
                })
                .collect(),
        }))
    }
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

async fn get_list_doc(client: &SessionClient, user_id: &UserId, id: &str) -> Result<List, Error> {
    if let Some(list) = client
        .get_document(|db| {
            Ok(db
                .collection_client("lists")
                .document_client(id, &user_id.0)?
                .get_document())
        })
        .await?
    {
        Ok(list)
    } else {
        todo!()
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
        query: String::from("SELECT name, user_score FROM tracks"),
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
    let current_list = get_list_doc(&state.client, &user_id, &id).await?;
    if current_list.sources != list.sources {
        list.items.clear();
        for source in &mut list.sources {
            let (updated_source, items) = source::get_source_and_items(&user_id, source).await?;
            list.items.extend(topbops_web::convert_items(&items));
            create_items(&state.client, items, false).await?;
            *source = updated_source;
        }
    }
    if let Ok((Some("spotify"), external_id)) = get_unique_source(&list) {
        list.iframe = Some(format!(
            "https://open.spotify.com/embed/playlist/{}?utm_source=generator",
            external_id
        ));
    }
    state
        .client
        .write_document(|db| {
            Ok(db
                .collection_client("lists")
                .document_client(id, &user_id.0)?
                .replace_document(list))
        })
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
    let Some(query) = params.get("query") else { return Err(Error::client_error("invalid finder").into()); };

    let (query, fields) = query::rewrite_query(query, &user_id)?;
    let values: Vec<Map<String, Value>> = state
        .client
        .query_documents(|db| {
            db.collection_client("items")
                .query_documents(CosmosQuery::new(query.to_string()))
        })
        .await
        .map_err(|e| {
            eprintln!("{}: {:?}", query, e);
            Error::from(e)
        })?;
    Ok(Json(ItemQuery {
        fields,
        items: values
            .iter()
            .map(|r| topbops::Item {
                values: r.values().map(format_value).collect(),
                metadata: None,
            })
            .collect(),
    }))
}

async fn handle_action(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
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
                let Some(mut user) = user else { return Err(StatusCode::UNAUTHORIZED.into_response()); };
                return Ok(push_list(state, &mut user, id).await?);
            }
        }
        Some("import") => {
            if let Some(id) = params.get("id") {
                return Ok(import_list(state, user_id, id, false).await?);
            }
        }
        Some("updateItems") => {
            return Ok(update_items(state, user_id, body).await?);
        }
        _ => {}
    }
    Err(StatusCode::BAD_REQUEST.into_response())
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
        get_list_doc(client, &user_id, id),
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
        client.write_document(|db| {
            Ok(db
                .collection_client("lists")
                .document_client(id, &user_id.0)?
                .replace_document(list))
        }),
        client.write_document(|db| {
            Ok(db
                .collection_client("items")
                .document_client(&win_item.id, &user_id.0)?
                .replace_document(win_item))
        }),
        client.write_document(|db| {
            Ok(db
                .collection_client("items")
                .document_client(&lose_item.id, &user_id.0)?
                .replace_document(lose_item))
        }),
    )
    .await?;
    Ok(StatusCode::OK)
}

async fn push_list(state: Arc<AppState>, user: &mut User, id: &str) -> Result<StatusCode, Error> {
    let list = get_list_doc(&state.client, &UserId(user.user_id.clone()), id).await?;
    // TODO: create new playlist if one doesn't exist
    let (_, external_id) = get_unique_source(&list)?;
    let ids: Vec<_> = list.items.into_iter().map(|i| i.id).collect();
    let query = String::from("SELECT VALUE c.id FROM c WHERE c.user_id = @user_id AND ARRAY_CONTAINS(@ids, c.id) AND c.hidden = true");
    let hidden: HashSet<_> = state
        .client
        .query_documents::<_, String>(|db| {
            db.collection_client("items")
                .query_documents(CosmosQuery::with_params(
                    query,
                    [
                        Param::new(String::from("@user_id"), user.user_id.clone()),
                        Param::new(String::from("@ids"), ids.clone()),
                    ],
                ))
        })
        .await?
        .into_iter()
        .collect();
    let ids: Vec<_> = ids.into_iter().filter(|id| !hidden.contains(id)).collect();
    let access_token = source::spotify::get_access_token(&state.client, user).await?;
    source::spotify::update_list(access_token, &external_id, &ids.join(",")).await?;
    Ok(StatusCode::OK)
}

fn get_unique_source(list: &List) -> Result<(Option<&str>, String), Error> {
    let ListMode::User(Some(external_id)) = &list.mode else {
        return Err(Error::client_error("Push is not supported for this list type"));
    };
    let mut iter = list.sources.iter().map(get_source_id);
    let source = if let Some(source) = iter.next() {
        if source.is_none() {
            return Err(Error::client_error("Push is not supported for the source"));
        }
        source
    } else {
        return Err(Error::client_error("List has no sources"));
    };
    for s in iter {
        if s != source {
            return Err(Error::client_error("List has multiple sources"));
        }
    }
    Ok((source, external_id.clone()))
}

fn get_source_id(source: &Source) -> Option<&str> {
    match source.source_type {
        SourceType::Spotify(_) => Some("spotify"),
        SourceType::Setlist(_) => Some("spotify"),
        _ => None,
    }
}

async fn import_list(
    state: Arc<AppState>,
    user_id: UserId,
    id: &str,
    favorite: bool,
) -> Result<StatusCode, Error> {
    let (mut list, items) = match id.split_once(':') {
        Some(("spotify", id)) => source::spotify::import(&user_id, id).await?,
        _ => todo!(),
    };
    list.favorite = favorite;
    create_external_list(&state.client, list, items, user_id.0 == DEMO_USER).await?;
    Ok(StatusCode::CREATED)
}

async fn get_item_doc(client: &SessionClient, user_id: &UserId, id: &str) -> Result<Item, Error> {
    if let Some(item) = client
        .get_document(|db| {
            Ok(db
                .collection_client("items")
                .document_client(id, &user_id.0)?
                .get_document())
        })
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

async fn create_list_doc(client: &SessionClient, list: List, is_upsert: bool) -> Result<(), Error> {
    client
        .write_document(|db| {
            Ok(db
                .collection_client("lists")
                .create_document(list)
                .is_upsert(is_upsert))
        })
        .await
        .map_err(Error::from)
}

// TODO: inline
async fn create_external_list(
    client: &SessionClient,
    list: List,
    items: Vec<Item>,
    // Used to reset demo user data
    is_upsert: bool,
) -> Result<(), Error> {
    create_list_doc(client, list, is_upsert).await?;
    create_items(client, items, is_upsert).await
}

async fn create_items(
    client: &SessionClient,
    items: Vec<Item>,
    is_upsert: bool,
) -> Result<(), Error> {
    futures::stream::iter(items.into_iter().map(|item| async move {
        match client
            .write_document(|db| {
                Ok(db
                    .collection_client("items")
                    .create_document(item)
                    .is_upsert(is_upsert))
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                if let azure_core::StatusCode::Conflict = e.as_http_error().unwrap().status() {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }))
    .buffered(5)
    .try_collect()
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
            .write_document(move |db| {
                Ok(db
                    .collection_client("items")
                    .document_client(id, &user_id.0)?
                    .replace_document(item))
            })
            .await
            .map_err(Error::from)
    }))
    .buffered(5)
    .try_collect::<()>()
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

struct AppState {
    client: SessionClient,
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
        client: SessionClient::new(db.clone(), Arc::new(RwLock::new(None))),
    });

    // Reset demo user data during startup in production
    if cfg!(not(feature = "dev")) {
        let demo_user = String::from(DEMO_USER);
        import_list(
            Arc::clone(&shared_state),
            UserId(demo_user.clone()),
            "spotify:playlist:5MztFbRbMpyxbVYuOSfQV9",
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
                query: String::from("SELECT artists, AVG(user_score) FROM tracks GROUP BY artists"),
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
                query: String::from("SELECT name, user_score FROM tracks WHERE user_score >= 1500"),
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
    let session_layer = SessionLayer::new(session_store.clone(), &secret).with_secure(false);

    let auth_layer = AuthLayer::new(session_store, &secret);

    let api_router = Router::new()
        .route("/lists", get(get_lists).post(create_list))
        .route("/lists/:id", get(get_list).put(update_list))
        .route("/lists/:id/items", get(get_list_items))
        .route("/items", get(find_items))
        .route("/", post(handle_action))
        .route("/login", get(login_handler))
        .route("/logout", get(logout_handler))
        .with_state(shared_state);

    let app = Router::new()
        .nest("/api/", api_router)
        .layer(auth_layer)
        .layer(session_layer)
        .layer(TraceLayer::new_for_http());
    #[cfg(feature = "dev")]
    let app = {
        app.route_service(
            "/topbops_wasm.js",
            ServeFile::new("../topbops-wasm/pkg/topbops_wasm.js"),
        )
        .route_service(
            "/topbops_wasm_bg.wasm",
            ServeFile::new("../topbops-wasm/pkg/topbops_wasm_bg.wasm"),
        )
        .fallback_service(ServeFile::new("../topbops-wasm/www/index.html"))
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
