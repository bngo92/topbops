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
use yew_router::{BrowserRouter, Routable, Switch};

mod random;
pub mod tournament;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/lists/:id/tournament")]
    Tournament { id: String },
}

fn switch(routes: &Route) -> Html {
    match routes {
        Route::Home => html! { <Landing/> },
        Route::Tournament { id } => html! {
          <div>
            <nav class="navbar navbar-dark bg-dark">
              <div id="navbar" class="container-lg">
                <a id="brand" class="navbar-brand" href="_blank">{"Bops to the Top"}</a>
              </div>
            </nav>
            <div class="container-lg my-md-4">
              <Tournament id={id.clone()}/>
            </div>
          </div>
        },
    }
}

struct App;

impl Component for App {
    type Message = ();
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        App
    }

    fn view(&self, _: &Context<Self>) -> Html {
        html! {
            <BrowserRouter>
                <Switch<Route> render={Switch::render(switch)} />
            </BrowserRouter>
        }
    }
}

enum Msg {
    FetchHome(String),
    LoadHome(String, Vec<(List, ItemQuery)>),
    FetchRandom(String, String),
    LoadRandom(String, String, ItemQuery, Mode),
    UpdateStats((String, String, String, String), Mode),
}

struct Landing {
    current_page: Page,
    random_queue: Vec<topbops::Item>,
    left: Option<ItemMetadata>,
    right: Option<ItemMetadata>,
}

impl Component for Landing {
    type Message = Msg;
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        Landing {
            current_page: Page::Login,
            random_queue: Vec::new(),
            left: None,
            right: None,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let page = match &self.current_page {
            Page::Login => html! {
                <Login on_demo_select={ctx.link().callback(|_| Msg::FetchHome(String::from("demo")))}/>
            },
            Page::Home(lists) => {
                html! {
                    <Home lists={lists.clone()}/>
                }
            }
            Page::Random(_, list, _, Mode::Tournament) => {
                html! {
                    <Tournament id={list.clone()}/>
                }
            }
            Page::Random(user, list, query, mode) => {
                let left = self.left.clone().unwrap();
                let right = self.right.clone().unwrap();
                let left_param = (
                    user.clone(),
                    list.clone(),
                    left.id.clone(),
                    right.id.clone(),
                );
                let right_param = (
                    user.clone(),
                    list.clone(),
                    right.id.clone(),
                    left.id.clone(),
                );
                let mode = *mode;
                let mode_string = match mode {
                    Mode::Match => String::from("Random Matches"),
                    Mode::Round => String::from("Random Rounds"),
                    Mode::Tournament => String::from("Tournament"),
                };
                html! {
                    <Random mode={mode_string} left={left} on_left_select={ctx.link().callback_once(move |_| Msg::UpdateStats(left_param, mode))} right={right} on_right_select={ctx.link().callback_once(move |_| Msg::UpdateStats(right_param, mode))} query={query.clone()}/>
                }
            }
        };
        html! {
          <div>
            <nav class="navbar navbar-dark bg-dark">
              <div id="navbar" class="container-lg">
                <a id="brand" class="navbar-brand" href="_blank" onclick={ctx.link().callback(|_| Msg::FetchHome(String::from("demo")))}>{"Bops to the Top"}</a>
              </div>
            </nav>
            <div class="container-lg my-md-4">
              {page}
            </div>
          </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::FetchHome(user) => {
                ctx.link().send_future(async move {
                    let lists = fetch_lists(&user).await.unwrap();
                    let lists = futures::future::join_all(lists.into_iter().map(|list| async {
                        let query = query_items(&user, &list.id).await?;
                        Ok((list, query))
                    }))
                    .await
                    .into_iter()
                    .collect::<Result<_, JsValue>>()
                    .unwrap();
                    Msg::LoadHome(user, lists)
                });
                false
            }
            Msg::LoadHome(user, lists) => {
                self.current_page = Page::Home(
                    lists
                        .into_iter()
                        .map(|(data, query)| {
                            let user = user.clone();
                            let list = data.id.clone();
                            ListData {
                                data,
                                query,
                                on_go_select: ctx
                                    .link()
                                    .callback_once(move |_| Msg::FetchRandom(user, list)),
                            }
                        })
                        .collect(),
                );
                true
            }
            Msg::FetchRandom(user, list) => {
                let window = web_sys::window().expect("no global `window` exists");
                let document = window.document().expect("should have a document on window");
                let mode = document
                    .get_element_by_id("mode")
                    .unwrap()
                    .dyn_into::<HtmlSelectElement>()
                    .unwrap()
                    .value();
                let mode = match mode.as_ref() {
                    "Random Matches" => Mode::Match,
                    "Random Rounds" => Mode::Round,
                    "Tournament" => Mode::Tournament,
                    _ => {
                        web_sys::console::log_1(&JsValue::from("Invalid mode"));
                        return false;
                    }
                };
                self.random_queue.clear();
                ctx.link().send_future(async move {
                    let query = query_items(&user, &list).await.unwrap();
                    Msg::LoadRandom(user, list, query, mode)
                });
                false
            }
            Msg::LoadRandom(user, list, query, mode) => {
                self.current_page = Page::Random(user, list, query.clone(), mode);
                match mode {
                    Mode::Round => {
                        match self.random_queue.len() {
                            // Reload the queue if it's empty
                            0 => {
                                let mut items = query.items;
                                items.shuffle(&mut rand::thread_rng());
                                self.random_queue.extend(items);
                            }
                            // Always queue the last song next before reloading
                            1 => {
                                let last = self.random_queue.pop().unwrap();
                                let mut items = query.items;
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
                    Mode::Tournament => {}
                }
                true
            }
            Msg::UpdateStats((user, list, win, lose), mode) => {
                ctx.link().send_future(async move {
                    update_stats(&user, &list, &win, &lose).await.unwrap();
                    let query = query_items(&user, &list).await.unwrap();
                    Msg::LoadRandom(user, list, query, mode)
                });
                false
            }
        }
    }
}

#[derive(PartialEq)]
enum Page {
    Login,
    Home(Vec<ListData>),
    Random(String, String, ItemQuery, Mode),
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Match,
    Round,
    Tournament,
}

#[derive(PartialEq, Properties)]
pub struct LoginProps {
    on_demo_select: Callback<MouseEvent>,
}

pub struct Login;

impl Component for Login {
    type Message = ();
    type Properties = LoginProps;

    fn create(_: &Context<Self>) -> Self {
        Login
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
          <div>
            <div class="row justify-content-center">
              <button type="button" id="login" class="col-2 btn btn-success">{"Login with Spotify"}</button>
            </div>
            <div class="row justify-content-center">
              <button type="button" id="demo" class="col-2 btn btn-outline-success" onclick={ctx.props().on_demo_select.clone()}>{"Demo"}</button>
            </div>
          </div>
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct HomeProps {
    lists: Vec<ListData>,
}

pub struct Home;

impl Component for Home {
    type Message = ();
    type Properties = HomeProps;

    fn create(_: &Context<Self>) -> Self {
        Home
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
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
              {ctx.props().lists.iter().map(|l| html! {<Widget list={l.clone()}/>}).collect::<Vec<_>>()}
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
