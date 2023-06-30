#![feature(iter_intersperse)]
use crate::bootstrap::{Accordion, Collapse, Modal};
use crate::edit::Edit;
use crate::list::item::ListItems;
use crate::random::Match;
use crate::search::Search;
use crate::tournament::Tournament;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlSelectElement, MouseEvent, Request, RequestInit, RequestMode, Response, Window};
use yew::{html, Callback, Component, Context, Html, NodeRef, Properties};
use yew_router::prelude::{Link, Redirect};
use yew_router::scope_ext::RouterScopeExt;
use yew_router::{BrowserRouter, Routable, Switch};
use zeroflops::{Id, ItemQuery, List, ListMode, Lists, Spotify, User};

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
    View { id: String },
    #[at("/lists/:id/items")]
    List { id: String },
    #[at("/lists/:id/edit")]
    Edit { id: String },
    #[at("/lists/:id/match")]
    Match { id: String },
    #[at("/lists/:id/tournament")]
    Tournament { id: String },
    #[at("/search")]
    Search,
    #[at("/settings")]
    Settings,
}

fn switch(routes: Route, user: Rc<Option<User>>) -> Html {
    let logged_in = user.is_some();
    let content = match routes {
        Route::Home => html! { <Home {logged_in}/> },
        Route::Lists => html! { <crate::list::Lists {logged_in}/> },
        Route::List { id } => html! { <ListComponent {user} {id} view={ListTab::Items}/> },
        Route::View { id } => html! { <ListComponent {user} {id} view={ListTab::View}/> },
        Route::Edit { id } => html! { <ListComponent {user} {id} view={ListTab::Edit}/> },
        Route::Match { id } => html! { <Match {id}/> },
        Route::Tournament { id } => html! {
            <Tournament {id}/>
        },
        Route::Search => return html! { <Search/> },
        Route::Settings => html! {
            if let Some(user) = (*user).clone() {
                <Settings {user}/>
            } else {
                <Redirect<Route> to={Route::Home}/>
            }
        },
    };
    html! {
        <div class="container-lg my-md-4">
            { content }
        </div>
    }
}

enum Msg {
    Demo,
    Success(User),
    Login,
    HideLogin,
    Dropdown,
    ResetDropdown,
    //Logout,
    //Reload,
}

struct App {
    user_loaded: bool,
    user: Rc<Option<User>>,
    login: bool,
    dropdown: bool,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_future(async move {
            match get_user().await {
                Ok(user) => Msg::Success(user),
                Err(_) => Msg::Demo,
            }
        });
        App {
            user_loaded: false,
            user: Rc::new(None),
            login: false,
            dropdown: false,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let window = window();
        let location = window.location();
        //let onclick = ctx.link().callback(|_| Msg::Logout);
        // TODO: make anchors active if active
        let search = /*if location.pathname().unwrap() == "/search" {
            "nav-link active"
        } else */{
            "nav-link"
        };
        let (toggle_class, menu_class) = if self.dropdown {
            ("nav-link dropdown-toggle show", "dropdown-menu show")
        } else {
            ("nav-link dropdown-toggle", "dropdown-menu")
        };
        let dropdown = ctx.link().callback(|e: MouseEvent| {
            // Prevent reset_dropdown from triggering
            e.stop_propagation();
            Msg::Dropdown
        });
        let login = ctx.link().callback(|_| Msg::Login);
        let hide = ctx.link().callback(|_| Msg::HideLogin);
        let reset_dropdown = ctx.link().callback(|_| Msg::ResetDropdown);
        html! {
            <div class="vh-100" onclick={reset_dropdown}>
                <BrowserRouter>
                    <nav class="navbar navbar-expand navbar-dark bg-dark">
                        <div class="container-lg">
                            <Link<Route> classes="navbar-brand" to={Route::Home}>{"zeroflops"}</Link<Route>>
                            <ul class="navbar-nav me-auto">
                                <li class="nav-item">
                                    <Link<Route> classes={search} to={Route::Lists}>{"Lists"}</Link<Route>>
                                </li>
                                <li class="nav-item">
                                    <Link<Route> classes={search} to={Route::Search}>{"Search"}</Link<Route>>
                                </li>
                            </ul>
                            if self.user_loaded {
                                <ul class="navbar-nav">
                                    if let Some(user) = &*self.user {
                                        <li class="nav-item dropdown">
                                            <a class={toggle_class} href="#" onclick={dropdown}>{&user.user_id}</a>
                                            <ul class={menu_class}>
                                                <li><Link<Route> classes="dropdown-item" to={Route::Settings}>{"Settings"}</Link<Route>></li>
                                                <li><a class="dropdown-item" href="/api/logout">{"Log out"}</a></li>
                                            </ul>
                                        </li>
                                    } else {
                                        <li class="nav-item">
                                            <a class="nav-link" href="#" onclick={login}>{"Log in"}</a>
                                        </li>
                                    }
                                </ul>
                            }
                        </div>
                    </nav>
                    if self.login {
                        <Modal header={"Log in"} {hide}>
                            <div class="modal-body d-grid gap-2">
                                <a class="btn btn-success" href={format!("https://accounts.spotify.com/authorize?client_id=ee3d1b4f8d80477ea48743a511ef3018&redirect_uri={}/api/login&response_type=code&scope=playlist-modify-public playlist-modify-private", location.origin().unwrap().as_str())}>{"Log in with Spotify"}</a>
                                <a class="btn btn-success" href={format!("https://accounts.google.com/o/oauth2/v2/auth?client_id=1038220726403-n55jha2cvprd8kdb4akdfvo0uiok4p5u.apps.googleusercontent.com&redirect_uri={}/api/login/google&response_type=code&scope=email", location.origin().unwrap().as_str())}>{"Log in with Google"}</a>
                            </div>
                        </Modal>
                        <div class="modal-backdrop show"></div>
                    }
                    if self.user_loaded {
                        <Switch<Route> render={let user = Rc::clone(&self.user); move |routes| switch(routes, user.clone())} />
                    }
                </BrowserRouter>
            </div>
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Demo => self.user_loaded = true,
            Msg::Success(user) => {
                self.user_loaded = true;
                self.user = Rc::new(Some(user))
            }
            Msg::Login => self.login = true,
            Msg::HideLogin => self.login = false,
            Msg::Dropdown => self.dropdown = !self.dropdown,
            Msg::ResetDropdown => self.dropdown = false,
            /*Msg::Logout => {
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
            Msg::Reload => true,*/
        }
        true
    }
}

pub enum HomeMsg {
    ToggleHelp,
    Load(Vec<List>),
    Create,
}

#[derive(Eq, PartialEq, Properties)]
pub struct UserProps {
    logged_in: bool,
}

pub struct Home {
    help_collapsed: bool,
    lists: Vec<List>,
    select_ref: NodeRef,
}

impl Component for Home {
    type Message = HomeMsg;
    type Properties = UserProps;

    fn create(ctx: &Context<Self>) -> Self {
        let select_ref = NodeRef::default();
        ctx.link().send_future(Home::fetch_lists());
        Home {
            help_collapsed: ctx.props().logged_in,
            lists: Vec::new(),
            select_ref,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let disabled = !ctx.props().logged_in;
        let create = ctx.link().callback(|_| HomeMsg::Create);
        html! {
          <div>
            <h1>if disabled { {"Demo"} } else { { "Home" } }</h1>
            <div class="row mb-3">
              <label class="col-auto col-form-label">
                <strong>{"Sort Mode:"}</strong>
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
              {"zeroflops is an app that helps you filter your data and remove flops from your life.
                Use it to gain insights about your favorite songs, TV shows, and even restaurants.
                zeroflops makes it easy to rate and/or rank what's important to you."}
              </p>
              <p>
              {"The data is organized into lists of items and your lists are displayed here on the home page using user-defined widgets.
                The fastest way to rank your items is with a randomly generated tournament.
                You can start a tournament for a list by clicking the "}<button type="button" class="btn btn-success btn-sm">{"Rank"}</button>
                {" button below the list widget. Here is the full list of sort modes:"}
              </p>
              <ul>
                <li><strong>{"Tournament"}</strong>{" - Sort by choosing between items that are organized using a seeded tournament."}</li>
                <li><strong>{"Random Tournament"}</strong>{" - Sort by choosing between items that are organized using a randomly generated tournament."}</li>
                <li><strong>{"Random Matches"}</strong>{" - Sort by choosing between randomly selected items."}</li>
                <li><strong>{"Random Rounds"}</strong>{" - This mode is similar to Random Matches except every item will be selected before an item is repeated."}</li>
              </ul>
              <p>{"To rate items, go to the item rating page for the list by clicking on the "}<button type="button" class="btn btn-success btn-sm">{"Rate"}</button>{" button."}</p>
              <p>{"You can also:"}</p>
              <ul class="mb-0">
                  <li>{"View items in the list by clicking on the widget to expand it."}</li>
                  <li>{"Search for data about your ratings and rankings by going to the "}<Link<Route> to={Route::Search}>{"Search"}</Link<Route>>{" page."}</li>
              </ul>
            </Collapse>
            <div class="row mt-3">
            {for self.lists.iter().map(|l| html! {<Widget list={l.clone()} select_ref={self.select_ref.clone()}/>})}
            </div>
            <button type="button" class="btn btn-primary" onclick={create} {disabled}>{"Create List"}</button>
          </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
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
                ctx.link().send_future_batch(async move {
                    let list = create_list().await.unwrap();
                    navigator.push(&Route::Edit { id: list.id });
                    None
                });
                false
            }
        }
    }
}

fn parse_spotify_source(input: String) -> Option<Spotify> {
    let playlist_re = Regex::new(r"https://open.spotify.com/playlist/([[:alnum:]]*)").unwrap();
    let album_re = Regex::new(r"https://open.spotify.com/album/([[:alnum:]]*)").unwrap();
    return if let Some(caps) = playlist_re.captures_iter(&input).next() {
        Some(Spotify::Playlist(Id {
            id: caps[1].to_owned(),
            raw_id: input,
        }))
    } else if let Some(caps) = album_re.captures_iter(&input).next() {
        Some(Spotify::Album(Id {
            id: caps[1].to_owned(),
            raw_id: input,
        }))
    } else {
        None
    };
}

fn parse_setlist_source(input: String) -> Option<Id> {
    let re = Regex::new(r"https://www.setlist.fm/setlist/.*-([[:alnum:]]*).html").unwrap();
    return if let Some(caps) = re.captures_iter(&input).next() {
        Some(Id {
            id: caps[1].to_owned(),
            raw_id: input,
        })
    } else {
        None
    };
}

impl Home {
    async fn fetch_lists() -> HomeMsg {
        let lists = fetch_lists(true).await.unwrap();
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
                        <button type="button" class="btn btn-success" onclick={go} {disabled}>{"Rate"}</button>
                    </div>
                    <div class="col-auto">
                        <button type="button" class="btn btn-success" onclick={compare} {disabled}>{"Rank"}</button>
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

enum ListViewMsg {
    Success(ItemQuery),
}

#[derive(PartialEq, Properties)]
pub struct ListViewProps {
    id: String,
}

struct ListView {
    query: Option<ItemQuery>,
}

impl Component for ListView {
    type Message = ListViewMsg;
    type Properties = ListViewProps;

    fn create(ctx: &Context<Self>) -> Self {
        let id = ctx.props().id.clone();
        ctx.link()
            .send_future(async move { ListViewMsg::Success(query_items(&id).await.unwrap()) });
        Self { query: None }
    }

    fn view(&self, _: &Context<Self>) -> Html {
        let Some(query) = &self.query else { return html! {}; };
        crate::base::table_view(
            &query.fields.iter().map(String::as_str).collect::<Vec<_>>(),
            query
                .items
                .iter()
                .zip(1..)
                .map(|(item, i)| Some((i, Cow::from(&item.values)))),
        )
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        let ListViewMsg::Success(query) = msg;
        self.query = Some(query);
        true
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

enum ListState {
    Fetching,
    Success(List),
}

pub enum ListMsg {
    Load(List),
}

#[derive(Eq, PartialEq, Properties)]
pub struct ListProps {
    pub view: ListTab,
    pub user: Rc<Option<User>>,
    pub id: String,
}

pub struct ListComponent {
    state: ListState,
}

impl Component for ListComponent {
    type Message = ListMsg;
    type Properties = ListProps;

    fn create(ctx: &Context<Self>) -> Self {
        let id = ctx.props().id.clone();
        ctx.link()
            .send_future(async move { ListMsg::Load(crate::fetch_list(&id).await.unwrap()) });
        ListComponent {
            state: ListState::Fetching,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        match &self.state {
            ListState::Fetching => html! {},
            ListState::Success(list) => {
                let mut tabs = ["nav-link"; 3];
                let active = "nav-link active";
                match ctx.props().view {
                    ListTab::View => {
                        tabs[0] = active;
                    }
                    ListTab::Items => {
                        tabs[1] = active;
                    }
                    ListTab::Edit => {
                        tabs[2] = active;
                    }
                }
                let view = match ctx.props().view {
                    ListTab::View => html! { <ListView id={list.id.clone()}/> },
                    ListTab::Items => {
                        html! { <ListItems user={Rc::clone(&ctx.props().user)} list={list.clone()}/> }
                    }
                    ListTab::Edit => {
                        html! { <Edit logged_in={ctx.props().user.is_some()} list={list.clone()}/> }
                    }
                };
                html! {
                    <div class="row">
                        <div class="col-lg-10 col-xl-8">
                            <h2 class="col-11">{&list.name}</h2>
                            <ul class="nav nav-tabs mb-3">
                                <li class="nav-item">
                                    <Link<Route> classes={tabs[0]} to={Route::View{id: list.id.clone()}}>{"View"}</Link<Route>>
                                </li>
                                <li class="nav-item">
                                    <Link<Route> classes={tabs[1]} to={Route::List{id: list.id.clone()}}>{"Items"}</Link<Route>>
                                </li>
                                <li class="nav-item">
                                    <Link<Route> classes={tabs[2]} to={Route::Edit{id: list.id.clone()}}>{"Settings"}</Link<Route>>
                                </li>
                            </ul>
                            {view}
                        </div>
                    </div>
                }
            }
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            ListMsg::Load(list) => {
                self.state = ListState::Success(list);
                true
            }
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum ListTab {
    View,
    Items,
    Edit,
}

#[derive(Eq, PartialEq, Properties)]
pub struct SettingsProps {
    user: User,
}

struct Settings;

impl Component for Settings {
    type Message = ();
    type Properties = SettingsProps;

    fn create(_: &Context<Self>) -> Self {
        Settings
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let window = window();
        let location = window.location();
        // TODO: let you remove integrations
        // Should we link to Google profile?
        html! {
            <div>
                <h1>{"Integrations"}</h1>
                <h2>{"Spotify"}</h2>
                if let (Some(url), Some(user)) = (&ctx.props().user.spotify_url, &ctx.props().user.spotify_user) {
                    <a href={url.clone()}>{&user}</a>
                } else {
                    <a class="btn btn-success" href={format!("https://accounts.spotify.com/authorize?client_id=ee3d1b4f8d80477ea48743a511ef3018&redirect_uri={}/api/login&response_type=code&scope=playlist-modify-public playlist-modify-private", location.origin().unwrap().as_str())}>{"Log in with Spotify"}</a>
                }
                <h2>{"Google"}</h2>
                if let Some(google_email) = &ctx.props().user.google_email {
                    <p>{google_email}</p>
                } else {
                    <a class="btn btn-success" href={format!("https://accounts.google.com/o/oauth2/v2/auth?client_id=1038220726403-n55jha2cvprd8kdb4akdfvo0uiok4p5u.apps.googleusercontent.com&redirect_uri={}/api/login/google&response_type=code&scope=email", location.origin().unwrap().as_str())}>{"Log in with Google"}</a>
                }
            </div>
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
    let window = window();
    let request = query(&format!("/api/lists?favorite={}", favorite), "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    let lists: Lists = serde_wasm_bindgen::from_value(json).unwrap();
    Ok(lists.lists)
}

async fn fetch_list(id: &str) -> Result<List, JsValue> {
    let window = window();
    let request = query(&format!("/api/lists/{}", id), "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

async fn create_list() -> Result<List, JsValue> {
    let window = window();
    let request = Request::new_with_str_and_init(
        "/api/lists",
        RequestInit::new().method("POST").mode(RequestMode::Cors),
    )?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

async fn update_list(list: &List) -> Result<(), JsValue> {
    let window = window();
    let request = Request::new_with_str_and_init(
        &format!("/api/lists/{}", list.id),
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

async fn delete_list(id: &str) -> Result<(), JsValue> {
    let window = window();
    let request = Request::new_with_str_and_init(
        &format!("/api/lists/{}", id),
        RequestInit::new().method("DELETE").mode(RequestMode::Cors),
    )?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

async fn get_items(id: &str) -> Result<ItemQuery, JsValue> {
    let window = window();
    let request = query(&format!("/api/lists/{}/items", id), "GET").unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

async fn query_items(id: &str) -> Result<ItemQuery, JsValue> {
    let window = window();
    let request = query(&format!("/api/lists/{}/query", id), "GET").unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

async fn update_stats(list: &str, win: &str, lose: &str) -> Result<(), JsValue> {
    let window = window();
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
    let window = window();
    let request = query(&format!("/api/?action=push&list={}", id), "POST")?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

async fn import_list(source: &str, id: &str) -> Result<(), JsValue> {
    let window = window();
    let request = query(
        &format!("/api/?action=import&source={source}&id={id}"),
        "POST",
    )?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

async fn find_items(search: &str) -> Result<ItemQuery, JsValue> {
    let window = window();
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

async fn get_user() -> Result<User, JsValue> {
    let window = window();
    let request = query("/api/user", "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

fn window() -> Window {
    web_sys::window().expect("no global `window` exists")
}
