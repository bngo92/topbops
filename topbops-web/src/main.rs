#![feature(async_closure, box_patterns, let_else)]
use azure_core::Context;
use azure_data_cosmos::prelude::{
    AuthorizationToken, CollectionClient, ConsistencyLevel, CosmosClient, CosmosEntity,
    CosmosOptions, CreateDocumentOptions, DatabaseClient, DeleteDocumentOptions,
    GetDocumentOptions, GetDocumentResponse, Query, ReplaceDocumentOptions,
};
use futures::{StreamExt, TryStreamExt};
use hyper::header::HeaderValue;
use hyper::http::response::Builder;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode, Uri};
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlparser::ast::{
    BinaryOperator, Expr, FunctionArg, FunctionArgExpr, Ident, Select, SelectItem, SetExpr,
    Statement, TableFactor,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
#[cfg(feature = "dev")]
use tokio::fs::File;
#[cfg(feature = "dev")]
use tokio::io::AsyncReadExt;
use topbops::{ItemMetadata, ItemQuery, List, ListMode, Lists};
use uuid::Uuid;

const ITEM_FIELDS: [&str; 3] = ["id", "name", "user_score"];

#[derive(Debug, Deserialize, Serialize)]
struct Token {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct User {
    id: String,
    user_id: String,
    auth: String,
    access_token: String,
    refresh_token: String,
}

impl<'a> CosmosEntity<'a> for User {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Item {
    pub id: String,
    pub user_id: String,
    pub r#type: String,
    pub name: String,
    pub iframe: Option<String>,
    pub rating: Option<i32>,
    pub user_score: i32,
    pub user_wins: i32,
    pub user_losses: i32,
    pub metadata: Map<String, Value>,
}

impl<'a> CosmosEntity<'a> for Item {
    type Entity = &'a str;

    fn partition_key(&'a self) -> Self::Entity {
        self.user_id.as_ref()
    }
}

const DEMO_USER: &str = "demo";

async fn handle(
    db: CosmosClient,
    req: Request<Body>,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
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
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
) -> Result<Response<Body>, Error> {
    let db = db.into_database_client("smashsort");
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
        let Some(auth) = req.headers().get("Authorization") else {
            return unauthorized()};
        let Some((_, auth)) = auth.to_str().expect("auth to be ASCII").split_once(' ') else {
            return get_response_builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .map_err(Error::from);
        };
        if auth == "demo" {
            let user_id = String::from(DEMO_USER);
            match (&path[..], req.method()) {
                (["lists"], &Method::GET) => get_lists(db, session, user_id).await,
                (["lists", id, "items"], &Method::GET) => {
                    get_list_items(db, session, user_id, id).await
                }
                (_, _) => get_response_builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body(Body::empty())
                    .map_err(Error::from),
            }
        } else if let Ok((user_id, access_token)) = login(db.clone(), &session, auth, {
            let uri: Uri = req.headers()["Referer"]
                .to_str()
                .expect("Referer to be ASCII")
                .parse()
                .expect("referer URI");
            &format!(
                "{}://{}",
                uri.scheme().expect("scheme"),
                uri.authority().expect("authority")
            )
        })
        .await
        {
            match (&path[..], req.method()) {
                (["login"], &Method::POST) => get_response_builder()
                    .header(
                        "Access-Control-Allow-Headers",
                        HeaderValue::from_static("Authorization"),
                    )
                    .status(StatusCode::OK)
                    .body(Body::empty())
                    .map_err(Error::from),
                (["lists"], &Method::GET) => get_lists(db, session, user_id).await,
                (["lists", id, "items"], &Method::GET) => {
                    get_list_items(db, session, user_id, id).await
                }
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
            "/" => Some((File::open("../topbops-wasm/www/index.html"), "text/html")),
            "/topbops_wasm.js" => Some((
                File::open("../topbops-wasm/pkg/topbops_wasm.js"),
                "application/javascript",
            )),
            "/topbops_wasm_bg.wasm" => Some((
                File::open("../topbops-wasm/pkg/topbops_wasm_bg.wasm"),
                "application/wasm",
            )),
            _ => None,
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
    session: &Arc<RwLock<Option<ConsistencyLevel>>>,
    auth: &str,
    origin: &str,
) -> Result<(String, String), Error> {
    let db = db.into_collection_client("users");
    let query = format!("SELECT * FROM c WHERE c.auth = \"{}\"", auth);
    let query = Query::new(&query);
    let session_copy = session.read().unwrap().clone();
    // TODO: debug why session token isn't working here
    let (resp, session) = /*if let Some(session) = session_copy {
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
            .query_documents()
            .query_cross_partition(true)
            .parallelize_cross_partition_query(true)
            .execute(&query)
            .await?;
        let token = ConsistencyLevel::Session(resp.session_token.clone());
        *session.write().unwrap() = Some(token.clone());
        (resp, token)
    };
    if let Some(user) = resp
        .into_documents()?
        .results
        .into_iter()
        .map(|r| -> User { r.result })
        .next()
    {
        return Ok((user.user_id, user.access_token));
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
    let user: topbops_web::User = serde_json::from_slice(&got)?;

    let user = User {
        id: Uuid::new_v4().to_hyphenated().to_string(),
        user_id: user.id,
        auth: auth.to_owned(),
        access_token: token.access_token.clone(),
        refresh_token: token
            .refresh_token
            .expect("Spotify should return refresh token"),
    };
    db.create_document(
        Context::new(),
        &user,
        CreateDocumentOptions::new().consistency_level(session),
    )
    .await?;
    Ok((user.user_id, user.access_token))
}

async fn get_lists(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
) -> Result<Response<Body>, Error> {
    let db = db.into_collection_client("lists");
    let query = format!("SELECT * FROM c WHERE c.user_id = \"{}\"", user_id);
    let query = Query::new(&query);
    let session_copy = session.read().unwrap().clone();
    let resp = if let Some(session) = session_copy {
        println!("{:?}", session);
        db.query_documents()
            .consistency_level(session)
            .execute(&query)
            .await?
    } else {
        let resp = db.query_documents().execute(&query).await?;
        *session.write().unwrap() = Some(ConsistencyLevel::Session(resp.session_token.clone()));
        resp
    };
    let lists = Lists {
        lists: resp
            .into_documents()?
            .results
            .into_iter()
            .map(|r| r.result)
            .collect(),
    };
    get_response_builder()
        .body(Body::from(serde_json::to_string(&lists)?))
        .map_err(Error::from)
}

async fn get_list_items(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
    id: &str,
) -> Result<Response<Body>, Error> {
    let client = db
        .clone()
        .into_collection_client("lists")
        .into_document_client(id, &user_id)?;
    let list = if let GetDocumentResponse::Found(list) = client
        .get_document::<List>(Context::new(), GetDocumentOptions::new())
        .await?
    {
        list.document.document
    } else {
        todo!()
    };

    let mut map = HashMap::new();
    let original_query = if let ListMode::User = list.mode {
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
        for i in list.items {
            map.insert(i.id.clone(), i);
        }
        query
    };
    let db = db.into_collection_client("items");
    let mut query = parse_select(&original_query);
    transform_query(&mut query, &user_id);
    let query = query.to_string();
    let session_copy = session.read().unwrap().clone();
    let resp = if let Some(session) = session_copy {
        println!("{:?}", session);
        db.query_documents()
            .consistency_level(session)
            .execute(&query)
            .await?
    } else {
        let resp = db.query_documents().execute(&Query::new(&query)).await?;
        *session.write().unwrap() = Some(ConsistencyLevel::Session(resp.session_token.clone()));
        resp
    };
    let values: Vec<Map<String, Value>> = resp.into_raw().results;
    let response = ItemQuery {
        fields: parse_select(&original_query)
            .projection
            .iter()
            .map(ToString::to_string)
            .collect(),
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

fn transform_query(query: &mut Select, user_id: &str) {
    let from = if let TableFactor::Table { name, .. } = &mut query.from[0].relation {
        std::mem::replace(&mut name.0[0].value, String::from("c"))
    } else {
        todo!();
    };
    let from = &from[..from.len() - 1];
    for expr in &mut query.projection {
        match expr {
            SelectItem::UnnamedExpr(Expr::Identifier(id)) => {
                *expr = SelectItem::UnnamedExpr(replace_identifier(id.clone()));
            }
            SelectItem::UnnamedExpr(Expr::Function(f)) => {
                if let Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(id)))) =
                    f.args.pop()
                {
                    f.args.push(FunctionArg::Unnamed(FunctionArgExpr::Expr(
                        replace_identifier(id),
                    )));
                } else {
                    todo!()
                }
            }
            _ => {
                todo!()
            }
        }
    }
    query.selection = Some(if let Some(mut selection) = query.selection.take() {
        let expr = &mut selection;
        let mut queue = VecDeque::new();
        queue.push_back(expr);
        while let Some(expr) = queue.pop_front() {
            match expr {
                Expr::BinaryOp { left, op: _, right } => {
                    queue.push_back(left);
                    queue.push_back(right);
                }
                Expr::Identifier(id) => {
                    *expr = replace_identifier(id.clone());
                }
                _ => {}
            }
        }
        Expr::BinaryOp {
            left: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::CompoundIdentifier(vec![
                    Ident::new("c"),
                    Ident::new("user_id"),
                ])),
                op: BinaryOperator::Eq,
                right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", user_id)))),
            }),
            op: BinaryOperator::And,
            right: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::BinaryOp {
                    left: Box::new(Expr::CompoundIdentifier(vec![
                        Ident::new("c"),
                        Ident::new("type"),
                    ])),
                    op: BinaryOperator::Eq,
                    right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", from)))),
                }),
                op: BinaryOperator::And,
                right: Box::new(selection),
            }),
        }
    } else {
        Expr::BinaryOp {
            left: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::CompoundIdentifier(vec![
                    Ident::new("c"),
                    Ident::new("user_id"),
                ])),
                op: BinaryOperator::Eq,
                right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", user_id)))),
            }),
            op: BinaryOperator::And,
            right: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::CompoundIdentifier(vec![
                    Ident::new("c"),
                    Ident::new("type"),
                ])),
                op: BinaryOperator::Eq,
                right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", from)))),
            }),
        }
    });
    for expr in &mut query.group_by {
        match expr {
            Expr::Identifier(id) => {
                *expr = replace_identifier(id.clone());
            }
            _ => todo!(),
        }
    }
}

fn replace_identifier(id: Ident) -> Expr {
    Expr::CompoundIdentifier(if ITEM_FIELDS.contains(&id.value.as_ref()) {
        vec![Ident::new("c"), id]
    } else {
        vec![Ident::new("c"), Ident::new("metadata"), id]
    })
}

fn parse_select(s: &str) -> Select {
    let dialect = GenericDialect {};
    let statement = Parser::parse_sql(&dialect, s).unwrap().pop();
    if let Some(Statement::Query(box sqlparser::ast::Query {
        body: SetExpr::Select(box s),
        ..
    })) = statement
    {
        s
    } else {
        todo!()
    }
}

async fn import_playlist(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    user_id: String,
    playlist_id: &str,
) -> Result<Response<Body>, Error> {
    let token = get_token().await?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!("https://api.spotify.com/v1/playlists/{}", playlist_id)
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
    let playlist: topbops_web::Playlist = serde_json::from_slice(&got)?;

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);
    let uri: Uri = format!(
        "https://api.spotify.com/v1/playlists/{}/tracks",
        playlist_id
    )
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
    let mut playlist_items: topbops_web::PlaylistItems = serde_json::from_slice(&got)?;
    let mut items: Vec<_> = playlist_items
        .items
        .into_iter()
        .map(|i| new_spotify_item(i.track, &user_id))
        .collect();
    while let Some(uri) = playlist_items.next {
        let uri: Uri = uri.parse().unwrap();
        let resp = client
            .request(
                Request::builder()
                    .uri(uri)
                    .header("Authorization", format!("Bearer {}", token.access_token))
                    .body(Body::empty())?,
            )
            .await?;
        let got = hyper::body::to_bytes(resp.into_body()).await?;
        playlist_items = serde_json::from_slice(&got)?;
        items.extend(
            playlist_items
                .items
                .into_iter()
                .map(|i| new_spotify_item(i.track, &user_id)),
        );
    }
    let list = List {
        id: playlist_id.to_owned(),
        user_id: user_id.clone(),
        name: playlist.name,
        items: items
            .iter()
            .map(|i| ItemMetadata::new(i.id.clone(), i.name.clone(), i.iframe.clone()))
            .collect(),
        mode: ListMode::External,
        query: String::from("SELECT name, user_score FROM tracks"),
    };
    create_external_list(db, session, list, items, user_id == DEMO_USER).await
}

pub fn new_spotify_item(track: topbops_web::Track, user_id: &String) -> Item {
    let mut metadata = Map::new();
    metadata.insert(String::from("album"), Value::String(track.album.name));
    metadata.insert(
        String::from("artists"),
        Value::String(
            track
                .artists
                .into_iter()
                .map(|a| a.name)
                .collect::<Vec<_>>()
                .join(", "),
        ),
    );
    Item {
        iframe: Some(format!(
            "https://open.spotify.com/embed/track/{}?utm_source=generator",
            track.id
        )),
        id: track.id,
        user_id: user_id.clone(),
        r#type: String::from("track"),
        name: track.name,
        rating: None,
        user_score: 1500,
        user_wins: 0,
        user_losses: 0,
        metadata,
    }
}

async fn create_user_list(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    list: List,
    is_upsert: bool,
) -> Result<(), Error> {
    let list_client = db.clone().into_collection_client("lists");
    let session_copy = session.read().unwrap().clone();
    let session = if let Some(session) = session_copy {
        list_client
            .create_document(
                Context::new(),
                &list,
                CreateDocumentOptions::new()
                    .is_upsert(true)
                    .consistency_level(session.clone()),
            )
            .await?;
        session
    } else {
        let resp = list_client
            .create_document(
                Context::new(),
                &list,
                CreateDocumentOptions::new().is_upsert(true),
            )
            .await
            .unwrap();
        let session_copy = ConsistencyLevel::Session(resp.session_token);
        *session.write().unwrap() = Some(session_copy.clone());
        session_copy
    };
    Ok(())
}

async fn create_external_list(
    db: DatabaseClient,
    session: Arc<RwLock<Option<ConsistencyLevel>>>,
    list: List,
    items: Vec<Item>,
    // Used to reset demo user data
    is_upsert: bool,
) -> Result<Response<Body>, Error> {
    let list_client = db.clone().into_collection_client("lists");
    let session_copy = session.read().unwrap().clone();
    let session = if let Some(session) = session_copy {
        list_client
            .create_document(
                Context::new(),
                &list,
                CreateDocumentOptions::new()
                    .is_upsert(true)
                    .consistency_level(session.clone()),
            )
            .await?;
        session
    } else {
        let resp = list_client
            .create_document(
                Context::new(),
                &list,
                CreateDocumentOptions::new().is_upsert(true),
            )
            .await
            .unwrap();
        let session_copy = ConsistencyLevel::Session(resp.session_token);
        *session.write().unwrap() = Some(session_copy.clone());
        session_copy
    };
    let items_client = db.clone().into_collection_client("items");
    let items_client = &items_client;
    let session = &session;
    futures::stream::iter(items.iter().map(async move |item| {
        items_client
            .create_document(
                Context::new(),
                item,
                CreateDocumentOptions::new()
                    .is_upsert(is_upsert)
                    .consistency_level(session.clone()),
            )
            .await
            .map(|_| ())
            .or_else(|e| {
                if let azure_data_cosmos::Error::Core(azure_core::Error::Policy(ref e)) = e {
                    if let Some(azure_core::HttpError::StatusCode {
                        status: StatusCode::CONFLICT,
                        ..
                    }) = e.downcast_ref::<azure_core::HttpError>()
                    {
                        return Ok(());
                    }
                }
                Err(e)
            })
    }))
    .buffered(5)
    .try_collect::<()>()
    .await?;
    get_response_builder()
        .status(StatusCode::CREATED)
        .body(Body::empty())
        .map_err(Error::from)
}

async fn get_token() -> Result<Token, Error> {
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
                .body(Body::from("grant_type=client_credentials"))?,
        )
        .await?;
    let got = hyper::body::to_bytes(resp.into_body()).await?;
    serde_json::from_slice(&got).map_err(Error::from)
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
    let client = CosmosClient::new(
        account.clone(),
        authorization_token,
        CosmosOptions::default(),
    );
    let session = Arc::new(RwLock::new(None));

    // Reset demo user data during startup in production
    if cfg!(not(feature = "dev")) {
        let demo_user = String::from(DEMO_USER);
        import_playlist(
            client.clone().into_database_client("smashsort"),
            Arc::clone(&session),
            demo_user.clone(),
            "5jPjYAdQO0MgzHdwSmYPNZ",
        )
        .await
        .unwrap();
        // Generate IDs using random but constant UUIDs
        create_user_list(
            client.clone().into_database_client("smashsort"),
            Arc::clone(&session),
            List {
                id: String::from("4539f893-8471-4e23-b815-cd7c8b722016"),
                user_id: demo_user.clone(),
                name: String::from("Winners"),
                items: Vec::new(),
                mode: ListMode::User,
                query: String::from("SELECT name, user_score FROM tracks WHERE user_score >= 1500"),
            },
            true,
        )
        .await
        .unwrap();
        create_user_list(
            client.clone().into_database_client("smashsort"),
            Arc::clone(&session),
            List {
                id: String::from("3c16df67-582d-449a-9862-0540f516d6b5"),
                user_id: demo_user.clone(),
                name: String::from("Artists"),
                items: Vec::new(),
                mode: ListMode::User,
                query: String::from("SELECT artists, AVG(user_score) FROM tracks GROUP BY artists"),
            },
            true,
        )
        .await
        .unwrap();
        create_user_list(
            client.clone().into_database_client("smashsort"),
            Arc::clone(&session),
            List {
                id: String::from("a425903e-d12f-43eb-8a53-dbfad3325fd5"),
                user_id: demo_user,
                name: String::from("Albums"),
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

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
enum Error {
    HyperError(hyper::Error),
    RequestError(hyper::http::Error),
    JSONError(serde_json::Error),
    CosmosError(azure_data_cosmos::Error),
    #[cfg(feature = "dev")]
    IOError(std::io::Error),
}

impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Error {
        Error::HyperError(e)
    }
}

impl From<hyper::http::Error> for Error {
    fn from(e: hyper::http::Error) -> Error {
        Error::RequestError(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::JSONError(e)
    }
}

impl From<azure_data_cosmos::Error> for Error {
    fn from(e: azure_data_cosmos::Error) -> Error {
        Error::CosmosError(e)
    }
}

#[cfg(feature = "dev")]
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::IOError(e)
    }
}

#[cfg(test)]
mod test {
    use sqlparser::ast::{Query, SetExpr, Statement};
    use sqlparser::dialect::GenericDialect;
    use sqlparser::parser::Parser;

    #[test]
    fn test_select() {
        let statement =
            Parser::parse_sql(&GenericDialect {}, "SELECT name, user_score FROM tracks")
                .unwrap()
                .pop();
        let mut query = if let Some(Statement::Query(box Query {
            body: SetExpr::Select(box s),
            ..
        })) = statement
        {
            s
        } else {
            unreachable!()
        };
        crate::transform_query(&mut query, "demo");
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\""
        );
    }

    #[test]
    fn test_where() {
        let statement = Parser::parse_sql(
            &GenericDialect {},
            "SELECT name, user_score FROM tracks WHERE user_score >= 1500",
        )
        .unwrap()
        .pop();
        let mut query = if let Some(Statement::Query(box Query {
            body: SetExpr::Select(box s),
            ..
        })) = statement
        {
            s
        } else {
            unreachable!()
        };
        crate::transform_query(&mut query, "demo");
        assert_eq!(query.to_string(), "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.user_score >= 1500");
    }

    #[test]
    fn test_group_by() {
        let statement = Parser::parse_sql(
            &GenericDialect {},
            "SELECT artists, AVG(user_score) FROM tracks GROUP BY artists",
        )
        .unwrap()
        .pop();
        let mut query = if let Some(Statement::Query(box Query {
            body: SetExpr::Select(box s),
            ..
        })) = statement
        {
            s
        } else {
            unreachable!()
        };
        crate::transform_query(&mut query, "demo");
        assert_eq!(query.to_string(), "SELECT c.metadata.artists, AVG(c.user_score) FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" GROUP BY c.metadata.artists");
    }
}
