use crate::{
    cosmos::{CosmosParam, CosmosQuery, CosmosSessionClient, QueryDocumentsBuilder, SessionClient},
    UserId, ITEM_FIELDS,
};
use serde_json::{Map, Value};
use sqlparser::{
    ast::{
        BinaryOperator, Expr, FunctionArg, FunctionArgExpr, Ident, Query, SelectItem, SetExpr,
        Statement, TableFactor,
    },
    dialect::MySqlDialect,
    parser::Parser,
};
use std::collections::{HashMap, VecDeque};
use zeroflops::{Error, ItemMetadata, ItemQuery, List, ListMode};

pub async fn get_list_query(
    client: &impl SessionClient,
    user_id: &UserId,
    list: List,
) -> Result<ItemQuery, Error> {
    if list.items.is_empty() {
        Ok(ItemQuery {
            fields: Vec::new(),
            items: Vec::new(),
        })
    } else {
        let (query, fields, map, ids) = rewrite_list_query(&list, user_id)?;
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

pub async fn get_list_items(
    client: &CosmosSessionClient,
    user_id: &UserId,
    list: List,
) -> Result<Vec<Map<String, Value>>, Error> {
    let query = if let ListMode::View = &list.mode {
        rewrite_query(&list.query, user_id)?.0.to_string()
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

fn rewrite_list_query<'a>(
    list: &'a List,
    user_id: &UserId,
) -> Result<
    (
        Query,
        Vec<String>,
        HashMap<String, &'a ItemMetadata>,
        Vec<String>,
    ),
    Error,
> {
    // TODO: clean up column parsing
    let mut query = parse_select(&list.query)?;
    let SetExpr::Select(select) = &mut *query.body else {
        return Err(Error::client_error("Only SELECT queries are supported"));
    };
    let fields = select.projection.iter().map(ToString::to_string).collect();

    let mut map = HashMap::new();
    // TODO: update AST directly
    let ids = list
        .items
        .iter()
        .map(|i| format!("\"{}\"", i.id))
        .collect::<Vec<_>>();
    let (query, _) = if let ListMode::View = list.mode {
        rewrite_query(&list.query, user_id)?
    } else {
        let mut query = list.query.clone();
        let i = query.find("FROM").unwrap();
        query.insert_str(i - 1, ", id ");
        for i in &list.items {
            map.insert(i.id.clone(), i);
        }
        rewrite_query_impl(&query, user_id, Some(id_filter(&ids)))?
    };
    Ok((query, fields, map, ids))
}

fn id_filter(ids: &[String]) -> Expr {
    Expr::InList {
        expr: Box::new(Expr::CompoundIdentifier(vec![
            Ident::new("c"),
            Ident::new("id"),
        ])),
        list: ids
            .iter()
            .map(|id| Expr::Identifier(Ident::new(id)))
            .collect(),
        negated: false,
    }
}

pub fn rewrite_query(s: &str, user_id: &UserId) -> Result<(Query, Vec<String>), Error> {
    rewrite_query_impl(s, user_id, None)
}

fn rewrite_query_impl(
    s: &str,
    user_id: &UserId,
    filter: Option<Expr>,
) -> Result<(Query, Vec<String>), Error> {
    let mut query = parse_select(s)?;
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
        name.0[0].value = String::from("c");
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
        left: Box::new(Expr::CompoundIdentifier(vec![
            Ident::new("c"),
            Ident::new("user_id"),
        ])),
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

fn parse_select(s: &str) -> Result<Query, Error> {
    // The MySQL dialect seems to be the closest to Cosmos DB in regards to string value handling
    let dialect = MySqlDialect {};
    let statement = Parser::parse_sql(&dialect, s)?.pop();
    if let Some(Statement::Query(query)) = statement {
        Ok(*query)
    } else {
        Err(Error::client_error("No query was provided"))
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
    Expr::CompoundIdentifier(if ITEM_FIELDS.contains(&id.value.as_ref()) {
        vec![Ident::new("c"), id]
    } else {
        vec![Ident::new("c"), Ident::new("metadata"), id]
    })
}

#[cfg(test)]
pub mod test {
    use crate::{
        cosmos::{
            CosmosQuery, DocumentWriter, GetDocumentBuilder, QueryDocumentsBuilder, SessionClient,
        },
        UserId,
    };
    use async_trait::async_trait;
    use azure_data_cosmos::CosmosEntity;
    use serde::{de::DeserializeOwned, Serialize};
    use sqlparser::ast::Query;
    use std::sync::{Arc, Mutex};
    use zeroflops::{Error, Item, ItemMetadata, ItemQuery, List, ListMode};

    pub struct Mock<T, U> {
        pub call_args: Arc<Mutex<Vec<T>>>,
        side_effect: Vec<U>,
    }

    impl<T, U> Mock<T, U> {
        pub fn new(side_effect: Vec<U>) -> Mock<T, U> {
            Mock {
                call_args: Arc::new(Mutex::new(Vec::new())),
                side_effect,
            }
        }

        pub fn empty() -> Mock<T, U> {
            Mock {
                call_args: Arc::new(Mutex::new(Vec::new())),
                side_effect: Vec::new(),
            }
        }
    }

    impl<T, U: Clone> Mock<T, U> {
        pub fn call(&self, arg: T) -> U {
            let mut call_args = self.call_args.lock().unwrap();
            let value = self.side_effect[call_args.len()].clone();
            call_args.push(arg);
            value
        }
    }

    struct TestSessionClient {
        query_mock: Mock<QueryDocumentsBuilder, &'static str>,
    }

    #[async_trait]
    impl SessionClient for TestSessionClient {
        async fn get_document<T>(&self, _: GetDocumentBuilder) -> Result<Option<T>, Error>
        where
            T: DeserializeOwned + Send + Sync,
        {
            unimplemented!()
        }

        async fn query_documents<T>(&self, builder: QueryDocumentsBuilder) -> Result<Vec<T>, Error>
        where
            T: DeserializeOwned + Send + Sync,
        {
            let value = self.query_mock.call(builder);
            Ok(serde_json::de::from_str(value).unwrap())
        }

        /// CosmosDB creates new session tokens after writes
        async fn write_document<T>(
            &self,
            _: DocumentWriter<T>,
        ) -> Result<(), azure_core::error::Error>
        where
            T: Serialize + CosmosEntity + Send + 'static,
        {
            unimplemented!()
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
            super::get_list_query(
                &TestSessionClient {
                    query_mock: Mock::empty(),
                },
                &UserId(String::new()),
                list,
            )
            .await
            .unwrap(),
            ItemQuery {
                fields: Vec::new(),
                items: Vec::new()
            }
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
            query_mock: Mock::new(vec![r#"[{"name":"test","user_score":0,"id":"id"}]"#]),
        };
        assert_eq!(
            super::get_list_query(&client, &UserId(String::new()), list)
                .await
                .unwrap(),
            ItemQuery {
                fields: vec!["name".to_owned(), "user_score".to_owned()],
                items: vec![Item {
                    values: vec!["test".to_owned(), "0".to_owned()],
                    metadata: Some(ItemMetadata {
                        id: "id".to_owned(),
                        name: "".to_owned(),
                        iframe: None,
                        score: 0,
                        wins: 0,
                        losses: 0,
                        rank: None
                    })
                }]
            }
        );
        assert_eq!(
            *client.query_mock.call_args.lock().unwrap(),
            [QueryDocumentsBuilder::new(
                "items",
                CosmosQuery::new("SELECT c.name, c.user_score, c.id FROM c WHERE c.user_id = \"\" AND c.id IN (\"\")".to_owned())
            )]
        );
    }

    #[tokio::test]
    async fn test_get_list_query() {
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
            query_mock: Mock::new(vec!["[]"]),
        };
        assert_eq!(
            super::get_list_query(&client, &UserId(String::new()), list,)
                .await
                .unwrap(),
            ItemQuery {
                fields: vec!["name".to_owned(), "user_score".to_owned()],
                items: Vec::new()
            }
        );
        assert_eq!(
            *client.query_mock.call_args.lock().unwrap(),
            [QueryDocumentsBuilder::new(
                "items",
                CosmosQuery::new("SELECT c.name, c.user_score, c.id FROM c WHERE c.user_id = \"\" AND c.id IN (\"\")".to_owned())
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
            query,
            &UserId(String::from("demo")),
            Some(super::id_filter(ids)),
        )
    }

    #[test]
    fn test_select() {
        let (query, column_names) = rewrite_query("SELECT name, user_score FROM tracks").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\""
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_where() {
        for (input, expected) in [
            ("SELECT name, user_score FROM tracks WHERE user_score >= 1500",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.user_score >= 1500"),
            ("SELECT name, user_score FROM tracks WHERE user_score IN (1500)",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.user_score IN (1500)"),
            ("SELECT name, user_score FROM tracks WHERE album = 'foo'",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.metadata.album = 'foo'"),
            ("SELECT name, user_score FROM tracks WHERE album = \"foo\"",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.metadata.album = \"foo\""),
            ("SELECT name, user_score FROM tracks WHERE ARRAY_CONTAINS(artists, \"foo\")",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND ARRAY_CONTAINS(c.metadata.artists, \"foo\")"),
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
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.id IN (\"1\", \"2\", \"3\") AND c.user_score >= 1500"),
            ("SELECT name, user_score FROM tracks WHERE user_score IN (1500)",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.id IN (\"1\", \"2\", \"3\") AND c.user_score IN (1500)"),
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
        assert_eq!(query.to_string(), "SELECT c.metadata.artists, AVG(c.user_score) FROM c WHERE c.user_id = \"demo\" GROUP BY c.metadata.artists");
        assert_eq!(column_names, vec!["artists", "AVG(user_score)"]);
    }

    #[test]
    fn test_order_by() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks ORDER BY user_score").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" ORDER BY c.user_score"
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_count() {
        let (query, column_names) = rewrite_query("SELECT COUNT(1) FROM tracks").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT COUNT(1) FROM c WHERE c.user_id = \"demo\""
        );
        assert_eq!(column_names, vec!["COUNT(1)"]);
    }

    #[test]
    fn test_hidden_false() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks WHERE hidden = false").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.hidden = false"
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_hidden_true() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks WHERE hidden = true").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.hidden = true"
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
