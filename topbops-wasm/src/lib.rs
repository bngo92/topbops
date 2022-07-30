#![feature(async_closure, let_else)]
use crate::random::Random;
use crate::tournament::Tournament;
use rand::prelude::SliceRandom;
use topbops::{ItemMetadata, ItemQuery, List, Lists};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlSelectElement, Request, RequestInit, RequestMode, Response};
use yew::{html, Callback, Component, Context, Html, MouseEvent, Properties};
use yew_router::history::History;
use yew_router::prelude::Link;
use yew_router::scope_ext::RouterScopeExt;
use yew_router::{BrowserRouter, Routable, Switch};

mod random;
pub mod tournament;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Login,
    #[at("/home")]
    Home,
    #[at("/lists/:id/match")]
    Match { id: String },
    #[at("/lists/:id/tournament")]
    Tournament { id: String },
}

fn switch(routes: &Route) -> Html {
    match routes {
        Route::Login => html! { <Login/> },
        Route::Home => html! { <Home/> },
        Route::Match { id } => html! { <Match id={id.clone()}/> },
        Route::Tournament { id } => html! {
            <Tournament id={id.clone()}/>
        },
    }
}

struct App;

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        App
    }

    fn view(&self, _: &Context<Self>) -> Html {
        html! {
            <div>
                <nav class="navbar navbar-dark bg-dark">
                    <div id="navbar" class="container-lg">
                        <Link<Route> classes="navbar-brand" to={Route::Home}>{"Bops to the Top"}</Link<Route>>
                    </div>
                </nav>
                <div class="container-lg my-md-4">
                    <BrowserRouter>
                        <Switch<Route> render={Switch::render(switch)} />
                    </BrowserRouter>
                </div>
            </div>
        }
    }
}

enum Msg {
    LoadRandom(ItemQuery, Mode),
    UpdateStats((String, String, String, String), Mode),
}

#[derive(Clone, PartialEq, Properties)]
struct MatchProps {
    id: String,
}

struct Match {
    random_queue: Vec<topbops::Item>,
    left: Option<ItemMetadata>,
    right: Option<ItemMetadata>,
    query: Option<ItemQuery>,
}

impl Component for Match {
    type Message = Msg;
    type Properties = MatchProps;

    fn create(ctx: &Context<Self>) -> Self {
        let mode = Mode::Match;
        let id = ctx.props().id.clone();
        ctx.link().send_future(async move {
            let user = String::from("demo");
            let query = query_items(&user, &id).await.unwrap();
            Msg::LoadRandom(query, mode)
        });
        Match {
            random_queue: Vec::new(),
            left: None,
            right: None,
            query: None,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        if let Some(query) = &self.query {
            let user = String::from("demo");
            let list = &ctx.props().id;
            let left = self.left.clone().unwrap();
            let right = self.right.clone().unwrap();
            let left_param = (
                user.clone(),
                list.clone(),
                left.id.clone(),
                right.id.clone(),
            );
            let right_param = (user, list.clone(), right.id.clone(), left.id.clone());
            let mode = Mode::Match;
            let mode_string = match mode {
                Mode::Match => String::from("Random Matches"),
                Mode::Round => String::from("Random Rounds"),
            };
            html! {
                <Random mode={mode_string} left={left} on_left_select={ctx.link().callback_once(move |_| Msg::UpdateStats(left_param, mode))} right={right} on_right_select={ctx.link().callback_once(move |_| Msg::UpdateStats(right_param, mode))} query={query.clone()}/>
            }
        } else {
            html! {}
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadRandom(query, mode) => {
                match mode {
                    Mode::Round => {
                        match self.random_queue.len() {
                            // Reload the queue if it's empty
                            0 => {
                                let mut items = query.items.clone();
                                items.shuffle(&mut rand::thread_rng());
                                self.random_queue.extend(items);
                            }
                            // Always queue the last song next before reloading
                            1 => {
                                let last = self.random_queue.pop().unwrap();
                                let mut items = query.items.clone();
                                items.shuffle(&mut rand::thread_rng());
                                self.random_queue.extend(items);
                                self.random_queue.push(last);
                            }
                            _ => {}
                        }
                        self.left = self.random_queue.pop().unwrap().metadata;
                        self.right = self.random_queue.pop().unwrap().metadata;
                    }
                    Mode::Match => {
                        let mut queued_scores: Vec<_> = query.items.iter().collect();
                        queued_scores.shuffle(&mut rand::thread_rng());
                        self.left = queued_scores.pop().unwrap().metadata.clone();
                        self.right = queued_scores.pop().unwrap().metadata.clone();
                    }
                }
                self.query = Some(query);
                true
            }
            Msg::UpdateStats((user, list, win, lose), mode) => {
                ctx.link().send_future(async move {
                    update_stats(&user, &list, &win, &lose).await.unwrap();
                    let query = query_items(&user, &list).await.unwrap();
                    Msg::LoadRandom(query, mode)
                });
                false
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Match,
    Round,
}

pub struct Login;

impl Component for Login {
    type Message = ();
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        Login
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let history = ctx.link().history().unwrap();
        let onclick = Callback::once(move |_| history.push(Route::Home));
        html! {
          <div>
            <div class="row justify-content-center">
              <button type="button" class="col-2 btn btn-success">{"Login with Spotify"}</button>
            </div>
            <div class="row justify-content-center">
              <button type="button" class="col-2 btn btn-outline-success" {onclick}>{"Demo"}</button>
            </div>
          </div>
        }
    }
}

pub enum HomeMsg {
    Load(Vec<ListData>),
}

pub struct Home {
    lists: Vec<ListData>,
}

impl Component for Home {
    type Message = HomeMsg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let history = ctx.link().history().unwrap();
        ctx.link().send_future(async move {
            let user = "demo";
            let lists = fetch_lists(&user).await.unwrap();
            let lists = futures::future::join_all(lists.into_iter().map(|list| async {
                let query = query_items(&user, &list.id).await?;
                let history = history.clone();
                let id = list.id.clone();
                let onclick = Callback::once(move |_| {
                    let window = web_sys::window().expect("no global `window` exists");
                    let document = window.document().expect("should have a document on window");
                    let mode = document
                        .get_element_by_id("mode")
                        .unwrap()
                        .dyn_into::<HtmlSelectElement>()
                        .unwrap()
                        .value();
                    match mode.as_ref() {
                        "Random Matches" => {
                            history.push(Route::Match { id });
                        }
                        "Random Rounds" => {
                            history.push(Route::Match { id });
                        }
                        "Tournament" => {
                            history.push(Route::Tournament { id });
                        }
                        _ => {
                            web_sys::console::log_1(&JsValue::from("Invalid mode"));
                        }
                    };
                });
                Ok(ListData {
                    data: list,
                    query,
                    on_go_select: onclick,
                })
            }))
            .await
            .into_iter()
            .collect::<Result<_, JsValue>>()
            .unwrap();
            HomeMsg::Load(lists)
        });
        Home { lists: Vec::new() }
    }

    fn view(&self, _: &Context<Self>) -> Html {
        html! {
          <div class="container-lg my-md-4">
            <div class="row">
              <div class="col-8">
                <h1>{"Home"}</h1>
              </div>
              <label class="col-2 col-form-label text-end align-self-end">
                <strong>{"Sort Mode:"}</strong>
              </label>
              <div class="col-2 align-self-end">
                <select id="mode" class="form-select">
                  <option>{"Random Matches"}</option>
                  <option>{"Random Rounds"}</option>
                  <option>{"Tournament"}</option>
                </select>
              </div>
            </div>
            <div class="row">
              {self.lists.iter().map(|l| html! {<Widget list={l.clone()}/>}).collect::<Vec<_>>()}
            </div>
            <h1>{"My Spotify Playlists"}</h1>
            <div></div>
            <form>
              <div class="row">
                <div class="col-9 pt-1">
                  <input type="text" id="input" class="col-12" value="https://open.spotify.com/playlist/5MztFbRbMpyxbVYuOSfQV9?si=9db089ab25274efa"/>
                </div>
                <div class="col-1 pe-2">
                  <button type="button" class="col-12 btn btn-success">{"Save"}</button>
                </div>
              </div>
            </form>
          </div>
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        let HomeMsg::Load(lists) = msg;
        self.lists = lists;
        true
    }
}

#[derive(PartialEq, Properties)]
pub struct WidgetProps {
    list: ListData,
}

struct Widget;

impl Component for Widget {
    type Message = ();
    type Properties = WidgetProps;

    fn create(_: &Context<Self>) -> Self {
        Widget
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let list = &ctx.props().list;
        html! {
          <div class="col-6">
            <div class="row">
              <div class="col-8">
                <h2>{&list.data.name}</h2>
              </div>
              <div class="col-2">
                <button type="button" class="btn btn-success col-12" onclick={list.on_go_select.clone()}>{"Go"}</button>
              </div>
              <div class="col-2">
                <button type="button" class="btn btn-danger col-12">{"Unsave"}</button>
              </div>
            </div>
            <table class="table table-striped">
              <thead>
                <tr>
                  <th class="col-1">{"#"}</th>
                  <th class="col-8">{&list.query.fields[0]}</th>
                  <th>{&list.query.fields[1]}</th>
                </tr>
              </thead>
              <tbody>{for list.query.items.iter().zip(1..).map(|(item, i)| html! {
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
            <td>{&ctx.props().values[0]}</td>
            <td>{&ctx.props().values[1]}</td>
          </tr>
        }
    }
}

#[derive(Clone, PartialEq, Properties)]
pub struct ListData {
    data: List,
    query: ItemQuery,
    on_go_select: Callback<MouseEvent>,
}

// Called by our JS entry point to run the example
#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue> {
    yew::start_app::<App>();
    Ok(())
}

async fn fetch_lists(auth: &str) -> Result<Vec<List>, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query("/api/lists", "GET", auth)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let lists: Lists = json.into_serde().unwrap();
    Ok(lists.lists)
}

async fn query_items(auth: &str, id: &str) -> Result<ItemQuery, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/lists/{}/items", id), "GET", auth).unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(json.into_serde().unwrap())
}

async fn update_stats(auth: &str, list: &str, win: &str, lose: &str) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(
        &format!(
            "/api/?action=update&list={}&win={}&lose={}",
            list, win, lose
        ),
        "POST",
        auth,
    )?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

fn query(url: &str, method: &str, auth: &str) -> Result<Request, JsValue> {
    let mut opts = RequestInit::new();
    opts.method(method);
    opts.mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(url, &opts)?;
    request
        .headers()
        .set("Authorization", &format!("Basic {}", auth))?;
    Ok(request)
}
