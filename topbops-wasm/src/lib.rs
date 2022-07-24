#![feature(async_closure)]
use rand::prelude::SliceRandom;
use topbops::{ItemMetadata, ItemQuery, List, Lists};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};
use yew::{html, Callback, Component, Context, Html, MouseEvent, Properties};

pub enum Msg {
    FetchHome(String),
    LoadHome(String, Vec<(List, ItemQuery)>),
    LoadRandom(String, String, ItemQuery),
    UpdateStats((String, String, String, String)),
}

pub struct App {
    current_page: Page,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        App {
            current_page: Page::Login,
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
            Page::Random(user, list, query, _) => {
                let mut queued_scores: Vec<_> = query.items.iter().collect();
                queued_scores.shuffle(&mut rand::thread_rng());
                let left = queued_scores.pop().unwrap().metadata.clone().unwrap();
                let right = queued_scores.pop().unwrap().metadata.clone().unwrap();
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
                html! {
                    <Random left={left} on_left_select={ctx.link().callback_once(|_| Msg::UpdateStats(left_param))} right={right} on_right_select={ctx.link().callback_once(|_| Msg::UpdateStats(right_param))} query={query.clone()}/>
                }
            }
        };
        html! {
          <div>
            <nav class="navbar navbar-dark bg-dark">
              <div id="navbar" class="container-lg">
                <a id="brand" class="navbar-brand" href="#" onclick={ctx.link().callback(|_| Msg::FetchHome(String::from("demo")))}>{"Bops to the Top"}</a>
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
                                query: query.clone(),
                                on_go_select: ctx
                                    .link()
                                    .callback_once(move |_| Msg::LoadRandom(user, list, query)),
                            }
                        })
                        .collect(),
                );
                true
            }
            Msg::LoadRandom(user, list, query) => {
                self.current_page = Page::Random(user, list, query, RandomMode::Match);
                true
            }
            Msg::UpdateStats((user, list, win, lose)) => {
                ctx.link().send_future(async move {
                    update_stats(&user, &list, &win, &lose).await.unwrap();
                    let query = query_items(&user, &list).await.unwrap();
                    Msg::LoadRandom(user, list, query)
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
    Random(String, String, ItemQuery, RandomMode),
}

#[derive(PartialEq)]
enum RandomMode {
    Match,
    Round,
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
              <div id="mode" class="col-2 align-self-end">
                <select class="form-select">
                  <option>{"Random Matches"}</option>
                  <option>{"Random Rounds"}</option>
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

#[derive(PartialEq, Properties)]
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

#[derive(PartialEq, Properties)]
pub struct RandomProps {
    left: ItemMetadata,
    on_left_select: Callback<MouseEvent>,
    right: ItemMetadata,
    on_right_select: Callback<MouseEvent>,
    query: ItemQuery,
}

struct Random;

impl Component for Random {
    type Message = ();
    type Properties = RandomProps;

    fn create(_: &Context<Self>) -> Self {
        Random
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let RandomProps {
            left,
            right,
            query,
            on_left_select,
            on_right_select,
        } = ctx.props();
        let (left_items, right_items): (Vec<_>, Vec<_>) = query
            .items
            .iter()
            .zip(1..)
            .map(|(item, i)| {
                (
                    i,
                    html! {<Item i={i} item={item.metadata.clone().unwrap()}/>},
                )
            })
            .partition(|(i, _)| i % 2 == 1);
        let left_items = left_items.into_iter().map(|(_, item)| item);
        let right_items = right_items.into_iter().map(|(_, item)| item);
        html! {
          <div>
            <h1>{"Random Matches"}</h1>
            <div class="row">
              <div class="col-6">
                <iframe id="iframe1" width="100%" height="380" frameborder="0" src={left.iframe.clone()}></iframe>
                <button type="button" class="btn btn-info width" onclick={on_left_select.clone()}>{&left.name}</button>
              </div>
              <div class="col-6">
                <iframe id="iframe2" width="100%" height="380" frameborder="0" src={right.iframe.clone()}></iframe>
                <button type="button" class="btn btn-warning width" onclick={on_right_select.clone()}>{&right.name}</button>
              </div>
            </div>
            <div class="row">
              <div class="col-6">
                <table class="table table-striped">
                  <thead>
                    <tr>
                      <th class="col-1">{"#"}</th>
                      <th class="col-8">{"Track"}</th>
                      <th>{"Record"}</th>
                      <th>{"Score"}</th>
                    </tr>
                  </thead>
                  <tbody>{for left_items}</tbody>
                </table>
              </div>
              <div class="col-6">
                <table class="table table-striped">
                  <thead>
                    <tr>
                      <th class="col-1">{"#"}</th>
                      <th class="col-8">{"Track"}</th>
                      <th>{"Record"}</th>
                      <th>{"Score"}</th>
                    </tr>
                  </thead>
                  <tbody>{for right_items}</tbody>
                </table>
              </div>
            </div>
          </div>
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct ItemProps {
    i: i32,
    item: ItemMetadata,
}

struct Item;

impl Component for Item {
    type Message = ();
    type Properties = ItemProps;

    fn create(_: &Context<Self>) -> Self {
        Item
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        html! {
          <tr>
            <th>{props.i}</th>
            <td>{&props.item.name}</td>
            <td>{format!("{}-{}", props.item.wins, props.item.losses)}</td>
            <td>{&props.item.score}</td>
          </tr>
        }
    }
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
