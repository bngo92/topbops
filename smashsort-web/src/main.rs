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
use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
#[cfg(feature = "dev")]
use tokio::fs::File;
#[cfg(feature = "dev")]
use tokio::io::AsyncReadExt;
use uuid::Uuid;

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
    pub score: i32,
    pub wins: i32,
    pub losses: i32,
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
    let db = db.into_database_client("songsort");
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
            "/" => Some((File::open("../smashsort-wasm/www/index.html"), "text/html")),
            "/smashsort_wasm.js" => Some((
                File::open("../smashsort-wasm/pkg/smashsort_wasm.js"),
                "application/javascript",
            )),
            "/smashsort_wasm_bg.wasm" => Some((
                File::open("../smashsort-wasm/pkg/smashsort_wasm_bg.wasm"),
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
    let user: smashsort_web::User = serde_json::from_slice(&got)?;

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
    Expr::CompoundIdentifier(if id.value == "score" {
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
        let statement = Parser::parse_sql(&GenericDialect {}, "SELECT track, score FROM tracks")
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
        assert_eq!(query.to_string(), "SELECT c.metadata.track, c.score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\"");
    }

    #[test]
    fn test_where() {
        let statement = Parser::parse_sql(
            &GenericDialect {},
            "SELECT track, score FROM tracks WHERE score >= 1500",
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
        assert_eq!(query.to_string(), "SELECT c.metadata.track, c.score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.score >= 1500");
    }

    #[test]
    fn test_group_by() {
        let statement = Parser::parse_sql(
            &GenericDialect {},
            "SELECT artists, AVG(score) FROM tracks GROUP BY artists",
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
        assert_eq!(query.to_string(), "SELECT c.metadata.artists, AVG(c.score) FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" GROUP BY c.metadata.artists");
    }
}
