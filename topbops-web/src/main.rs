#![feature(let_else)]
use azure_data_cosmos::prelude::{
    AuthorizationToken, CollectionClient, ConsistencyLevel, CosmosClient, CosmosEntity,
    DatabaseClient, GetDocumentResponse, Query,
};
use futures::{StreamExt, TryStreamExt};
use hyper::header::HeaderValue;
use hyper::http::response::Builder;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode, Uri};
use hyper_tls::HttpsConnector;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
#[cfg(feature = "dev")]
use tokio::fs::File;
#[cfg(feature = "dev")]
use tokio::io::AsyncReadExt;
use topbops::{ItemQuery, List, ListMode, Lists};
use topbops_web::{Error, Item, Token, UserId};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
struct User {
    id: String,
    user_id: String,
    auth: String,
    access_token: String,
    refresh_token: String,
}

impl CosmosEntity for User {
    type Entity = String;

    fn partition_key(&self) -> Self::Entity {
        self.user_id.clone()
    }
}

const DEMO_USER: &str = "demo";

async fn handle(
    db: CosmosClient,
    req: Request<Body>,
    session: Arc<SessionClient>,
) -> Result<Response<Body>, Infallible> {
    Ok(match route(db, req, session).await {
        Err(e) => {
            eprintln!("server error: {:?}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("empty response builder should work")
        }
        Ok(resp) => resp,
    })
}

async fn route(
    db: CosmosClient,
    req: Request<Body>,
    session: Arc<SessionClient>,
) -> Result<Response<Body>, Error> {
    let db = db.database_client("topbops");
    eprintln!("{}", req.uri().path());
    if let Some(path) = req.uri().path().strip_prefix("/api/") {
        let path: Vec<_> = path.split('/').collect();
        if req.method() == Method::OPTIONS {
            return get_response_builder()
                .header(
                    "Access-Control-Allow-Headers",
                    HeaderValue::from_static("Authorization"),
                )
                .header(
                    "Access-Control-Allow-Methods",
                    HeaderValue::from_static("GET,POST,DELETE"),
                )
                .status(StatusCode::OK)
                .body(Body::empty())
                .map_err(Error::from);
        }
        // TODO: fix rerender on logout
        if let (["logout"], &Method::GET) = (&path[..], req.method()) {
            return get_response_builder()
                .header(
                    "Access-Control-Allow-Headers",
                    HeaderValue::from_static("Authorization"),
                )
                .header("Set-Cookie", "session=; Max-Age=0; Path=/; HttpOnly")
                .header("Set-Cookie", "user=; Max-Age=0; Path=/")
                .header("Location", "/")
                .status(StatusCode::FOUND)
                .body(Body::empty())
                .map_err(Error::from);
        }
        let cookies: HashMap<_, _> = req.headers().get("Cookie").map_or_else(HashMap::new, |c| {
            c.to_str()
                .expect("cookie to be ASCII")
                .split(';')
                .filter_map(|c| c.split_once('='))
                .collect()
        });
        let auth = if let Some(auth) = cookies.get("session") {
            auth
        } else {
            let query: HashMap<_, _> = req.uri().query().map_or_else(HashMap::new, |q| {
                q.split('&').filter_map(|p| p.split_once('=')).collect()
            });
            if let Some(code) = query.get("code") {
                code
            } else {
                DEMO_USER
            }
        };
        if auth == DEMO_USER {
            let user_id = UserId(String::from(DEMO_USER));
            match (&path[..], req.method()) {
                (["lists"], &Method::GET) => get_lists(db, session, user_id).await,
                (["lists", id], &Method::GET) => get_list(db, session, user_id, id).await,
                (["lists", id, "items"], &Method::GET) => {
                    get_list_items(db, session, user_id, id).await
                }
                (["items"], &Method::GET) => {
                    find_items(db, session, user_id, req.uri().query()).await
                }
                ([""], &Method::POST) => handle_action(db, session, user_id, req).await,
                (_, _) => get_response_builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body(Body::empty())
                    .map_err(Error::from),
            }
        } else if let Ok((user_id, access_token)) = login(db.clone(), &session, auth, {
            let host = req.headers()["Host"].to_str().expect("Host to be ASCII");
            #[cfg(feature = "dev")]
            {
                &format!("http://{}{}", host, req.uri().path())
            }
            #[cfg(not(feature = "dev"))]
            {
                &format!("https://{}{}", host, req.uri().path())
            }
        })
        .await
        {
            match (&path[..], req.method()) {
                (["login"], &Method::GET) => get_response_builder()
                    .header(
                        "Access-Control-Allow-Headers",
                        HeaderValue::from_static("Authorization"),
                    )
                    .header("Location", "/")
                    .header(
                        "Set-Cookie",
                        &format!("session={}; Max-Age=31536000; Path=/; HttpOnly", auth),
                    )
                    .header(
                        "Set-Cookie",
                        &format!("user={}; Max-Age=31536000; Path=/", user_id.0),
                    )
                    .status(StatusCode::FOUND)
                    .body(Body::empty())
                    .map_err(Error::from),
                (["lists"], &Method::GET) => get_lists(db, session, user_id).await,
                (["lists", id], &Method::GET) => get_list(db, session, user_id, id).await,
                (["lists", id, "items"], &Method::GET) => {
                    get_list_items(db, session, user_id, id).await
                }
                (["items"], &Method::GET) => {
                    find_items(db, session, user_id, req.uri().query()).await
                }
                ([""], &Method::POST) => handle_action(db, session, user_id, req).await,
                (_, _) => get_response_builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body(Body::empty())
                    .map_err(Error::from),
            }
        } else {
            unauthorized()
        }
    } else {
        #[cfg(feature = "dev")]
        if let Some((file, mime)) = match req.uri().path() {
            "/topbops_wasm.js" => Some((
                File::open("../topbops-wasm/pkg/topbops_wasm.js"),
                "application/javascript",
            )),
            "/topbops_wasm_bg.wasm" => Some((
                File::open("../topbops-wasm/pkg/topbops_wasm_bg.wasm"),
                "application/wasm",
            )),
            _ => Some((File::open("../topbops-wasm/www/index.html"), "text/html")),
        } {
            let mut contents = Vec::new();
            file.await?.read_to_end(&mut contents).await?;
            return get_response_builder()
                .header("Content-Type", HeaderValue::from_static(mime))
                .status(StatusCode::OK)
                .body(Body::from(contents))
                .map_err(Error::from);
        }
        get_response_builder()
            .header(
                "Access-Control-Allow-Headers",
                HeaderValue::from_static("Authorization"),
            )
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .map_err(Error::from)
    }
}

async fn login(
    db: DatabaseClient,
    session: &Arc<SessionClient>,
    auth: &str,
    origin: &str,
) -> Result<(UserId, String), Error> {
    let db = db.collection_client("users");
    let query = Query::new(format!("SELECT * FROM c WHERE c.auth = \"{}\"", auth));
    // TODO: debug why session token isn't working here
    //let session_copy = session.session.read().unwrap().clone();
    let (mut resp, session) = /*if let Some(session) = session_copy {
        println!("{:?}", session);
        (
            db.query_documents()
                .query_cross_partition(true)
                .parallelize_cross_partition_query(true)
                .consistency_level(session.clone())
                .execute(&query)
                .await?,
            session,
        )
    } else */{
        let resp = db
            .query_documents(query)
            .query_cross_partition(true)
            .parallelize_cross_partition_query(true)
            .into_stream::<User>()
            .next()
            .await
            .expect("response from database")?;
        let token = ConsistencyLevel::Session(resp.session_token.clone());
        *session.session.write().unwrap() = Some(token.clone());
        (resp, token)
    };
    if let Some((user, _)) = resp.results.pop() {
        return Ok((UserId(user.user_id), user.access_token));
    }
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
    let spotify_user: topbops_web::spotify::User = serde_json::from_slice(&got)?;

    let user = User {
        id: Uuid::new_v4().to_hyphenated().to_string(),
        user_id: spotify_user.id.clone(),
        auth: auth.to_owned(),
        access_token: token.access_token.clone(),
        refresh_token: token
            .refresh_token
            .expect("Spotify should return refresh token"),
    };
    db.create_document(user)
        .consistency_level(session)
        .into_future()
        .await?;
    Ok((UserId(spotify_user.id), token.access_token))
}

async fn get_lists(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
) -> Result<Response<Body>, Error> {
    let db = db.collection_client("lists");
    let query = format!("SELECT * FROM c WHERE c.user_id = \"{}\"", user_id.0);
    let lists = Lists {
        lists: session.query_documents(db, query).await?,
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&lists)?))
        .map_err(Error::from)
}

async fn get_list(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
    id: &str,
) -> Result<Response<Body>, Error> {
    let list = get_list_doc(&db, &session, &user_id, id).await?;
    get_response_builder()
        .body(Body::from(serde_json::to_string(&list)?))
        .map_err(Error::from)
}

async fn get_list_items(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
    id: &str,
) -> Result<Response<Body>, Error> {
    let list = get_list_doc(&db, &session, &user_id, id).await?;

    let mut map = HashMap::new();
    let query = if let ListMode::User = list.mode {
        list.query
    } else {
        // TODO: update AST directly
        let mut query = format!(
            "{} WHERE c.id IN ({})",
            list.query,
            list.items
                .iter()
                .map(|i| format!("\"{}\"", i.id))
                .collect::<Vec<_>>()
                .join(",")
        );
        let i = query.find("FROM").unwrap();
        query.insert_str(i - 1, ", id ");
        // TODO: need a first class way to get rating
        query.insert_str(i - 1, ", rating ");
        for i in list.items {
            map.insert(i.id.clone(), i);
        }
        query
    };
    let db = db.collection_client("items");
    let (query, fields) = topbops_web::query::rewrite_query(&query, &user_id).unwrap();
    let values: Vec<Map<String, Value>> = session.query_documents(db, query.to_string()).await?;
    let response = ItemQuery {
        fields,
        items: values
            .iter()
            .map(|r| {
                let mut iter = r.values();
                let metadata = if map.is_empty() {
                    None
                } else {
                    Some(map[iter.next_back().unwrap().as_str().unwrap()].clone())
                };
                topbops::Item {
                    values: iter
                        .map(|v| match v {
                            Value::String(s) => s.to_owned(),
                            Value::Number(n) => n.to_string(),
                            Value::Null => Value::Null.to_string(),
                            _ => todo!(),
                        })
                        .collect(),
                    metadata,
                }
            })
            .collect(),
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&response)?))
        .map_err(Error::from)
}

async fn get_list_doc(
    db: &DatabaseClient,
    _: &Arc<SessionClient>,
    user_id: &UserId,
    id: &str,
) -> Result<List, Error> {
    let client = db
        .clone()
        .collection_client("lists")
        .document_client(id, &user_id.0)?;
    if let GetDocumentResponse::Found(list) = client.get_document::<List>().into_future().await? {
        Ok(list.document.document)
    } else {
        todo!()
    }
}

async fn find_items(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
    query: Option<&str>,
) -> Result<Response<Body>, Error> {
    let Some(query) = query else {
        return get_response_builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .map_err(Error::from);
    };
    let response = match _find_items(db, session, user_id, query).await {
        Ok(query) => query,
        Err(Error::SqlError(error)) => {
            return get_response_builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(error))
                .map_err(Error::from);
        }
        Err(_) => {
            return get_response_builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Internal Server Error"))
                .map_err(Error::from);
        }
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&response)?))
        .map_err(Error::from)
}

async fn _find_items(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
    query: &str,
) -> Result<ItemQuery, Error> {
    let [(Cow::Borrowed("q"), Cow::Borrowed("search")), (Cow::Borrowed("query"), query)] = &url::form_urlencoded::parse(query.as_bytes())
        .collect::<Vec<_>>()[..] else { return Err("invalid finder".into()); };

    let db = db.collection_client("items");
    let (query, fields) = topbops_web::query::rewrite_query(query, &user_id)?;
    let values: Vec<Map<String, Value>> = session
        .query_documents(db, query.to_string())
        .await
        .map_err(|e| {
            eprintln!("{}: {:?}", query, e);
            e
        })?;
    Ok(ItemQuery {
        fields,
        items: values
            .iter()
            .map(|r| topbops::Item {
                values: r
                    .values()
                    .map(|v| match v {
                        Value::String(s) => s.to_owned(),
                        Value::Number(n) => n.to_string(),
                        Value::Null => Value::Null.to_string(),
                        _ => todo!(),
                    })
                    .collect(),
                metadata: None,
            })
            .collect(),
    })
}

struct SessionClient {
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
}

impl SessionClient {
    async fn query_documents<T: DeserializeOwned + Send + Sync>(
        &self,
        db: CollectionClient,
        query: String,
    ) -> Result<Vec<T>, Error> {
        let session_copy = self.session.read().unwrap().clone();
        let (stream, results) = if let Some(session) = session_copy {
            println!("{:?}", session);
            let mut stream = db
                .query_documents(Query::new(query))
                .consistency_level(session)
                .into_stream();
            let resp = stream.try_next().await?.map(|r| r.results);
            (stream, resp)
        } else {
            let mut stream = db.query_documents(Query::new(query)).into_stream();
            let resp = stream.try_next().await?.map(|r| {
                *self.session.write().unwrap() = Some(ConsistencyLevel::Session(r.session_token));
                r.results
            });
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
}

async fn handle_action(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
    req: Request<Body>,
) -> Result<Response<Body>, Error> {
    let query = req.uri().query();
    if let Some(query) = query.and_then(|q| {
        q.split('&')
            .map(|s| s.split_once('='))
            .collect::<Option<Vec<(&str, &str)>>>()
    }) {
        match query[..] {
            [("action", "update"), ("list", id), ("win", win), ("lose", lose)] => {
                return handle_stats_update(db, session, user_id, id, win, lose).await;
            }
            [("action", "import"), ("id", id)] => {
                return import_list(db, session, user_id, id).await;
            }
            [("action", "updateItems")] => {
                return update_items(db, session, user_id, req).await;
            }
            _ => {}
        }
    }
    get_response_builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Body::empty())
        .map_err(Error::from)
}

async fn handle_stats_update(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
    id: &str,
    win: &str,
    lose: &str,
) -> Result<Response<Body>, Error> {
    let list_client = db
        .clone()
        .collection_client("lists")
        .document_client(id, &user_id.0)?;
    let client = db.clone().collection_client("items");
    let (Ok(list_response), Ok(mut win_item), Ok(mut lose_item)) = futures::future::join3(
        list_client.get_document::<List>().into_future(),
        get_item_doc(client.clone(), &session, user_id.clone(), win),
        get_item_doc(client.clone(), &session, user_id.clone(), lose),
    ).await else {
        return get_response_builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .map_err(Error::from);
    };

    let mut list = if let GetDocumentResponse::Found(list) = list_response {
        list.document.document
    } else {
        todo!()
    };
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

    let win_client = client
        .clone()
        .document_client(win_item.id.clone(), &win_item.user_id)?;
    let lose_client = client.document_client(lose_item.id.clone(), &lose_item.user_id)?;
    let session_copy = session
        .session
        .read()
        .unwrap()
        .clone()
        .expect("session should be set by get_item_docs");
    futures::future::try_join3(
        list_client
            .replace_document(list)
            .consistency_level(session_copy.clone())
            .into_future(),
        win_client
            .replace_document(win_item)
            .consistency_level(session_copy.clone())
            .into_future(),
        lose_client
            .replace_document(lose_item)
            .consistency_level(session_copy)
            .into_future(),
    )
    .await?;
    get_response_builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .map_err(Error::from)
}

async fn import_list(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
    id: &str,
) -> Result<Response<Body>, Error> {
    let (list, items) = match id.split_once(':') {
        Some(("spotify", id)) => topbops_web::spotify::import(&user_id, id).await?,
        _ => todo!(),
    };
    create_external_list(db, session, list, items, user_id.0 == DEMO_USER).await
}

async fn get_item_doc(
    client: CollectionClient,
    session: &Arc<SessionClient>,
    user_id: UserId,
    id: &str,
) -> Result<Item, Error> {
    let session_copy = session.session.read().unwrap().clone().unwrap();
    if let GetDocumentResponse::Found(item) = client
        .document_client(id, &user_id.0)?
        .get_document::<Item>()
        .consistency_level(session_copy)
        .into_future()
        .await?
    {
        Ok(item.document.document)
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

async fn create_user_list(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    list: List,
    is_upsert: bool,
) -> Result<(), Error> {
    let list_client = db.clone().collection_client("lists");
    let session_copy = session.session.read().unwrap().clone();
    if let Some(session) = session_copy {
        list_client
            .create_document(list)
            .is_upsert(true)
            .consistency_level(session.clone())
            .into_future()
            .await?;
    } else {
        let resp = list_client
            .create_document(list)
            .is_upsert(true)
            .into_future()
            .await
            .unwrap();
        *session.session.write().unwrap() = Some(ConsistencyLevel::Session(resp.session_token));
    };
    Ok(())
}

async fn create_external_list(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    list: List,
    items: Vec<Item>,
    // Used to reset demo user data
    is_upsert: bool,
) -> Result<Response<Body>, Error> {
    let list_client = db.clone().collection_client("lists");
    let session_copy = session.session.read().unwrap().clone();
    let session = if let Some(session) = session_copy {
        list_client
            .create_document(list)
            .is_upsert(true)
            .consistency_level(session.clone())
            .into_future()
            .await?;
        session
    } else {
        let resp = list_client
            .create_document(list)
            .is_upsert(true)
            .into_future()
            .await
            .unwrap();
        let session_copy = ConsistencyLevel::Session(resp.session_token);
        *session.session.write().unwrap() = Some(session_copy.clone());
        session_copy
    };
    let items_client = db.clone().collection_client("items");
    futures::stream::iter(items.into_iter().map(move |item| {
        let items_client = items_client.clone();
        let session = session.clone();
        async move {
            items_client
                .create_document(item)
                .is_upsert(is_upsert)
                .consistency_level(session)
                .into_future()
                .await
                .map(|_| ())
                .or_else(|e| {
                    if let azure_core::StatusCode::Conflict = e.as_http_error().unwrap().status() {
                        return Ok(());
                    }
                    Err(e)
                })
        }
    }))
    .buffered(5)
    .try_collect::<()>()
    .await?;
    get_response_builder()
        .status(StatusCode::CREATED)
        .body(Body::empty())
        .map_err(Error::from)
}

async fn update_items(
    db: DatabaseClient,
    session: Arc<SessionClient>,
    user_id: UserId,
    req: Request<Body>,
) -> Result<Response<Body>, Error> {
    let body = &hyper::body::to_bytes(req.into_body()).await?;
    let updates: HashMap<String, HashMap<String, Value>> = serde_json::from_slice(body)?;
    let client = db.collection_client("items");
    let client = &client;
    let session = &session;
    let user_id = &user_id;
    futures::stream::iter(updates.into_iter().map(move |(id, update)| async {
        let mut item = get_item_doc(client.clone(), session, user_id.clone(), &id).await?;
        for (k, v) in update {
            match k.as_str() {
                "rating" => {
                    item.rating = serde_json::from_value(v)?;
                }
                _ => {}
            }
        }
        let session_copy = session.session.read().unwrap().clone().unwrap();
        client
            .clone()
            .document_client(id, &user_id.0)?
            .replace_document::<Item>(item)
            .consistency_level(session_copy)
            .into_future()
            .await?;
        Ok::<_, Error>(())
    }))
    .buffered(5)
    .try_collect::<()>()
    .await?;
    get_response_builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map_err(Error::from)
}

#[tokio::main]
async fn main() {
    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.
    let master_key =
        std::env::var("COSMOS_MASTER_KEY").expect("Set env variable COSMOS_MASTER_KEY first!");
    let account = std::env::var("COSMOS_ACCOUNT").expect("Set env variable COSMOS_ACCOUNT first!");
    let authorization_token =
        AuthorizationToken::primary_from_base64(&master_key).expect("cosmos config");
    let client = CosmosClient::new(account.clone(), authorization_token);
    let session = Arc::new(SessionClient {
        session: Arc::new(RwLock::new(None)),
    });

    // Reset demo user data during startup in production
    if cfg!(not(feature = "dev")) {
        let demo_user = String::from(DEMO_USER);
        import_list(
            client.clone().database_client("topbops"),
            Arc::clone(&session),
            UserId(demo_user.clone()),
            "spotify:playlist:5MztFbRbMpyxbVYuOSfQV9",
        )
        .await
        .unwrap();
        // Generate IDs using random but constant UUIDs
        create_user_list(
            client.clone().database_client("topbops"),
            Arc::clone(&session),
            List {
                id: String::from("4539f893-8471-4e23-b815-cd7c8b722016"),
                user_id: demo_user.clone(),
                name: String::from("Winners"),
                iframe: None,
                items: Vec::new(),
                mode: ListMode::User,
                query: String::from("SELECT name, user_score FROM tracks WHERE user_score >= 1500"),
            },
            true,
        )
        .await
        .unwrap();
        create_user_list(
            client.clone().database_client("topbops"),
            Arc::clone(&session),
            List {
                id: String::from("3c16df67-582d-449a-9862-0540f516d6b5"),
                user_id: demo_user.clone(),
                name: String::from("Artists"),
                iframe: None,
                items: Vec::new(),
                mode: ListMode::User,
                query: String::from("SELECT artists, AVG(user_score) FROM tracks GROUP BY artists"),
            },
            true,
        )
        .await
        .unwrap();
        create_user_list(
            client.clone().database_client("topbops"),
            Arc::clone(&session),
            List {
                id: String::from("a425903e-d12f-43eb-8a53-dbfad3325fd5"),
                user_id: demo_user,
                name: String::from("Albums"),
                iframe: None,
                items: Vec::new(),
                mode: ListMode::User,
                query: String::from("SELECT album, AVG(user_score) FROM tracks GROUP BY album"),
            },
            true,
        )
        .await
        .unwrap();
        println!("Demo lists were created");
    }

    let make_svc = make_service_fn(move |_conn| {
        let client_ref = client.clone();
        let session = Arc::clone(&session);
        async {
            // service_fn converts our function into a `Service`
            Ok::<_, Infallible>(service_fn(move |r| {
                handle(client_ref.clone(), r, Arc::clone(&session))
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

fn unauthorized() -> Result<Response<Body>, Error> {
    get_response_builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(Body::empty())
        .map_err(Error::from)
}

fn get_response_builder() -> Builder {
    Response::builder().header("Access-Control-Allow-Origin", HeaderValue::from_static("*"))
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
