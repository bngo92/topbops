#![feature(async_closure, let_else)]
use crate::edit::Edit;
use crate::random::Match;
use crate::tournament::Tournament;
use regex::Regex;
use std::collections::HashMap;
use topbops::{ItemQuery, List, ListMode, Lists};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlDocument, HtmlSelectElement, Request, RequestInit, RequestMode, Response};
use yew::{html, Callback, Component, Context, Html, MouseEvent, NodeRef, Properties};
use yew_router::history::{AnyHistory, History};
use yew_router::prelude::Link;
use yew_router::scope_ext::RouterScopeExt;
use yew_router::{BrowserRouter, Routable, Switch};

mod base;
mod edit;
mod random;
pub mod tournament;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/lists/:id")]
    Edit { id: String },
    #[at("/lists/:id/match")]
    Match { id: String },
    #[at("/lists/:id/tournament")]
    Tournament { id: String },
}

fn switch(routes: &Route) -> Html {
    match routes {
        Route::Home => html! { <Home/> },
        Route::Edit { id } => html! { <Edit id={id.clone()}/> },
        Route::Match { id } => html! { <Match id={id.clone()}/> },
        Route::Tournament { id } => html! {
            <Tournament id={id.clone()}/>
        },
    }
}

/*enum Msg {
    Logout,
    Reload,
}*/

struct App;

impl Component for App {
    type Message = ();
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        App
    }

    fn view(&self, _: &Context<Self>) -> Html {
        let window = web_sys::window().expect("no global `window` exists");
        let location = window.location();
        //let onclick = ctx.link().callback(|_| Msg::Logout);
        html! {
            <div>
                <BrowserRouter>
                    <nav class="navbar navbar-dark bg-dark">
                        <div class="container-lg">
                            <Link<Route> classes="navbar-brand" to={Route::Home}>{"Bops to the Top"}</Link<Route>>
                            <ul class="navbar-nav">
                                <li class="nav-item">
                                    if let Some(user) = get_user() {
                                        <a class="nav-link" href="/api/logout">{format!("{} Logout", user)}</a>
                                    } else {
                                        <a class="nav-link" href={format!("https://accounts.spotify.com/authorize?client_id=ee3d1b4f8d80477ea48743a511ef3018&redirect_uri={}/api/login&response_type=code", location.origin().unwrap().as_str())}>{"Login"}</a>
                                    }
                                </li>
                            </ul>
                        </div>
                    </nav>
                    <div class="container-lg my-md-4">
                        <Switch<Route> render={Switch::render(switch)} />
                    </div>
                </BrowserRouter>
            </div>
        }
    }

    /*fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Logout => {
                ctx.link().clone().send_future(async move {
                    let window = web_sys::window().expect("no global `window` exists");
                    let request = query("/api/logout", "POST").unwrap();
                    JsFuture::from(window.fetch_with_request(&request))
                        .await
                        .unwrap();
                    Msg::Reload
                });
                false
            }
            Msg::Reload => true,
        }
    }*/
}

pub enum HomeMsg {
    Load(Vec<ListData>),
    Import,
}

pub struct Home {
    lists: Vec<ListData>,
    select_ref: NodeRef,
    import_ref: NodeRef,
}

impl Component for Home {
    type Message = HomeMsg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let history = ctx.link().history().unwrap();
        let select_ref = NodeRef::default();
        ctx.link()
            .send_future(Home::fetch_lists(select_ref.clone(), history));
        Home {
            lists: Vec::new(),
            select_ref,
            import_ref: NodeRef::default(),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let disabled = get_user().is_none();
        let default_import = if disabled {
            "Not supported in demo"
        } else {
            "https://open.spotify.com/playlist/5MztFbRbMpyxbVYuOSfQV9?si=9db089ab25274efa"
        };
        let import = ctx.link().callback(|_| HomeMsg::Import);
        html! {
          <div>
            <div class="row">
              <div class="col-8">
                <h1>{"Home"}</h1>
              </div>
              <label class="col-2 col-form-label text-end align-self-end">
                <strong>{"Sort Mode:"}</strong>
              </label>
              <div class="col-2 align-self-end">
                <select ref={self.select_ref.clone()} class="form-select">
                  <option>{"Tournament"}</option>
                  <option selected=true>{"Random Tournament"}</option>
                  <option>{"Random Matches"}</option>
                  <option>{"Random Rounds"}</option>
                </select>
              </div>
            </div>
            <div class="row">
              {for self.lists.iter().map(|l| html! {<Widget list={l.clone()}/>})}
            </div>
            <h1>{"My Spotify Playlists"}</h1>
            <div></div>
            <form>
              <div class="row">
                <div class="col-12 col-md-8 col-lg-9 pt-1">
                  <input ref={self.import_ref.clone()} type="text" class="col-12" value={default_import} {disabled}/>
                </div>
                <div class="col-2 col-lg-1 pe-2">
                  <button type="button" class="col-12 btn btn-success" onclick={import} {disabled}>{"Save"}</button>
                </div>
              </div>
            </form>
          </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            HomeMsg::Load(lists) => {
                self.lists = lists;
                true
            }
            HomeMsg::Import => {
                let history = ctx.link().history().unwrap();
                let input = self.import_ref.cast::<HtmlSelectElement>().unwrap().value();
                let playlist_re =
                    Regex::new(r"https://open.spotify.com/playlist/([[:alnum:]]*)").unwrap();
                let album_re =
                    Regex::new(r"https://open.spotify.com/album/([[:alnum:]]*)").unwrap();
                // TODO: handle bad input
                let id = if let Some(input) = playlist_re.captures_iter(&input).next() {
                    format!("spotify:playlist:{}", &input[1])
                } else if let Some(input) = album_re.captures_iter(&input).next() {
                    format!("spotify:album:{}", &input[1])
                } else {
                    return false;
                };
                let select_ref = self.select_ref.clone();
                ctx.link().send_future(async move {
                    import_list(&id).await.unwrap();
                    Home::fetch_lists(select_ref, history).await
                });
                false
            }
        }
    }
}

impl Home {
    async fn fetch_lists(select_ref: NodeRef, history: AnyHistory) -> HomeMsg {
        let lists = fetch_lists().await.unwrap();
        let lists = futures::future::join_all(lists.into_iter().map(|list| async {
            let query = query_items(&list.id).await?;
            let select_ref = select_ref.clone();
            let history_copy = history.clone();
            let id = list.id.clone();
            let on_go_select = Callback::once(move |_| {
                let mode = select_ref.cast::<HtmlSelectElement>().unwrap().value();
                match mode.as_ref() {
                    "Random Matches" => {
                        history_copy.push(Route::Match { id });
                    }
                    "Random Rounds" => {
                        history_copy
                            .push_with_query(
                                Route::Match { id },
                                [("mode", "rounds")].into_iter().collect::<HashMap<_, _>>(),
                            )
                            .unwrap();
                    }
                    "Tournament" => {
                        history_copy.push(Route::Tournament { id });
                    }
                    "Random Tournament" => {
                        history_copy
                            .push_with_query(
                                Route::Tournament { id },
                                [("mode", "random")].into_iter().collect::<HashMap<_, _>>(),
                            )
                            .unwrap();
                    }
                    _ => {
                        web_sys::console::log_1(&JsValue::from("Invalid mode"));
                    }
                };
            });
            let history = history.clone();
            let id = list.id.clone();
            let on_edit_select = Callback::once(move |_| history.push(Route::Edit { id }));
            Ok(ListData {
                data: list,
                query,
                on_go_select,
                on_edit_select,
            })
        }))
        .await
        .into_iter()
        .collect::<Result<_, JsValue>>()
        .unwrap();
        HomeMsg::Load(lists)
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
        let go = list.on_go_select.clone();
        let edit = list.on_edit_select.clone();
        // TODO: support user list actions
        let disabled = matches!(list.data.mode, ListMode::User);
        html! {
          <div class="col-12 col-md-6">
            <div class="row">
              <div class="col-8">
                <h2>{&list.data.name}</h2>
              </div>
              <div class="col-2">
                <button type="button" class="btn btn-success col-12" onclick={go} {disabled}>{"Go"}</button>
              </div>
              <div class="col-2">
                <button type="button" class="btn btn-warning col-12" onclick={edit} {disabled}>{"Edit"}</button>
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
    on_edit_select: Callback<MouseEvent>,
}

// Called by our JS entry point to run the example
#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue> {
    yew::start_app::<App>();
    Ok(())
}

async fn fetch_lists() -> Result<Vec<List>, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query("/api/lists", "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let lists: Lists = json.into_serde().unwrap();
    Ok(lists.lists)
}

async fn fetch_list(id: &str) -> Result<List, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/lists/{}", id), "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(json.into_serde().unwrap())
}

async fn query_items(id: &str) -> Result<ItemQuery, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/lists/{}/items", id), "GET").unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(json.into_serde().unwrap())
}

async fn update_stats(list: &str, win: &str, lose: &str) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(
        &format!(
            "/api/?action=update&list={}&win={}&lose={}",
            list, win, lose
        ),
        "POST",
    )?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

async fn import_list(id: &str) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/?action=import&id={}", id), "POST")?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

fn query(url: &str, method: &str) -> Result<Request, JsValue> {
    let mut opts = RequestInit::new();
    opts.method(method);
    opts.mode(RequestMode::Cors);
    Request::new_with_str_and_init(url, &opts)
}

fn get_user() -> Option<String> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let html_document = document.dyn_into::<HtmlDocument>().unwrap();
    let cookie = html_document.cookie();
    let cookies: HashMap<_, _> = cookie
        .as_ref()
        .unwrap()
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .collect();
    cookies.get("user").map(ToString::to_string)
}
