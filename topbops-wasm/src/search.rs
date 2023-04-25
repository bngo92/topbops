use std::collections::HashMap;
use topbops::ItemQuery;
use web_sys::HtmlSelectElement;
use yew::{html, Component, Context, Html, NodeRef, Properties};
use yew_router::scope_ext::RouterScopeExt;

pub enum Msg {
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
                "col-12 is-invalid",
                Some(html! {<div class="invalid-feedback">{error}</div>}),
            )
        } else {
            ("col-12", None)
        };
        html! {
            <div>
                <form>
                    <div class="row">
                        <div class="col-12 col-md-10 col-xl-11 pt-1">
                            <input ref={self.search_ref.clone()} type="text" {class} placeholder={default_search}/>
                            {for error}
                        </div>
                        <div class="col-3 col-sm-2 col-md-2 col-xl-1 pe-2">
                            <button type="button" class="col-12 btn btn-success" onclick={search}>{"Search"}</button>
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
