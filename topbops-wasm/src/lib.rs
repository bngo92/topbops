#![feature(iter_intersperse)]
use crate::bootstrap::{Accordion, Collapse};
use crate::edit::Edit;
use crate::list::ListComponent;
use crate::random::Match;
use crate::search::Search;
use crate::tournament::Tournament;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use topbops::{ItemQuery, List, ListMode, Lists, Spotify};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlDocument, HtmlSelectElement, Request, RequestInit, RequestMode, Response};
use yew::{html, Callback, Component, Context, Html, NodeRef, Properties};
use yew_router::prelude::Link;
use yew_router::scope_ext::RouterScopeExt;
use yew_router::{BrowserRouter, Routable, Switch};

mod base;
mod bootstrap;
mod edit;
mod list;
mod random;
mod search;
pub mod tournament;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/lists")]
    Lists,
    #[at("/lists/:id")]
    List { id: String },
    #[at("/lists/:id/edit")]
    Edit { id: String },
    #[at("/lists/:id/match")]
    Match { id: String },
    #[at("/lists/:id/tournament")]
    Tournament { id: String },
    #[at("/search")]
    Search,
}

fn switch(routes: Route) -> Html {
    match routes {
        Route::Home => html! { <Home show_all_lists=false/> },
        Route::Lists => html! { <Home show_all_lists=true/> },
        Route::List { id } => html! { <ListComponent {id}/> },
        Route::Edit { id } => html! { <Edit {id}/> },
        Route::Match { id } => html! { <Match {id}/> },
        Route::Tournament { id } => html! {
            <Tournament {id}/>
        },
        Route::Search => html! { <Search/> },
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
        // TODO: make anchors active if active
        let search = /*if location.pathname().unwrap() == "/search" {
            "nav-link active"
        } else */{
            "nav-link"
        };
        html! {
            <div>
                <BrowserRouter>
                    <nav class="navbar navbar-expand navbar-dark bg-dark">
                        <div class="container-lg">
                            <Link<Route> classes="navbar-brand" to={Route::Home}>{"Bops to the Top"}</Link<Route>>
                            <ul class="navbar-nav me-auto">
                                <li class="nav-item">
                                    <Link<Route> classes={search} to={Route::Lists}>{"Lists"}</Link<Route>>
                                </li>
                                <li class="nav-item">
                                    <Link<Route> classes={search} to={Route::Search}>{"Search"}</Link<Route>>
                                </li>
                            </ul>
                            <ul class="navbar-nav">
                                <li class="nav-item">
                                    if let Some(user) = get_user() {
                                        <a class="nav-link" href="/api/logout">{format!("{} Logout", user)}</a>
                                    } else {
                                        <a class="nav-link" href={format!("https://accounts.spotify.com/authorize?client_id=ee3d1b4f8d80477ea48743a511ef3018&redirect_uri={}/api/login&response_type=code&scope=playlist-modify-public playlist-modify-private", location.origin().unwrap().as_str())}>{"Login"}</a>
                                    }
                                </li>
                            </ul>
                        </div>
                    </nav>
                    <div class="container-lg my-md-4">
                        <Switch<Route> render={switch} />
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
    None,
    ToggleHelp,
    Load(Vec<List>),
    Create,
    Import,
}

#[derive(PartialEq, Properties)]
pub struct HomeProps {
    show_all_lists: bool,
}

pub struct Home {
    help_collapsed: bool,
    lists: Vec<List>,
    select_ref: NodeRef,
    import_ref: NodeRef,
}

impl Component for Home {
    type Message = HomeMsg;
    type Properties = HomeProps;

    fn create(ctx: &Context<Self>) -> Self {
        let select_ref = NodeRef::default();
        ctx.link()
            .send_future(Home::fetch_lists(!ctx.props().show_all_lists));
        Home {
            help_collapsed: get_user().is_some(),
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
        let create = ctx.link().callback(|_| HomeMsg::Create);
        let import = ctx.link().callback(|_| HomeMsg::Import);
        html! {
          <div>
            <h1>{"Home"}</h1>
            <div class="row mb-3">
              <label class="col-auto col-form-label">
                <strong>{"Compare Mode:"}</strong>
              </label>
              <div class="col-auto">
                <select ref={self.select_ref.clone()} class="form-select">
                  <option>{"Tournament"}</option>
                  <option selected=true>{"Random Tournament"}</option>
                  <option>{"Random Matches"}</option>
                  <option>{"Random Rounds"}</option>
                </select>
              </div>
              <div class="col-auto">
                <button class="btn btn-info" onclick={ctx.link().callback(|_| HomeMsg::ToggleHelp)}>{"Help"}</button>
              </div>
            </div>
            <Collapse collapsed={self.help_collapsed}>
              <p>
              {"If you are the type of person that struggles to answer the question of what your favorite song is, this website is for you.
                This website allows you to discover which songs you like by comparing them in different ways.
                Select a comparison mode and click \"Compare\" to start comparing songs in that list.
                The default mode is to compare songs in a randomly generated tournament."}
              </p>
              <p>{"Here is the full list of compare modes:"}</p>
              <ul>
                <li><strong>{"Tournament"}</strong>{" - Compare songs in a seeded tournament."}</li>
                <li><strong>{"Random Tournament"}</strong>{" - Compare songs in a randomly generated tournament."}</li>
                <li><strong>{"Random Matches"}</strong>{" - Compare random songs."}</li>
                <li><strong>{"Random Rounds"}</strong>{" - Compare random songs. Every song will be chosen once before a song is repeated."}</li>
              </ul>
              <p>{"You can also:"}</p>
              <ul class="mb-0">
                  <li>{"View songs in the list by expanding the widget or by clicking on \"View\"."}</li>
                  <li>{"Search for data about your comparison results by clicking on \"Search\"."}</li>
              </ul>
            </Collapse>
            <div class="row mt-3">
            {for self.lists.iter().map(|l| html! {<Widget list={l.clone()} select_ref={self.select_ref.clone()}/>})}
            </div>
            <button type="button" class="btn btn-primary" onclick={create} {disabled}>{"Create List"}</button>
            if ctx.props().show_all_lists {
              <h1>{"My Spotify Playlists"}</h1>
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
            }
          </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            HomeMsg::None => false,
            HomeMsg::ToggleHelp => {
                self.help_collapsed = !self.help_collapsed;
                true
            }
            HomeMsg::Load(lists) => {
                self.lists = lists;
                true
            }
            HomeMsg::Create => {
                let navigator = ctx.link().navigator().unwrap();
                ctx.link().send_future(async move {
                    let list = create_list().await.unwrap();
                    navigator.push(&Route::Edit { id: list.id });
                    HomeMsg::None
                });
                false
            }
            HomeMsg::Import => {
                let input = self.import_ref.cast::<HtmlSelectElement>().unwrap().value();
                // TODO: handle bad input
                let id = match parse_spotify_source(&input) {
                    Some(Spotify::Playlist(id)) => {
                        format!("spotify:playlist:{}", id)
                    }
                    Some(Spotify::Album(id)) => {
                        format!("spotify:album:{}", id)
                    }
                    None => {
                        return false;
                    }
                };
                ctx.link().send_future(async move {
                    import_list(&id).await.unwrap();
                    Home::fetch_lists(false).await
                });
                false
            }
        }
    }

    // Changing the favorite prop using BrowserRouter doesn't re-render so manually do it
    fn changed(&mut self, ctx: &Context<Self>, _: &Self::Properties) -> bool {
        ctx.link()
            .send_future(Home::fetch_lists(!ctx.props().show_all_lists));
        true
    }
}

fn parse_spotify_source(input: &str) -> Option<Spotify> {
    let playlist_re = Regex::new(r"https://open.spotify.com/playlist/([[:alnum:]]*)").unwrap();
    let album_re = Regex::new(r"https://open.spotify.com/album/([[:alnum:]]*)").unwrap();
    return if let Some(caps) = playlist_re.captures_iter(input).next() {
        Some(Spotify::Playlist(caps[1].to_owned()))
    } else if let Some(caps) = album_re.captures_iter(input).next() {
        Some(Spotify::Album(caps[1].to_owned()))
    } else {
        None
    };
}

impl Home {
    async fn fetch_lists(favorite: bool) -> HomeMsg {
        let lists = fetch_lists(favorite).await.unwrap();
        HomeMsg::Load(lists)
    }
}

enum WidgetMsg {
    Fetching(Rc<String>),
    Success(ItemQuery),
}

#[derive(PartialEq, Properties)]
pub struct WidgetProps {
    list: List,
    select_ref: NodeRef,
}

struct Widget {
    collapsed: bool,
    query: Option<ItemQuery>,
}

impl Component for Widget {
    type Message = WidgetMsg;
    type Properties = WidgetProps;

    fn create(_: &Context<Self>) -> Self {
        Widget {
            collapsed: true,
            query: None,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let list = &ctx.props().list;
        let id = Rc::new(list.id.clone());
        let on_toggle = ctx
            .link()
            .callback(move |_| WidgetMsg::Fetching(Rc::clone(&id)));
        let navigator = ctx.link().navigator().unwrap();
        let select_ref = ctx.props().select_ref.clone();
        let navigator_copy = navigator.clone();
        let id = list.id.clone();
        let compare = Callback::from(move |_| {
            let id = id.clone();
            let mode = select_ref.cast::<HtmlSelectElement>().unwrap().value();
            match mode.as_ref() {
                "Random Matches" => {
                    navigator_copy.push(&Route::Match { id });
                }
                "Random Rounds" => {
                    navigator_copy
                        .push_with_query(
                            &Route::Match { id },
                            &[("mode", "rounds")].into_iter().collect::<HashMap<_, _>>(),
                        )
                        .unwrap();
                }
                "Tournament" => {
                    navigator_copy.push(&Route::Tournament { id });
                }
                "Random Tournament" => {
                    navigator_copy
                        .push_with_query(
                            &Route::Tournament { id },
                            &[("mode", "random")].into_iter().collect::<HashMap<_, _>>(),
                        )
                        .unwrap();
                }
                _ => {
                    web_sys::console::log_1(&JsValue::from("Invalid mode"));
                }
            };
        });
        let id = list.id.clone();
        let go = Callback::from(move |_| {
            navigator.push(&Route::List { id: id.clone() });
        });
        // TODO: support actions on views
        let disabled = matches!(list.mode, ListMode::View);
        html! {
            <div class="col-12 col-md-6">
                <Accordion header={list.name.clone()} collapsed={self.collapsed} {on_toggle}>
                    if let Some(query) = &self.query {
                        {crate::base::table_view(&query.fields.iter().map(String::as_str).collect::<Vec<_>>(), query.items.iter().zip(1..).map(|(item, i)| Some((i, Cow::from(&item.values)))))}
                    } else {
                        <div></div>
                    }
                </Accordion>
                <div class="row mb-3">
                    <div class="col-auto">
                        <button type="button" class="btn btn-success" onclick={go} {disabled}>{"View"}</button>
                    </div>
                    <div class="col-auto">
                        <button type="button" class="btn btn-warning" onclick={compare} {disabled}>{"Compare"}</button>
                    </div>
                </div>
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            WidgetMsg::Fetching(id) => {
                // TODO: add the ability to refresh
                if self.query.is_none() {
                    ctx.link().send_future(async move {
                        WidgetMsg::Success(query_items(&id).await.unwrap())
                    });
                    false
                } else {
                    self.collapsed = !self.collapsed;
                    true
                }
            }
            WidgetMsg::Success(query) => {
                self.collapsed = false;
                self.query = Some(query);
                true
            }
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
            <td class="td">{&ctx.props().values[0]}</td>
            <td class="td">{&ctx.props().values[1]}</td>
          </tr>
        }
    }
}

// Called by our JS entry point to run the example
#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue> {
    yew::Renderer::<App>::new().render();
    Ok(())
}

async fn fetch_lists(favorite: bool) -> Result<Vec<List>, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/lists?favorite={}", favorite), "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let lists: Lists = serde_wasm_bindgen::from_value(json).unwrap();
    Ok(lists.lists)
}

async fn fetch_list(id: &str) -> Result<List, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/lists/{}", id), "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

async fn create_list() -> Result<List, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = Request::new_with_str_and_init(
        "/api/lists",
        RequestInit::new().method("POST").mode(RequestMode::Cors),
    )?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

async fn update_list(id: &str, list: List) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = Request::new_with_str_and_init(
        &format!("/api/lists/{}", id),
        RequestInit::new()
            .method("PUT")
            .mode(RequestMode::Cors)
            .body(Some(&JsValue::from_str(
                &serde_json::to_string(&list).unwrap(),
            ))),
    )?;
    request.headers().set("Content-Type", "application/json")?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

async fn query_items(id: &str) -> Result<ItemQuery, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/lists/{}/items", id), "GET").unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
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

async fn push_list(id: &str) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/?action=push&list={}", id), "POST")?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

async fn import_list(id: &str) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/?action=import&id={}", id), "POST")?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

async fn find_items(search: &str) -> Result<ItemQuery, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let request = query(&format!("/api/items?q=search&query={}", search), "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    if [400, 500].contains(&resp.status()) {
        return Err(JsFuture::from(resp.text()?).await?);
    }
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
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
