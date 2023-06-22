use std::collections::HashMap;
use web_sys::{HtmlSelectElement, KeyboardEvent};
use yew::{html, Component, Context, Html, NodeRef, Properties};
use yew_router::scope_ext::RouterScopeExt;
use zeroflops::ItemQuery;

pub enum Msg {
    None,
    Fetching,
    Success(ItemQuery),
    Failed(String),
}

pub struct Search {
    search_ref: NodeRef,
    query: Option<ItemQuery>,
    error: Option<String>,
    format: Format,
}

enum Format {
    Table,
    Csv,
}

impl Component for Search {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let query = ctx
            .link()
            .location()
            .unwrap()
            .query::<HashMap<String, String>>()
            .unwrap_or_default();
        let format = match query.get("mode").map(String::as_str) {
            Some("csv") => Format::Csv,
            _ => Format::Table,
        };
        Search {
            search_ref: NodeRef::default(),
            query: None,
            error: None,
            format,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let default_search = "SELECT name, user_score FROM tracks";
        let search = ctx.link().callback(|_| Msg::Fetching);
        let (class, error) = if let Some(error) = &self.error {
            (
                "w-100 is-invalid",
                Some(html! {<div class="invalid-feedback">{error}</div>}),
            )
        } else {
            ("w-100", None)
        };
        let onkeydown = ctx.link().callback(|event: KeyboardEvent| {
            if event.key_code() == 13 {
                event.prevent_default();
                Msg::Fetching
            } else {
                Msg::None
            }
        });
        html! {
            <div>
                <form {onkeydown}>
                    <div class="row">
                        <div class="col-12 col-md-10 col-xl-11">
                            // Copy only the styles from .form-control that are needed for sizing
                            <input ref={self.search_ref.clone()} type="text" {class} style="padding: .5rem 1rem; font-size: .875rem; border-width: 1px" placeholder={default_search}/>
                            {for error}
                        </div>
                        <div class="col-3 col-sm-2 col-md-2 col-xl-1 pe-2">
                            <button type="button" class="btn btn-success" onclick={search}>{"Search"}</button>
                        </div>
                    </div>
                </form>
                if let Some(query) = &self.query {
                    if let Format::Table = self.format {
                        <Table query={query.clone()}/>
                    } else if let Format::Csv = self.format {
                        <p>{query
                            .items
                                .iter()
                                .map(|items| html! {items.values.join(",")})
                                .intersperse(html! {<br/>})
                                .collect::<Html>()}</p>
                    }
                }
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => false,
            Msg::Fetching => {
                let input = self.search_ref.cast::<HtmlSelectElement>().unwrap().value();
                ctx.link().send_future(async move {
                    match crate::find_items(&input).await {
                        Ok(query) => Msg::Success(query),
                        Err(error) => Msg::Failed(error.as_string().unwrap()),
                    }
                });
                false
            }
            Msg::Success(query) => {
                self.query = Some(query);
                self.error = None;
                true
            }
            Msg::Failed(error) => {
                self.error = Some(error);
                true
            }
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct TableProps {
    query: ItemQuery,
}

struct Table;

impl Component for Table {
    type Message = ();
    type Properties = TableProps;

    fn create(_: &Context<Self>) -> Self {
        Table
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let query = &ctx.props().query;
        html! {
            <div class="table-responsive">
                <table class="table table-striped">
                    <thead>
                        <tr>
                            <th>{"#"}</th>
                            {for query.fields.iter().map(|item| html! {
                                <th>{item}</th>
                            })}
                        </tr>
                    </thead>
                    <tbody>{for query.items.iter().zip(1..).map(|(item, i)| html! {
                        <Row i={i} values={item.values.clone()}/>
                    })}</tbody>
                </table>
            </div>
        }
    }
}

#[derive(Eq, PartialEq, Properties)]
pub struct RowProps {
    i: i32,
    values: Vec<String>,
}

struct Row;

impl Component for Row {
    type Message = ();
    type Properties = RowProps;

    fn create(_: &Context<Self>) -> Self {
        Row
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
          <tr>
            <th>{ctx.props().i}</th>
            {for ctx.props().values.iter().map(|item| html! {
                <td>{item}</td>
            })}
          </tr>
        }
    }
}
