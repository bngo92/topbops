use crate::{base::Input, bootstrap::Collapse, plot::DataView};
use polars::prelude::{CsvWriter, DataFrame, SerWriter};
use std::collections::HashMap;
use web_sys::{HtmlSelectElement, KeyboardEvent};
use yew::{html, Component, Context, Html, NodeRef, Properties};
use yew_router::scope_ext::RouterScopeExt;

pub enum SearchMsg {
    ToggleHelp,
    Toggle,
}

#[derive(PartialEq, Properties)]
pub struct SearchProps {
    pub logged_in: bool,
}

pub struct Search {
    help_collapsed: bool,
    split_view: bool,
}

impl Component for Search {
    type Message = SearchMsg;
    type Properties = SearchProps;

    fn create(ctx: &Context<Self>) -> Self {
        Search {
            help_collapsed: ctx.props().logged_in,
            split_view: false,
        }
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
                        <h1 class="col m-0">{"Query"}</h1>
                        <div class="col-2">
                            <button type="button" class="btn btn-info w-100" {onclick}>{button_text}</button>
                        </div>
                        <div class="col-auto">
                            <button class="btn btn-info" onclick={ctx.link().callback(|_| SearchMsg::ToggleHelp)}>{"Help"}</button>
                        </div>
                    </div>
                    <Collapse collapsed={self.help_collapsed}>
                        <p>{"Run SQL queries to transform your data into insights.
                            All queries should run against the \"c\" table."}</p>
                        <p><strong>{"Example Queries"}</strong></p>
                        <p>{"Get names of songs that have more tournament and match wins than losses:"}</p>
                        <code>{"SELECT name, user_wins, user_losses FROM c WHERE type='track' AND user_wins > user_losses"}</code>
                        <p>{"Get names of songs ordered by your scores:"}</p>
                        <code>{"SELECT name, user_score FROM c WHERE type='track' ORDER BY user_score DESC"}</code>
                        <p>{"Count how many songs were performed by each distinct group of artists:"}</p>
                        <code>{"SELECT artists, COUNT(1) FROM c WHERE type='track' GROUP BY artists"}</code>
                        <p>{"Get songs performed by Troy:"}</p>
                        <code>{"SELECT name, artists FROM c, json_each(metadata->'artists') WHERE json_each.value='Troy'"}</code>
                        <p>{"Get your average score for each group of artists:"}</p>
                        <code>{"SELECT artists, AVG(user_score) FROM c WHERE type='track' GROUP BY artists"}</code>
                        <p><strong>{"Fields"}</strong></p>
                        <p>{"The fields you can query on are listed below.
                            Here is the list of fields that are available for all items:"}</p>
                        <ul>
                            <li>{"type: string - The type of item"}</li>
                            <li>{"name: string - The name of the item"}</li>
                            <li>{"rating: number - The rating that you gave the item"}</li>
                            <li>{"user_score: number - Score computed from tournaments and matches"}</li>
                            <li>{"user_wins: number - Tournament and match wins"}</li>
                            <li>{"user_losses: number - Tournament and match losses"}</li>
                            <li>{"hidden: boolean - The item was hidden"}</li>
                        </ul>
                        <p>{"There are also fields that are specific to a single item type."}</p>
                        <p><em>{"Spotify Item Fields"}</em></p>
                        <p>{"Type is set to 'track' for Spotify items"}</p>
                        <ul>
                            <li>{"album: string - The name of the album that the track appears on"}</li>
                            <li>{"artists: array of string - The names of the artists who performed the track"}</li>
                            <li>{"duration_ms: number - The track length in milliseconds"}</li>
                            <li>{"popularity - Spotify popularity of the track"}</li>
                            <li>{"track_number - The number of the track"}</li>
                        </ul>
                    </Collapse>
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

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            SearchMsg::ToggleHelp => self.help_collapsed = !self.help_collapsed,
            SearchMsg::Toggle => self.split_view = !self.split_view,
        }
        true
    }
}

pub enum Msg {
    Fetching,
    Success(Option<DataFrame>),
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
        let default_search = Some("SELECT name, user_score FROM c");
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
                self.query = query;
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
