use crate::{UserId, ITEM_FIELDS};
use serde_json::{Map, Value};
use sqlparser::{
    ast::{
        BinaryOperator, Expr, FunctionArg, FunctionArgExpr, Ident, JsonOperator, Query, SelectItem,
        SetExpr, Statement, TableFactor,
    },
    dialect::MySqlDialect,
    parser::Parser,
};
use std::collections::{HashMap, HashSet, VecDeque};
use zeroflops::{
    storage::{CosmosParam, CosmosQuery, QueryDocumentsBuilder, SessionClient, SqlSessionClient},
    Error, ItemMetadata, Items, List, ListMode,
};

pub async fn get_view_items(
    client: &impl SessionClient,
    list: &List,
) -> Result<impl Iterator<Item = ItemMetadata>, Error> {
    let mut query = list.query.into_query()?;
    let SetExpr::Select(select) = &mut *query.body else {
        return Err(Error::client_error("Only SELECT queries are supported"));
    };
    // GROUP BY queries create schemas that don't produce items
    let items = if select.group_by.is_empty() {
        Vec::new()
    } else {
        select.projection = ["id", "name", "iframe"]
            .into_iter()
            .map(|s| SelectItem::UnnamedExpr(Expr::Identifier(Ident::new(s))))
            .collect();
        let (query, _) = rewrite_query(query, &UserId(list.user_id.clone()))?;
        client
            .query_documents::<Map<String, Value>>(QueryDocumentsBuilder::new(
                "item",
                CosmosQuery::new(query.to_string()),
            ))
            .await?
    };
    Ok(items.into_iter().map(|item| ItemMetadata {
        id: item["id"].as_str().unwrap().to_owned(),
        name: item["name"].as_str().unwrap().to_owned(),
        iframe: item["iframe"].as_str().map(ToOwned::to_owned),
        score: 0,
        wins: 0,
        losses: 0,
        rank: None,
    }))
}

pub async fn get_list_items(
    client: &impl SessionClient,
    user_id: &UserId,
    list: List,
) -> Result<Items, Error> {
    if list.items.is_empty() {
        Ok(Items { items: Vec::new() })
    } else {
        let (query, map, ids) = rewrite_list_query(&list, user_id)?;
        let mut items: Vec<_> = client
            .query_documents::<Map<String, Value>>(QueryDocumentsBuilder::new(
                "item",
                CosmosQuery::new(query.to_string()),
            ))
            .await
            .map_err(Error::from)?
            .into_iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect();
        // Use list item order if an ordering wasn't provided
        if query.order_by.is_empty() {
            let item_metadata: HashSet<_> = items.into_iter().collect();
            items = ids
                .into_iter()
                .filter(|id| item_metadata.contains(id))
                .collect();
        };
        Ok(Items {
            items: items
                .into_iter()
                .map(|id| {
                    if map.is_empty() {
                        None
                    } else {
                        Some(map[&id].clone())
                    }
                })
                .collect(),
        })
    }
}

pub async fn query_list(
    client: &SqlSessionClient,
    user_id: &UserId,
    list: List,
    fields: Option<&Vec<String>>,
) -> Result<Vec<Map<String, Value>>, Error> {
    let query = if let ListMode::View(_) = &list.mode {
        let query = list.query.into_query()?;
        CosmosQuery::with_params(rewrite_query(query, user_id)?.0.to_string(), Vec::new())
    } else if list.items.is_empty() {
        return Ok(Vec::new());
    } else {
        let fields = if let Some(fields) = fields {
            fields.join(", ")
        } else {
            "id, type, name, rating, user_score, user_wins, user_losses, hidden, metadata"
                .to_owned()
        };
        CosmosQuery::with_params(
            format!(
                "SELECT {} FROM item WHERE user_id = ?1 AND id IN ({})",
                fields,
                &"?,".repeat(list.items.len())[..list.items.len() * 2 - 1],
            ),
            std::iter::once(CosmosParam::new(
                String::from("@user_id"),
                user_id.0.clone(),
            ))
            .chain(
                list.items
                    .iter()
                    .map(|i| CosmosParam::new(String::from("@ids"), i.id.clone())),
            )
            .collect::<Vec<_>>(),
        )
    };
    Ok(client
        .query_documents::<Map<String, Value>>(QueryDocumentsBuilder::new("item", query))
        .await
        .map_err(Error::from)?
        .into_iter()
        // Cast hidden to bool
        .map(|mut m| {
            if let Some(hidden) = m.get_mut("hidden") {
                *hidden = Value::Bool(hidden.as_i64().unwrap() != 0);
            }
            m
        })
        .collect())
}

fn rewrite_list_query<'a>(
    list: &'a List,
    user_id: &UserId,
) -> Result<(Query, HashMap<String, &'a ItemMetadata>, Vec<String>), Error> {
    let mut map = HashMap::new();
    // TODO: update AST directly
    let ids = list
        .items
        .iter()
        .map(|i| format!("\"{}\"", i.id))
        .collect::<Vec<_>>();
    let (query, _) = if let ListMode::View(_) = list.mode {
        rewrite_query(&list.query, user_id)?
    } else {
        let mut query = list.query.clone();
        let i = query.find("FROM").unwrap();
        query.insert_str(i - 1, ", id ");
        for i in &list.items {
            map.insert(i.id.clone(), i);
        }
        rewrite_query_impl(query.into_query()?, user_id, Some(id_filter(&ids)))?
    };
    Ok((
        query,
        map,
        list.items.iter().map(|i| i.id.to_owned()).collect(),
    ))
}

fn id_filter(ids: &[String]) -> Expr {
    Expr::InList {
        expr: Box::new(Expr::CompoundIdentifier(vec![Ident::new("id")])),
        list: ids
            .iter()
            .map(|id| Expr::Identifier(Ident::new(id)))
            .collect(),
        negated: false,
    }
}

pub fn rewrite_query(
    query: impl IntoQuery,
    user_id: &UserId,
) -> Result<(Query, Vec<String>), Error> {
    rewrite_query_impl(query.into_query()?, user_id, None)
}

fn rewrite_query_impl(
    mut query: Query,
    user_id: &UserId,
    filter: Option<Expr>,
) -> Result<(Query, Vec<String>), Error> {
    let SetExpr::Select(select) = &mut *query.body else {
        return Err(Error::client_error("Only SELECT queries are supported"));
    };

    // TODO: do we still need this
    // TODO: support having via subquery
    let Some(from) = select.from.get_mut(0) else {
        return Err(Error::client_error("FROM clause is omitted"));
    };
    if let TableFactor::Table { name, alias, .. } = &mut from.relation {
        if alias.is_some() {
            return Err(Error::client_error("alias is not supported"));
        }
        name.0[0].value = String::from("item");
    } else {
        todo!();
    };

    let column_names = select.projection.iter().map(ToString::to_string).collect();
    for expr in &mut select.projection {
        match expr {
            SelectItem::UnnamedExpr(expr) => rewrite_expr(expr),
            // TODO: support alias
            SelectItem::ExprWithAlias { .. } => {
                return Err(Error::client_error("alias is not supported"));
            }
            SelectItem::QualifiedWildcard(..) | SelectItem::Wildcard(..) => {
                return Err(Error::client_error("wildcard is not supported"));
            }
        }
    }
    let required_user_id = Expr::BinaryOp {
        left: Box::new(Expr::CompoundIdentifier(vec![Ident::new("user_id")])),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", user_id.0)))),
    };
    let mut sanitized_select = select.selection.take();
    if let Some(selection) = &mut sanitized_select {
        rewrite_expr(selection);
    }
    let selection = match (filter, sanitized_select) {
        (None, None) => None,
        (None, Some(sanitized_select)) => Some(sanitized_select),
        (Some(filter), None) => Some(filter),
        (Some(filter), Some(sanitized_select)) => Some(Expr::BinaryOp {
            left: Box::new(filter),
            op: BinaryOperator::And,
            right: Box::new(sanitized_select),
        }),
    };
    select.selection = if let Some(selection) = selection {
        Some(Expr::BinaryOp {
            left: Box::new(required_user_id),
            op: BinaryOperator::And,
            right: Box::new(selection),
        })
    } else {
        Some(required_user_id)
    };
    for expr in &mut select.group_by {
        rewrite_expr(expr);
    }
    for expr in &mut query.order_by {
        rewrite_expr(&mut expr.expr);
    }
    Ok((query, column_names))
}

pub trait IntoQuery {
    fn into_query(self) -> Result<Query, Error>;
}

impl IntoQuery for &String {
    fn into_query(self) -> Result<Query, Error> {
        self.as_str().into_query()
    }
}

impl IntoQuery for &str {
    fn into_query(self) -> Result<Query, Error> {
        // The MySQL dialect seems to be the closest to Cosmos DB in regards to string value handling
        let dialect = MySqlDialect {};
        let statement = Parser::parse_sql(&dialect, self)?.pop();
        if let Some(Statement::Query(query)) = statement {
            Ok(*query)
        } else {
            Err(Error::client_error("No query was provided"))
        }
    }
}

impl IntoQuery for Query {
    fn into_query(self) -> Result<Query, Error> {
        Ok(self)
    }
}

fn rewrite_expr(expr: &mut Expr) {
    let mut queue = VecDeque::new();
    queue.push_back(expr);
    while let Some(expr) = queue.pop_front() {
        match expr {
            Expr::Identifier(id) => {
                *expr = rewrite_identifier(id.clone());
            }
            Expr::InList { expr, .. } => {
                if let Expr::Identifier(id) = &**expr {
                    *expr = Box::new(rewrite_identifier(id.clone()));
                }
            }
            Expr::BinaryOp { left, op: _, right } => {
                queue.push_back(left);
                queue.push_back(right);
            }
            Expr::Function(f) => {
                for arg in &mut f.args {
                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) = arg {
                        if let Expr::Identifier(id) = expr.clone() {
                            *expr = rewrite_identifier(id);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn rewrite_identifier(id: Ident) -> Expr {
    if ITEM_FIELDS.contains(&id.value.as_ref()) {
        Expr::Identifier(id)
    } else {
        Expr::JsonAccess {
            left: Box::new(Expr::Identifier(Ident::new("metadata"))),
            operator: JsonOperator::Arrow,
            right: Box::new(Expr::Identifier(Ident::new(format!("'{}'", id.value)))),
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::IntoQuery;
    use crate::UserId;
    use async_trait::async_trait;
    use serde::{de::DeserializeOwned, Serialize};
    use sqlparser::ast::Query;
    use std::sync::{Arc, Mutex};
    use zeroflops::{
        storage::{
            CosmosQuery, CreateDocumentBuilder, DeleteDocumentBuilder, DocumentWriter,
            GetDocumentBuilder, QueryDocumentsBuilder, ReplaceDocumentBuilder, SessionClient,
        },
        Error, ItemMetadata, Items, List, ListMode,
    };

    pub struct Mock<T, U> {
        pub call_args: Arc<Mutex<Vec<T>>>,
        side_effect: Arc<Mutex<Vec<Option<U>>>>,
    }

    impl<T, U> Mock<T, U> {
        pub fn new(side_effect: Vec<U>) -> Mock<T, U> {
            Mock {
                call_args: Arc::new(Mutex::new(Vec::new())),
                side_effect: Arc::new(Mutex::new(
                    side_effect.into_iter().map(Option::Some).collect(),
                )),
            }
        }

        pub fn empty() -> Mock<T, U> {
            Mock {
                call_args: Arc::new(Mutex::new(Vec::new())),
                side_effect: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl<T, U> Mock<T, U> {
        pub fn call(&self, arg: T) -> U {
            let mut call_args = self.call_args.lock().unwrap();
            let value = self.side_effect.lock().unwrap()[call_args.len()]
                .take()
                .unwrap();
            call_args.push(arg);
            value
        }
    }

    pub struct TestSessionClient {
        pub get_mock: Mock<GetDocumentBuilder, &'static str>,
        pub query_mock: Mock<QueryDocumentsBuilder, &'static str>,
        pub write_mock: Mock<DocumentWriter<String>, ()>,
    }

    #[async_trait]
    impl SessionClient for TestSessionClient {
        async fn get_document<T>(&self, builder: GetDocumentBuilder) -> Result<Option<T>, Error>
        where
            T: DeserializeOwned + Send + Sync,
        {
            let value = self.get_mock.call(builder);
            Ok(serde_json::de::from_str(value).unwrap())
        }

        async fn query_documents<T>(&self, builder: QueryDocumentsBuilder) -> Result<Vec<T>, Error>
        where
            T: DeserializeOwned + Send + Sync,
        {
            let value = self.query_mock.call(builder);
            Ok(serde_json::de::from_str(value).unwrap())
        }

        /// CosmosDB creates new session tokens after writes
        async fn write_document<T>(&self, builder: DocumentWriter<T>) -> Result<(), Error>
        where
            T: Serialize + Send + 'static,
        {
            let builder = match builder {
                DocumentWriter::Create(builder) => DocumentWriter::Create(CreateDocumentBuilder {
                    collection_name: builder.collection_name,
                    document: serde_json::to_string(&builder.document).unwrap(),
                    is_upsert: builder.is_upsert,
                }),
                DocumentWriter::Replace(builder) => {
                    DocumentWriter::Replace(ReplaceDocumentBuilder {
                        collection_name: builder.collection_name,
                        document_name: builder.document_name,
                        partition_key: builder.partition_key,
                        document: serde_json::to_string(&builder.document).unwrap(),
                    })
                }
                DocumentWriter::Delete(builder) => DocumentWriter::Delete(DeleteDocumentBuilder {
                    collection_name: builder.collection_name,
                    document_name: builder.document_name,
                    partition_key: builder.partition_key,
                }),
            };
            self.write_mock.call(builder);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_get_empty_list_query() {
        let list = List {
            id: String::new(),
            user_id: String::new(),
            mode: ListMode::User(None),
            name: String::new(),
            sources: Vec::new(),
            iframe: None,
            items: Vec::new(),
            favorite: false,
            query: String::from("SELECT name, user_score FROM c"),
        };
        assert_eq!(
            super::get_list_items(
                &TestSessionClient {
                    get_mock: Mock::empty(),
                    query_mock: Mock::empty(),
                    write_mock: Mock::empty(),
                },
                &UserId(String::new()),
                list,
            )
            .await
            .unwrap(),
            Items { items: Vec::new() }
        );
    }

    #[tokio::test]
    async fn test_get_list_empty_query() {
        let list = List {
            id: String::new(),
            user_id: String::new(),
            mode: ListMode::User(None),
            name: String::new(),
            sources: Vec::new(),
            iframe: None,
            items: vec![ItemMetadata {
                id: "id".to_owned(),
                name: String::new(),
                iframe: None,
                score: 0,
                wins: 0,
                losses: 0,
                rank: None,
            }],
            favorite: false,
            query: String::from("SELECT name, user_score FROM c"),
        };
        let client = TestSessionClient {
            get_mock: Mock::empty(),
            query_mock: Mock::new(vec![r#"[{"name":"test","user_score":0,"id":"id"}]"#]),
            write_mock: Mock::empty(),
        };
        assert_eq!(
            super::get_list_items(&client, &UserId(String::new()), list)
                .await
                .unwrap(),
            Items {
                items: vec![Some(ItemMetadata {
                    id: "id".to_owned(),
                    name: "".to_owned(),
                    iframe: None,
                    score: 0,
                    wins: 0,
                    losses: 0,
                    rank: None
                })]
            }
        );
        assert_eq!(
            *client.query_mock.call_args.lock().unwrap(),
            [QueryDocumentsBuilder::new(
                "item",
                CosmosQuery::new(
                    "SELECT name, user_score, id FROM item WHERE user_id = \"\" AND id IN (\"id\")"
                        .to_owned()
                )
            )]
        );
    }

    #[tokio::test]
    async fn test_get_list_items() {
        let list = List {
            id: String::new(),
            user_id: String::new(),
            mode: ListMode::User(None),
            name: String::new(),
            sources: Vec::new(),
            iframe: None,
            items: vec![ItemMetadata {
                id: String::new(),
                name: String::new(),
                iframe: None,
                score: 0,
                wins: 0,
                losses: 0,
                rank: None,
            }],
            favorite: false,
            query: String::from("SELECT name, user_score FROM c"),
        };
        let client = TestSessionClient {
            get_mock: Mock::empty(),
            query_mock: Mock::new(vec![r#"[{"name":"test","user_score":0,"id":"id"}]"#]),
            write_mock: Mock::empty(),
        };
        assert_eq!(
            super::get_list_items(&client, &UserId(String::new()), list,)
                .await
                .unwrap(),
            Items { items: Vec::new() }
        );
        assert_eq!(
            *client.query_mock.call_args.lock().unwrap(),
            [QueryDocumentsBuilder::new(
                "item",
                CosmosQuery::new(
                    "SELECT name, user_score, id FROM item WHERE user_id = \"\" AND id IN (\"\")"
                        .to_owned()
                )
            )]
        );
    }

    fn rewrite_query(query: &str) -> Result<(Query, Vec<String>), Error> {
        super::rewrite_query(query, &UserId(String::from("demo")))
    }

    fn rewrite_query_with_id_filter(
        query: &str,
        ids: &[String],
    ) -> Result<(Query, Vec<String>), Error> {
        super::rewrite_query_impl(
            query.into_query()?,
            &UserId(String::from("demo")),
            Some(super::id_filter(ids)),
        )
    }

    #[test]
    fn test_select() {
        let (query, column_names) = rewrite_query("SELECT name, user_score FROM tracks").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT name, user_score FROM item WHERE user_id = \"demo\""
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_where() {
        for (input, expected) in [
            ("SELECT name, user_score FROM tracks WHERE user_score >= 1500",
             "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND user_score >= 1500"),
            ("SELECT name, user_score FROM tracks WHERE user_score IN (1500)",
             "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND user_score IN (1500)"),
            ("SELECT name, user_score FROM tracks WHERE album = 'foo'",
             "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND metadata -> 'album' = 'foo'"),
            ("SELECT name, user_score FROM tracks WHERE album = \"foo\"",
             "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND metadata -> 'album' = \"foo\""),
            ("SELECT name, user_score FROM tracks WHERE ARRAY_CONTAINS(artists, \"foo\")",
             "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND ARRAY_CONTAINS(metadata -> 'artists', \"foo\")"),
        ] {
            let (query, column_names) = rewrite_query(input).unwrap();
            assert_eq!(query.to_string(), expected);
            assert_eq!(column_names, vec!["name", "user_score"]);
        }
    }

    #[test]
    fn test_id_filter() {
        for (input, expected) in [
            ("SELECT name, user_score FROM tracks WHERE user_score >= 1500",
             "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND id IN (\"1\", \"2\", \"3\") AND user_score >= 1500"),
            ("SELECT name, user_score FROM tracks WHERE user_score IN (1500)",
             "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND id IN (\"1\", \"2\", \"3\") AND user_score IN (1500)"),
        ] {
            let (query, column_names) = rewrite_query_with_id_filter(input, &["\"1\"".into(), "\"2\"".into(), "\"3\"".into()]).unwrap();
            assert_eq!(query.to_string(), expected);
            assert_eq!(column_names, vec!["name", "user_score"]);
        }
    }

    #[test]
    fn test_group_by() {
        let (query, column_names) =
            rewrite_query("SELECT artists, AVG(user_score) FROM tracks GROUP BY artists").unwrap();
        assert_eq!(query.to_string(), "SELECT metadata -> 'artists', AVG(user_score) FROM item WHERE user_id = \"demo\" GROUP BY metadata -> 'artists'");
        assert_eq!(column_names, vec!["artists", "AVG(user_score)"]);
    }

    #[test]
    fn test_order_by() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks ORDER BY user_score").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT name, user_score FROM item WHERE user_id = \"demo\" ORDER BY user_score"
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_count() {
        let (query, column_names) = rewrite_query("SELECT COUNT(1) FROM tracks").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT COUNT(1) FROM item WHERE user_id = \"demo\""
        );
        assert_eq!(column_names, vec!["COUNT(1)"]);
    }

    #[test]
    fn test_hidden_false() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks WHERE hidden = false").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND hidden = false"
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_hidden_true() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks WHERE hidden = true").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT name, user_score FROM item WHERE user_id = \"demo\" AND hidden = true"
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_errors() {
        for (input, expected) in [
            ("", "No query was provided"),
            ("S", "Expected an SQL statement, found: S"),
            ("SELECT", "Expected an expression:, found: EOF"),
            ("SELECT name", "FROM clause is omitted"),
            ("SELECT name FROM", "Expected identifier, found: EOF"),
            (
                "SELECT name FROM tracks WHERE",
                "Expected an expression:, found: EOF",
            ),
            (
                "SELECT name, user_score FROM tracks WHERE user_score IN (",
                "Expected an expression:, found: EOF",
            ),
            (
                "SELECT name, user_score FROM tracks WHERE user_score IN (1500",
                "Expected ), found: EOF",
            ),
        ] {
            let err = rewrite_query(input).unwrap_err();
            if let Error::ClientError(error) = err {
                assert_eq!(error, expected);
            } else {
                unreachable!()
            }
        }
    }
}
