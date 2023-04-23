use crate::{Error, UserId, ITEM_FIELDS};
use sqlparser::ast::{
    BinaryOperator, Expr, FunctionArg, FunctionArgExpr, Ident, Query, SelectItem, SetExpr,
    Statement, TableFactor, Value,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::collections::{HashMap, VecDeque};
use topbops::{ItemMetadata, List, ListMode};

pub fn rewrite_list_query<'a>(
    list: &'a List,
    user_id: &UserId,
) -> Result<(Query, Vec<String>, HashMap<String, &'a ItemMetadata>), Error> {
    // TODO: clean up column parsing
    let mut query = parse_select(&list.query)?;
    let SetExpr::Select(select) = &mut query.body else { return Err("Only SELECT queries are supported".into()) };
    let fields = select.projection.iter().map(ToString::to_string).collect();

    let mut map = HashMap::new();
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
    let query = if let ListMode::View = list.mode {
        &list.query
    } else {
        let i = query.find("FROM").unwrap();
        query.insert_str(i - 1, ", id ");
        // TODO: need a first class way to get rating
        query.insert_str(i - 1, ", rating ");
        query.insert_str(i - 1, ", hidden ");
        for i in &list.items {
            map.insert(i.id.clone(), i);
        }
        &query
    };
    let (query, _) = rewrite_query_impl(query, user_id, true)?;
    Ok((query, fields, map))
}

pub fn rewrite_query(s: &str, user_id: &UserId) -> Result<(Query, Vec<String>), Error> {
    rewrite_query_impl(s, user_id, false)
}

fn rewrite_query_impl(
    s: &str,
    user_id: &UserId,
    disable_hidden_filter: bool,
) -> Result<(Query, Vec<String>), Error> {
    let mut query = parse_select(s)?;
    let SetExpr::Select(select) = &mut query.body else { return Err("Only SELECT queries are supported".into()) };

    // TODO: support having via subquery
    let Some(from) = select.from.get_mut(0) else { return Err("FROM clause is omitted".into()); };
    let from = if let TableFactor::Table { name, alias, .. } = &mut from.relation {
        if alias.is_some() {
            return Err("alias is not supported".into());
        }
        std::mem::replace(&mut name.0[0].value, String::from("c"))
    } else {
        todo!();
    };
    let from = &from[..from.len() - 1];

    let column_names = select.projection.iter().map(ToString::to_string).collect();
    for expr in &mut select.projection {
        match expr {
            SelectItem::UnnamedExpr(expr) => rewrite_expr(expr),
            // TODO: support alias
            SelectItem::ExprWithAlias { .. } => {
                return Err("alias is not supported".into());
            }
            SelectItem::QualifiedWildcard(_) | SelectItem::Wildcard => {
                return Err("wildcard is not supported".into());
            }
        }
    }
    let required_user_id = Box::new(Expr::BinaryOp {
        left: Box::new(Expr::CompoundIdentifier(vec![
            Ident::new("c"),
            Ident::new("user_id"),
        ])),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", user_id.0)))),
    });
    let table_column_map = Box::new(Expr::BinaryOp {
        left: Box::new(Expr::CompoundIdentifier(vec![
            Ident::new("c"),
            Ident::new("type"),
        ])),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", from)))),
    });
    // If the query doesn't filter on hidden, default to hiding hidden items
    let hidden = || {
        Box::new(Expr::BinaryOp {
            left: Box::new(Expr::CompoundIdentifier(vec![
                Ident::new("c"),
                Ident::new("hidden"),
            ])),
            op: BinaryOperator::Eq,
            right: Box::new(Expr::Value(Value::Boolean(false))),
        })
    };
    let sanitized_select = if let Some(mut selection) = select.selection.take() {
        let no_filter = disable_hidden_filter || find_expr(&selection, |id| id.value == "hidden");
        rewrite_expr(&mut selection);
        if no_filter {
            Box::new(selection)
        } else {
            Box::new(Expr::BinaryOp {
                left: Box::new(selection),
                op: BinaryOperator::And,
                right: hidden(),
            })
        }
    } else {
        hidden()
    };
    select.selection = Some(Expr::BinaryOp {
        left: required_user_id,
        op: BinaryOperator::And,
        right: Box::new(Expr::BinaryOp {
            left: table_column_map,
            op: BinaryOperator::And,
            right: sanitized_select,
        }),
    });
    for expr in &mut select.group_by {
        rewrite_expr(expr);
    }
    for expr in &mut query.order_by {
        rewrite_expr(&mut expr.expr);
    }
    Ok((query, column_names))
}

fn parse_select(s: &str) -> Result<Query, Error> {
    let dialect = GenericDialect {};
    let statement = Parser::parse_sql(&dialect, s)?.pop();
    if let Some(Statement::Query(query)) = statement {
        Ok(*query)
    } else {
        Err("No query was provided".into())
    }
}

fn find_expr(expr: &Expr, predicate: fn(&Ident) -> bool) -> bool {
    let mut queue = VecDeque::new();
    queue.push_back(expr);
    while let Some(expr) = queue.pop_front() {
        match expr {
            Expr::Identifier(id) => {
                if predicate(id) {
                    return true;
                }
            }
            Expr::InList { expr, .. } => {
                if let Expr::Identifier(id) = &**expr {
                    if predicate(id) {
                        return true;
                    }
                }
            }
            Expr::BinaryOp { left, op: _, right } => {
                queue.push_back(left);
                queue.push_back(right);
            }
            Expr::Function(f) => {
                if let Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(id)))) =
                    f.args.last()
                {
                    if predicate(id) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
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
                if let Some(last) = f.args.pop() {
                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(id))) = last
                    {
                        f.args.push(FunctionArg::Unnamed(FunctionArgExpr::Expr(
                            rewrite_identifier(id),
                        )));
                    } else {
                        f.args.push(last);
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

    #[test]
    fn test_select() {
        let (query, column_names) = rewrite_query("SELECT name, user_score FROM tracks").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.hidden = false"
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_where() {
        for (input, expected) in [
            ("SELECT name, user_score FROM tracks WHERE user_score >= 1500",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.user_score >= 1500 AND c.hidden = false"),
            ("SELECT name, user_score FROM tracks WHERE user_score IN (1500)",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.user_score IN (1500) AND c.hidden = false"),
        ] {
            let (query, column_names) = rewrite_query(input).unwrap();
            assert_eq!(query.to_string(), expected);
            assert_eq!(column_names, vec!["name", "user_score"]);
        }
    }

    #[test]
    fn test_group_by() {
        let (query, column_names) =
            rewrite_query("SELECT artists, AVG(user_score) FROM tracks GROUP BY artists").unwrap();
        assert_eq!(query.to_string(), "SELECT c.metadata.artists, AVG(c.user_score) FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.hidden = false GROUP BY c.metadata.artists");
        assert_eq!(column_names, vec!["artists", "AVG(user_score)"]);
    }

    #[test]
    fn test_order_by() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks ORDER BY user_score").unwrap();
        assert_eq!(query.to_string(), "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.hidden = false ORDER BY c.user_score");
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_count() {
        let (query, column_names) = rewrite_query("SELECT COUNT(1) FROM tracks").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT COUNT(1) FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.hidden = false"
        );
        assert_eq!(column_names, vec!["COUNT(1)"]);
    }

    #[test]
    fn test_hidden_false() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks WHERE hidden = false").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.hidden = false"
        );
        assert_eq!(column_names, vec!["name", "user_score"]);
    }

    #[test]
    fn test_hidden_true() {
        let (query, column_names) =
            rewrite_query("SELECT name, user_score FROM tracks WHERE hidden = true").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.hidden = true"
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
            if let Error::SqlError(error) = err {
                assert_eq!(error, expected);
            } else {
                unreachable!()
            }
        }
    }
}
