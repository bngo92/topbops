use crate::{UserId, ITEM_FIELDS};
use sqlparser::ast::{
    BinaryOperator, Expr, FunctionArg, FunctionArgExpr, Ident, Query, SelectItem, SetExpr,
    Statement, TableFactor,
};
use sqlparser::dialect::MySqlDialect;
use sqlparser::parser::Parser;
use std::collections::{HashMap, VecDeque};
use zeroflops::{Error, ItemMetadata, List, ListMode};

pub fn rewrite_list_query<'a>(
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
    let SetExpr::Select(select) = &mut *query.body else { return Err(Error::client_error("Only SELECT queries are supported")) };
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
    let SetExpr::Select(select) = &mut *query.body else { return Err(Error::client_error("Only SELECT queries are supported")) };

    // TODO: do we still need this
    // TODO: support having via subquery
    let Some(from) = select.from.get_mut(0) else { return Err(Error::client_error("FROM clause is omitted")); };
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
mod test {
    use crate::{Error, UserId};
    use sqlparser::ast::Query;

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
