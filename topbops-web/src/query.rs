use crate::{Error, ITEM_FIELDS};
use sqlparser::ast::{
    BinaryOperator, Expr, FunctionArg, FunctionArgExpr, Ident, Select, SelectItem, SetExpr,
    Statement, TableFactor,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::collections::VecDeque;

pub fn transform_query(query: &mut Select, user_id: &str) -> Result<(), Error> {
    let Some(from) = query.from.get_mut(0) else { return Err("FROM clause is omitted".into()); };
    let from = if let TableFactor::Table { name, alias, .. } = &mut from.relation {
        if alias.is_some() {
            return Err("alias is not supported".into());
        }
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
            SelectItem::UnnamedExpr(_) => {
                todo!()
            }
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
        right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", user_id)))),
    });
    let table_column_map = Box::new(Expr::BinaryOp {
        left: Box::new(Expr::CompoundIdentifier(vec![
            Ident::new("c"),
            Ident::new("type"),
        ])),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Identifier(Ident::new(format!("\"{}\"", from)))),
    });
    let sanitized_select = if let Some(mut selection) = query.selection.take() {
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
                Expr::InList { expr, .. } => {
                    if let Expr::Identifier(id) = &**expr {
                        *expr = Box::new(replace_identifier(id.clone()));
                    }
                }
                _ => {}
            }
        }
        Box::new(Expr::BinaryOp {
            left: table_column_map,
            op: BinaryOperator::And,
            right: Box::new(selection),
        })
    } else {
        table_column_map
    };
    query.selection = Some(Expr::BinaryOp {
        left: required_user_id,
        op: BinaryOperator::And,
        right: sanitized_select,
    });
    for expr in &mut query.group_by {
        match expr {
            Expr::Identifier(id) => {
                *expr = replace_identifier(id.clone());
            }
            _ => todo!(),
        }
    }
    Ok(())
}

fn replace_identifier(id: Ident) -> Expr {
    Expr::CompoundIdentifier(if ITEM_FIELDS.contains(&id.value.as_ref()) {
        vec![Ident::new("c"), id]
    } else {
        vec![Ident::new("c"), Ident::new("metadata"), id]
    })
}

pub fn parse_select(s: &str) -> Result<Select, Error> {
    let dialect = GenericDialect {};
    let statement = Parser::parse_sql(&dialect, s)?.pop();
    if let Some(Statement::Query(box sqlparser::ast::Query {
        body: SetExpr::Select(box s),
        ..
    })) = statement
    {
        Ok(s)
    } else {
        Err("No query was provided".into())
    }
}

#[cfg(test)]
mod test {
    use crate::Error;

    #[test]
    fn test_select() {
        let mut query = super::parse_select("SELECT name, user_score FROM tracks").unwrap();
        super::transform_query(&mut query, "demo").unwrap();
        assert_eq!(
            query.to_string(),
            "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\""
        );
    }

    #[test]
    fn test_where() {
        for (input, expected) in [
            ("SELECT name, user_score FROM tracks WHERE user_score >= 1500",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.user_score >= 1500"),
            ("SELECT name, user_score FROM tracks WHERE user_score IN (1500)",
             "SELECT c.name, c.user_score FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" AND c.user_score IN (1500)"),
        ] {
            let mut query = super::parse_select(input).unwrap();
            super::transform_query(&mut query, "demo").unwrap();
            assert_eq!(query.to_string(), expected);
        }
    }

    #[test]
    fn test_group_by() {
        let mut query =
            super::parse_select("SELECT artists, AVG(user_score) FROM tracks GROUP BY artists")
                .unwrap();
        super::transform_query(&mut query, "demo").unwrap();
        assert_eq!(query.to_string(), "SELECT c.metadata.artists, AVG(c.user_score) FROM c WHERE c.user_id = \"demo\" AND c.type = \"track\" GROUP BY c.metadata.artists");
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
            let query = super::parse_select(input);
            match query {
                Err(Error::SqlError(error)) => {
                    assert_eq!(error, expected);
                }
                Ok(mut query) => {
                    let err = super::transform_query(&mut query, "demo").unwrap_err();
                    if let Error::SqlError(error) = err {
                        assert_eq!(error, expected);
                    } else {
                        unreachable!()
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
