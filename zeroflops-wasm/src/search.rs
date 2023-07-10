use crate::base::Input;
use std::collections::HashMap;
use web_sys::{HtmlSelectElement, KeyboardEvent};
use yew::{html, Component, Context, Html, NodeRef, Properties};
use yew_router::scope_ext::RouterScopeExt;
use zeroflops::ItemQuery;

pub enum SearchMsg {
    Toggle,
}

pub struct Search {
    split_view: bool,
}

impl Component for Search {
    type Message = SearchMsg;
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        Search { split_view: false }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onclick = ctx.link().callback(|_| SearchMsg::Toggle);
        let button_text = if self.split_view {
            "Single View"
        } else {
            "Split View"
        };
        html! {
            <div>
                <div class="container-lg">
                    <div class="row align-items-end mb-3">
                        <h1 class="col-10 m-0">{"Search"}</h1>
                        <div class="col-2">
                            <button type="button" class="btn btn-info w-100" {onclick}>{button_text}</button>
                        </div>
                    </div>
                </div>
                if self.split_view {
                    <div class="container-fluid">
                        <div class="row">
                            <div class="col-6">
                                <SearchPane/>
                            </div>
                            <div class="col-6">
                                <SearchPane/>
                            </div>
                        </div>
                    </div>
                } else {
                    <div class="container-lg">
                        <SearchPane/>
                    </div>
                }
            </div>
        }
    }

    fn update(&mut self, _: &Context<Self>, _: Self::Message) -> bool {
        self.split_view = !self.split_view;
        true
    }
}

pub enum Msg {
    Fetching,
    Success(ItemQuery),
    Failed(String),
}

pub struct SearchPane {
    search_ref: NodeRef,
    query: Option<ItemQuery>,
    error: Option<String>,
    format: Format,
}

enum Format {
    Table,
    Csv,
}

impl Component for SearchPane {
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
        SearchPane {
            search_ref: NodeRef::default(),
            query: None,
            error: None,
            format,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let default_search = Some("SELECT name, user_score FROM tracks");
        let search = ctx.link().callback(|_| Msg::Fetching);
        let onkeydown = ctx.link().batch_callback(|event: KeyboardEvent| {
            if event.key_code() == 13 {
                event.prevent_default();
                Some(Msg::Fetching)
            } else {
                None
            }
        });
        html! {
            <div>
                <form {onkeydown}>
                    <Input input_ref={self.search_ref.clone()} default={default_search} onclick={search.clone()} error={self.error.clone()}/>
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
