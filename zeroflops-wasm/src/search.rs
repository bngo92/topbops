use crate::{base::Input, plot::DataView};
use polars::prelude::{CsvWriter, DataFrame, SerWriter};
use std::collections::HashMap;
use web_sys::{HtmlSelectElement, KeyboardEvent};
use yew::{html, Component, Context, Html, NodeRef};
use yew_router::scope_ext::RouterScopeExt;

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
                        <h1 class="col-10 m-0">{"Query"}</h1>
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
    Success(DataFrame),
    Failed(String),
    Select,
}

pub struct SearchPane {
    search_ref: NodeRef,
    query: Option<DataFrame>,
    error: Option<String>,
    format: Format,
    select_ref: NodeRef,
    view: DataView,
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
            select_ref: NodeRef::default(),
            view: DataView::Table,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let default_search = Some("SELECT name, user_score FROM tracks");
        let onchange = ctx.link().callback(|_| Msg::Select);
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
            <div class="row">
                if let Format::Table = self.format {
                    <div class="col-auto">
                        <select ref={self.select_ref.clone()} class="form-select mb-3" {onchange}>
                            <option selected=true>{"Table"}</option>
                            <option>{"Column Graph"}</option>
                            <option>{"Line Graph"}</option>
                            <option>{"Scatter Plot"}</option>
                            <option>{"Cumulative Line Graph"}</option>
                        </select>
                    </div>
                }
                <form {onkeydown}>
                    <Input input_ref={self.search_ref.clone()} default={default_search} onclick={search.clone()} error={self.error.clone()} disabled={false}/>
                </form>
                if let Some(query) = &self.query {
                    if let Format::Table = self.format {
                        {self.view.render(query)}
                    } else if let Format::Csv = self.format {
                        <p>{write_csv(query)
                            .lines()
                                .map(|items| html! {items})
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
                return false;
            }
            Msg::Success(query) => {
                self.query = Some(query);
                self.error = None;
            }
            Msg::Failed(error) => {
                self.error = Some(error);
            }
            Msg::Select => {
                let view = self.select_ref.cast::<HtmlSelectElement>().unwrap().value();
                self.view = match &*view {
                    "Table" => DataView::Table,
                    "Column Graph" => DataView::ColumnGraph,
                    "Line Graph" => DataView::LineGraph,
                    "Scatter Plot" => DataView::ScatterPlot,
                    "Cumulative Line Graph" => DataView::CumLineGraph,
                    _ => unreachable!(),
                };
            }
        }
        if let Some(df) = &self.query {
            if let Err(e) = self.view.draw(df) {
                self.error = Some(e.to_string());
            }
        }
        true
    }
}

fn write_csv(df: &DataFrame) -> String {
    let mut buffer = Vec::new();
    let mut df = df.clone();
    CsvWriter::new(&mut buffer)
        .has_header(false)
        .finish(&mut df)
        .unwrap();
    String::from_utf8(buffer).unwrap()
}
