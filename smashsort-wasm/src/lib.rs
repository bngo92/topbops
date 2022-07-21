#![feature(async_closure)]
use rand::prelude::SliceRandom;
use smashsort::{List, Lists, QueryResponse};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};
use yew::{html, Callback, Component, Context, Html, MouseEvent, Properties};

pub struct App;

impl Component for App {
    type Message = ();
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        App
    }

    fn view(&self, _: &Context<Self>) -> Html {
        html! {
          <div>
            <nav class="navbar navbar-dark bg-dark">
              <div id="navbar" class="container-lg">
                <a id="brand" class="navbar-brand" href="#">{"Smashsort"}</a>
              </div>
            </nav>
            <State/>
          </div>
        }
    }
}

pub enum StateMsg {
    LoadUser(String),
    LoadHome(Vec<(List, QueryResponse)>),
}

pub struct State {
    current_page: Page,
    home_data: Vec<(List, QueryResponse)>,
}

impl Component for State {
    type Message = StateMsg;
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        State {
            current_page: Page::Login,
            home_data: Vec::new(),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        if let Page::Login = self.current_page {
            html! {
                <Login on_demo_select={ctx.link().callback(|_| StateMsg::LoadUser(String::from("demo")))}/>
            }
        } else {
            html! {
                <Home data={self.home_data.clone()}/>
            }
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            StateMsg::LoadUser(user) => {
                ctx.link().send_future(async move {
                    let lists = fetch_lists(&user).await.unwrap();
                    let items =
                        futures::future::join_all(lists.iter().map(|l| fetch_items(&user, &l.id)))
                            .await
                            .into_iter()
                            .collect::<Result<Vec<_>, _>>()
                            .unwrap();
                    StateMsg::LoadHome(lists.into_iter().zip(items).collect())
                });
                false
            }
            StateMsg::LoadHome(home_data) => {
                self.current_page = Page::Home;
                self.home_data = home_data;
                true
            }
        }
    }
}

#[derive(PartialEq)]
enum Page {
    Login,
    Home,
    Random(Random, String),
}

#[derive(PartialEq)]
enum Random {
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
          <div class="container-lg my-md-4">
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
    data: Vec<(List, QueryResponse)>,
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
              {ctx.props().data.iter().map(|r| html! {<Widget data={r.clone()}/>}).collect::<Vec<_>>()}
            </div>
            <h1>{"My Spotify Playlists"}</h1>
            <div></div>
            <form>
              <div class="row">
                <div class="col-9 pt-1">
                  <input type="text" id="input" class="col-12" value="https://open.spotify.com/playlist/5jPjYAdQO0MgzHdwSmYPNZ?si=304cfe5d16ce4afd"/>
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
    data: (List, QueryResponse),
}

struct Widget;

impl Component for Widget {
    type Message = ();
    type Properties = WidgetProps;

    fn create(_: &Context<Self>) -> Self {
        Widget
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let (list, response) = &ctx.props().data;
        html! {
          <div class="col-6">
            <div class="row">
              <div class="col-8">
                <h2>{&list.name}</h2>
              </div>
              <div class="col-2">
                <button type="button" class="btn btn-success col-12">{"Go"}</button>
              </div>
              <div class="col-2">
                <button type="button" class="btn btn-danger col-12">{"Unsave"}</button>
              </div>
            </div>
            <table class="table table-striped">
              <thead>
                <tr>
                  <th class="col-1">{"#"}</th>
                  <th class="col-8">{&response.header[0]}</th>
                  <th>{&response.header[1]}</th>
                </tr>
              </thead>
              <tbody>{for response.items.iter().zip(1..).map(|(row, i)| html! {
                <Row i={i} row={row.clone()}/>
              })}</tbody>
            </table>
          </div>
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct RowProps {
    i: i32,
    row: Vec<String>,
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
            <td>{&ctx.props().row[0]}</td>
            <td>{&ctx.props().row[1]}</td>
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
    let document = window.document().expect("should have a document on window");
    let request = query("/api/lists", "GET", auth)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let lists: Lists = json.into_serde().unwrap();
    Ok(lists.items)
}

async fn fetch_items(auth: &str, id: &str) -> Result<QueryResponse, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/lists/{}/items", id), "GET", auth).unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(json.into_serde().unwrap())
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
