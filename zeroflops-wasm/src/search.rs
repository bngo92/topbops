use crate::{bootstrap::Collapse, dataframe::DataFrame, plot::DataView};
use web_sys::{HtmlSelectElement, KeyboardEvent};
use yew::{html, Component, Context, Html, NodeRef, Properties};

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

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            SearchMsg::ToggleHelp => self.help_collapsed = !self.help_collapsed,
            SearchMsg::Toggle => self.split_view = !self.split_view,
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onclick = ctx.link().callback(|_| SearchMsg::Toggle);
        let button_text = if self.split_view {
            "Single View"
        } else {
            "Split View"
        };
        crate::nav_content(
            html! {
              <>
                <ul class="navbar-nav me-auto">
                  <li class="navbar-brand">{"Query"}</li>
                </ul>
                <div class="d-flex gap-3">
                  <button type="button" class="btn btn-info" style="width: 112px" {onclick}>{button_text}</button>
                  <button class="btn btn-info" onclick={ctx.link().callback(|_| SearchMsg::ToggleHelp)}>{"Help"}</button>
                </div>
              </>
            },
            html! {
              <>
                <div class="mb-3">
                  <Collapse collapsed={self.help_collapsed}>
                    <p>{"Run SQL queries to transform your data into insights.
                        All queries should run against the \"c\" table."}</p>
                    <p><strong>{"Example Queries"}</strong></p>
                    <p>{"Get names of songs that have more tournament and match wins than losses:"}</p>
                    <code>{"SELECT name, user_wins, user_losses FROM item WHERE type='track' AND user_wins > user_losses"}</code>
                    <p>{"Get names of songs ordered by your scores:"}</p>
                    <code>{"SELECT name, user_score FROM item WHERE type='track' ORDER BY user_score DESC"}</code>
                    <p>{"Count how many songs were performed by each distinct group of artists:"}</p>
                    <code>{"SELECT artists, COUNT(1) FROM item WHERE type='track' GROUP BY artists"}</code>
                    <p>{"Get songs performed by Troy:"}</p>
                    <code>{"SELECT name, artists FROM item, json_each(metadata->'artists') WHERE json_each.value='Troy'"}</code>
                    <p>{"Get your average score for each group of artists:"}</p>
                    <code>{"SELECT artists, AVG(user_score) FROM item WHERE type='track' GROUP BY artists"}</code>
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
                  <div class="d-flex gap-3">
                    <SearchPane/>
                    <SearchPane/>
                  </div>
                } else {
                  <div style="max-width: 1000px">
                    <SearchPane/>
                  </div>
                }
              </>
            },
        )
    }
}

pub enum Msg {
    None,
    Fetching,
    Success(Option<DataFrame>),
    Failed(String),
    Select,
    CreateList,
}

pub struct SearchPane {
    search_ref: NodeRef,
    query: Option<DataFrame>,
    error: Option<String>,
    select_ref: NodeRef,
    view: DataView,
}

impl Component for SearchPane {
    type Message = Msg;
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        SearchPane {
            search_ref: NodeRef::default(),
            query: None,
            error: None,
            select_ref: NodeRef::default(),
            view: DataView::Table,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => {}
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
                    "CSV" => DataView::Csv,
                    _ => unreachable!(),
                };
            }
            Msg::CreateList => {
                let input = self.search_ref.cast::<HtmlSelectElement>().unwrap().value();
                ctx.link().send_future(async move {
                    match crate::create_list(Some(input)).await {
                        Ok(_) => Msg::None,
                        Err(error) => Msg::Failed(error.as_string().unwrap()),
                    }
                });
                return false;
            }
        }
        if let Some(df) = &self.query {
            if let Err(e) = self.view.draw(df) {
                self.error = Some(e.to_string());
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let default_search = Some("SELECT name, user_score FROM item");
        let onchange = ctx.link().callback(|_| Msg::Select);
        let search = ctx.link().callback(|_| Msg::Fetching);
        let create = ctx.link().callback(|_| Msg::CreateList);
        let onkeydown = ctx.link().batch_callback(|event: KeyboardEvent| {
            if event.key_code() == 13 {
                event.prevent_default();
                Some(Msg::Fetching)
            } else {
                None
            }
        });
        let (class, error) = if let Some(error) = &self.error {
            (
                "flex-grow-1 is-invalid",
                Some(html! {<div class="invalid-feedback">{error}</div>}),
            )
        } else {
            ("flex-grow-1", None)
        };
        html! {
            <div class="row w-100">
                <div class="col-auto">
                    <select ref={self.select_ref.clone()} class="form-select mb-3" {onchange}>
                        <option selected=true>{"Table"}</option>
                        <option>{"Column Graph"}</option>
                        <option>{"Line Graph"}</option>
                        <option>{"Scatter Plot"}</option>
                        <option>{"Cumulative Line Graph"}</option>
                        <option>{"CSV"}</option>
                    </select>
                </div>
                <form {onkeydown}>
                    <div class="d-flex gap-2">
                        <div class="flex-grow-1">
                            // Copy only the styles from .form-control that are needed for sizing
                            <input ref={self.search_ref.clone()} type="text" {class} style="padding: .5rem 1rem; font-size: .875rem; border-width: 1px; min-width: 100%" placeholder={default_search}/>
                            if let Some(error) = error {
                                {error}
                            }
                        </div>
                        <button type="button" class="btn btn-success" onclick={search.clone()} style="height: fit-content">{"Search"}</button>
                        <button type="button" class="btn btn-success" onclick={create.clone()} style="height: fit-content" disabled={self.query.is_none()}>{"Create List"}</button>
                    </div>
                </form>
                if let Some(query) = &self.query {
                    {self.view.render(query)}
                }
            </div>
        }
    }
}
