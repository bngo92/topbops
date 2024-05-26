#![feature(iter_intersperse)]
use crate::{
    base::Input,
    bootstrap::Modal,
    dataframe::DataFrame,
    edit::Edit,
    home::Home,
    integrations::spotify::SpotifyIntegration,
    list::item::ListItems,
    plot::DataView,
    random::{RandomMatches, RandomRounds},
    search::Search,
    settings::Settings,
    tournament::{RandomTournamentLoader, TournamentLoader},
};
use arrow::array::AsArray;
use js_sys::Uint8Array;
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    rc::Rc,
};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlSelectElement, MouseEvent, Request, RequestInit, RequestMode, Response, Window};
use yew::{html, Callback, Component, Context, Html, NodeRef, Properties};
use yew_router::{
    prelude::{Link, Redirect},
    scope_ext::RouterScopeExt,
    BrowserRouter, Routable, Switch,
};
use zeroflops::{Id, Items, List, ListMode, Lists, Spotify, User};

mod base;
mod bootstrap;
mod dataframe;
mod docs;
mod edit;
mod home;
mod integrations;
mod list;
mod plot;
mod random;
mod search;
mod settings;
pub mod tournament;

type RouteQuery = &'static [(&'static str, &'static str)];

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Home,
    #[at("/docs")]
    Docs,
    #[at("/lists")]
    ListsRoot,
    #[at("/lists/*")]
    Lists,
    #[at("/search")]
    Search,
    #[at("/settings")]
    Settings,
    #[at("/integrations/spotify")]
    Spotify,
}

#[derive(Clone, Routable, PartialEq)]
pub enum ListsRoute {
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
}

pub enum ListPage {
    View,
    List,
    Edit,
    RandomMatches,
    RandomRounds,
    Tournament,
    RandomTournament,
}

fn switch(
    routes: Route,
    user: Rc<Option<User>>,
    list_dropdown: bool,
    show_list_dropdown: Rc<Callback<MouseEvent>>,
) -> Html {
    let logged_in = user.is_some();
    match routes {
        Route::Home => html! { <Home {logged_in}/> },
        Route::Docs => docs::docs(),
        Route::ListsRoot => html! { <crate::list::Lists {logged_in}/> },
        Route::Lists => {
            let render = move |view| {
                html! {
                  <ListComponent {view} user={Rc::clone(&user)} dropdown={list_dropdown} show_dropdown={Rc::clone(&show_list_dropdown)}/>
                }
            };
            html! { <Switch<ListsRoute> {render}/> }
        }
        Route::Search => html! { <Search {logged_in}/> },
        Route::Settings => html! {
            if let Some(user) = (*user).clone() {
                <Settings {user}/>
            } else {
                <Redirect<Route> to={Route::Home}/>
            }
        },
        Route::Spotify => html! { <SpotifyIntegration {logged_in}/> },
    }
}

enum Msg {
    Demo,
    Success(User),
    Sidebar,
    HideSidebar,
    Login,
    HideLogin,
    Dropdown,
    ResetDropdown,
    ListDropdown,
    IntegrationsDropdown,
    //Logout,
    //Reload,
}

struct App {
    user_loaded: bool,
    user: Rc<Option<User>>,
    sidebar: bool,
    login: bool,
    dropdown: bool,
    list_dropdown: bool,
    integrations_dropdown: bool,
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
            sidebar: false,
            login: false,
            dropdown: false,
            list_dropdown: false,
            integrations_dropdown: false,
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
            "nav-link text-white"
        };
        let (toggle_class, menu_class) = dropdown_class(self.dropdown);
        let (int_toggle_class, int_menu_class) = dropdown_class(self.integrations_dropdown);
        let dropdown = ctx.link().callback(|e: MouseEvent| {
            // Prevent reset_dropdown from triggering
            e.stop_propagation();
            Msg::Dropdown
        });
        let int_dropdown = ctx.link().callback(|e: MouseEvent| {
            e.stop_propagation();
            Msg::IntegrationsDropdown
        });
        let sidebar_class = if self.sidebar {
            "p-3 bg-dark flex-shrink-0 h-100 offcanvas-sm offcanvas-start text-bg-dark show"
        } else {
            "p-3 bg-dark flex-shrink-0 h-100 offcanvas-sm offcanvas-start text-bg-dark"
        };
        let sidebar = ctx.link().callback(|_| Msg::Sidebar);
        let hide_sidebar = ctx.link().callback(|_| Msg::HideSidebar);
        let login = ctx.link().callback(|_| Msg::Login);
        let hide = ctx.link().callback(|_| Msg::HideLogin);
        let reset_dropdown = ctx.link().callback(|_| Msg::ResetDropdown);
        let render = {
            let user = Rc::clone(&self.user);
            let list_dropdown = self.list_dropdown;
            let show_list_dropdown = Rc::new(ctx.link().callback(|e: MouseEvent| {
                e.stop_propagation();
                Msg::ListDropdown
            }));
            move |routes| {
                switch(
                    routes,
                    Rc::clone(&user),
                    list_dropdown,
                    Rc::clone(&show_list_dropdown),
                )
            }
        };
        html! {
          <div onclick={reset_dropdown}>
            <BrowserRouter>
              <nav class="navbar navbar-expand navbar-dark bg-dark d-sm-none">
                <div class="container-lg d-flex justify-content-start gap-3">
                  <button type="button" class="border-0" style="background-color: transparent; color: rgba(255,255,255,0.85)" onclick={sidebar}>
                    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-list" viewBox="0 0 16 16">
                      <path fill-rule="evenodd" d="M2.5 12a.5.5 0 0 1 .5-.5h10a.5.5 0 0 1 0 1H3a.5.5 0 0 1-.5-.5m0-4a.5.5 0 0 1 .5-.5h10a.5.5 0 0 1 0 1H3a.5.5 0 0 1-.5-.5m0-4a.5.5 0 0 1 .5-.5h10a.5.5 0 0 1 0 1H3a.5.5 0 0 1-.5-.5"/>
                    </svg>
                  </button>
                  <Link<Route> classes="navbar-brand" to={Route::Home}>{"zeroflops"}</Link<Route>>
                </div>
              </nav>
              <div class="d-flex vh-100 min-vh-100 align-items-stretch">
                <div class={sidebar_class} style="width: 200px;">
                  <div class="h-100 offcanvas-body d-flex flex-column">
                    <div class="d-flex gap-2 align-items-baseline" data-bs-theme="dark">
                      <Link<Route> classes="text-white text-decoration-none fs-5" to={Route::Home}>{"zeroflops"}</Link<Route>>
                      <button type="button" class="btn-close d-sm-none" onclick={hide_sidebar}></button>
                    </div>
                    <hr/>
                    <ul class="nav nav-pills flex-column mb-auto">
                      <li class="nav-item">
                        <Link<Route> classes={search} to={Route::ListsRoot}>{"Lists"}</Link<Route>>
                      </li>
                      <li class="nav-item">
                        <Link<Route> classes={search} to={Route::Search}>{"Query"}</Link<Route>>
                      </li>
                      <li class="nav-item dropdown">
                        <a class={int_toggle_class} href="#" onclick={int_dropdown}>{"Integrations"}</a>
                        <ul class={int_menu_class}>
                          <li><Link<Route> classes="dropdown-item" to={Route::Spotify}>{"Spotify"}</Link<Route>></li>
                        </ul>
                      </li>
                      <li class="nav-item">
                        <Link<Route> classes={search} to={Route::Docs}>{"Docs"}</Link<Route>>
                      </li>
                    </ul>
                    if self.user_loaded {
                      <hr/>
                      <div>
                        <ul class="nav nav-pills flex-column">
                          if let Some(user) = &*self.user {
                            <li class="nav-item dropdown">
                              <a class={toggle_class} href="#" onclick={dropdown}>{&user.user_id}</a>
                              <ul class={menu_class} style="inset: auto auto 0px 0px; transform: translate3d(0px, -34px, 0px)">
                                <li><Link<Route> classes="dropdown-item" to={Route::Settings}>{"Settings"}</Link<Route>></li>
                                <li><a class="dropdown-item" href="/api/logout">{"Log out"}</a></li>
                              </ul>
                            </li>
                          } else {
                            <li class="nav-item">
                              <a class={search} href="#" onclick={login}>{"Log in"}</a>
                            </li>
                          }
                        </ul>
                      </div>
                    }
                  </div>
                </div>
                if self.user_loaded {
                  <div class="flex-grow-1 h-100 w-100 d-flex flex-column">
                    <Switch<Route> {render} />
                  </div>
                }
              </div>
              if self.login {
                <Modal header={"Log in"} {hide}>
                  <div class="modal-body d-grid gap-2">
                    <a class="btn btn-success" href={format!("https://accounts.spotify.com/authorize?client_id=ee3d1b4f8d80477ea48743a511ef3018&redirect_uri={}/api/login&response_type=code&scope=playlist-modify-public playlist-modify-private user-read-recently-played playlist-read-private", location.origin().unwrap().as_str())}>{"Log in with Spotify"}</a>
                    <a class="btn btn-success" href={format!("https://accounts.google.com/o/oauth2/v2/auth?client_id=1038220726403-n55jha2cvprd8kdb4akdfvo0uiok4p5u.apps.googleusercontent.com&redirect_uri={}/api/login/google&response_type=code&scope=email", location.origin().unwrap().as_str())}>{"Log in with Google"}</a>
                  </div>
                </Modal>
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
            Msg::Sidebar => self.sidebar = true,
            Msg::HideSidebar => self.sidebar = false,
            Msg::Login => self.login = true,
            Msg::HideLogin => self.login = false,
            Msg::Dropdown => self.dropdown = !self.dropdown,
            // We need to check which dropdown is clicked instead of relying on stop_propagation
            // TODO: fix multiple open dropdowns
            Msg::ResetDropdown => {
                self.dropdown = false;
                self.list_dropdown = false;
                self.integrations_dropdown = false;
            }
            Msg::ListDropdown => self.list_dropdown = !self.list_dropdown,
            Msg::IntegrationsDropdown => self.integrations_dropdown = !self.integrations_dropdown,
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

fn dropdown_class(dropdown: bool) -> (&'static str, &'static str) {
    match dropdown {
        true => (
            "nav-link dropdown-toggle show text-white",
            "dropdown-menu dropdown-menu-dark show",
        ),
        false => (
            "nav-link dropdown-toggle text-white",
            "dropdown-menu dropdown-menu-dark",
        ),
    }
}

#[derive(Eq, PartialEq, Properties)]
pub struct UserProps {
    logged_in: bool,
}

fn parse_spotify_source(input: String) -> Option<Spotify> {
    let playlist_re = Regex::new(r"https://open.spotify.com/playlist/([[:alnum:]]*)").unwrap();
    let album_re = Regex::new(r"https://open.spotify.com/album/([[:alnum:]]*)").unwrap();
    let track_re = Regex::new(r"https://open.spotify.com/track/([[:alnum:]]*)").unwrap();
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
    } else if let Some(caps) = track_re.captures_iter(&input).next() {
        Some(Spotify::Track(Id {
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

enum ListViewMsg {
    Success(Option<DataFrame>),
    Failed(String),
    Select,
    Query,
}

#[derive(PartialEq, Properties)]
pub struct ListViewProps {
    list: List,
}

struct ListView {
    data: Option<DataFrame>,
    select_ref: NodeRef,
    view: DataView,
    query_ref: NodeRef,
    error: Option<String>,
}

impl Component for ListView {
    type Message = ListViewMsg;
    type Properties = ListViewProps;

    fn create(ctx: &Context<Self>) -> Self {
        let list = ctx.props().list.clone();
        ctx.link().send_future(async move {
            match query_list(&list, None).await {
                Ok(data) => ListViewMsg::Success(data),
                Err(e) => ListViewMsg::Failed(e.as_string().unwrap()),
            }
        });
        Self {
            data: None,
            select_ref: NodeRef::default(),
            view: DataView::Table,
            query_ref: NodeRef::default(),
            error: None,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onchange = ctx.link().callback(|_| ListViewMsg::Select);
        let query = ctx.link().callback(|_| ListViewMsg::Query);
        html! {
            <div class="row">
                <div class="col-auto">
                    <select ref={self.select_ref.clone()} class="form-select mb-3" {onchange}>
                        <option selected=true>{"Table"}</option>
                        <option>{"Column Graph"}</option>
                        <option>{"Line Graph"}</option>
                        <option>{"Scatter Plot"}</option>
                        <option>{"Cumulative Line Graph"}</option>
                    </select>
                </div>
                <Input input_ref={self.query_ref.clone()} onclick={query.clone()} error={self.error.clone()} disabled={matches!(ctx.props().list.mode, ListMode::View(_))}/>
                if let Some(data) = &self.data {
                    {self.view.render(data)}
                }
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            ListViewMsg::Success(data) => {
                let Some(data) = data else {
                    return false;
                };
                self.data = {
                    let mut data = data.clone();
                    data.drop_in_place("id");
                    Some(data)
                };
                self.error = None;
            }
            ListViewMsg::Failed(e) => {
                self.error = Some(e);
            }
            ListViewMsg::Select => {
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
            ListViewMsg::Query => {
                let query = self.query_ref.cast::<HtmlSelectElement>().unwrap().value();
                let list = ctx.props().list.clone();
                ctx.link().send_future(async move {
                    match query_list(&list, Some(query)).await {
                        Ok(data) => ListViewMsg::Success(data),
                        Err(e) => ListViewMsg::Failed(e.as_string().unwrap()),
                    }
                });
            }
        }
        if let Some(data) = &self.data {
            if let Err(e) = self.view.draw(data) {
                self.error = Some(e.to_string());
            }
        }
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render || matches!(ctx.props().list.mode, ListMode::View(_)) {
            let query = self.query_ref.cast::<HtmlSelectElement>().unwrap();
            query.set_value(&ctx.props().list.query);
        }
    }
}

enum ListState {
    Fetching,
    Success(List),
    NotFound,
}

pub enum ListMsg {
    Load(List),
    NotFound,
}

#[derive(PartialEq, Properties)]
pub struct ListProps {
    pub view: ListsRoute,
    pub user: Rc<Option<User>>,
    pub dropdown: bool,
    pub show_dropdown: Rc<Callback<MouseEvent>>,
}

pub struct ListComponent {
    state: ListState,
}

impl Component for ListComponent {
    type Message = ListMsg;
    type Properties = ListProps;

    fn create(ctx: &Context<Self>) -> Self {
        let id = match &ctx.props().view {
            ListsRoute::List { id }
            | ListsRoute::View { id }
            | ListsRoute::Edit { id }
            | ListsRoute::Match { id }
            | ListsRoute::Tournament { id } => id.clone(),
        };
        ctx.link().send_future(async move {
            if let Some(list) = crate::fetch_list(&id).await.unwrap() {
                ListMsg::Load(list)
            } else {
                ListMsg::NotFound
            }
        });
        ListComponent {
            state: ListState::Fetching,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        if let ListState::NotFound = self.state {
            return not_found();
        }
        let query = ctx
            .link()
            .location()
            .unwrap()
            .query::<HashMap<String, String>>()
            .unwrap_or_default();
        let view = match &ctx.props().view.clone() {
            ListsRoute::View { .. } => ListPage::View,
            ListsRoute::List { .. } => ListPage::List,
            ListsRoute::Edit { .. } => ListPage::Edit,
            ListsRoute::Tournament { .. } => {
                if query.get("mode").map(String::as_str) == Some("random") {
                    ListPage::RandomTournament
                } else {
                    ListPage::Tournament
                }
            }
            ListsRoute::Match { .. } => {
                if query.get("mode").map(String::as_str) == Some("rounds") {
                    ListPage::RandomRounds
                } else {
                    ListPage::RandomMatches
                }
            }
        };
        match &self.state {
            ListState::NotFound => unreachable!(),
            ListState::Fetching => html! {},
            ListState::Success(list) => {
                let mut tabs = ["nav-link"; 3];
                let active = "nav-link active";
                match view {
                    ListPage::View => tabs[0] = active,
                    ListPage::List => tabs[1] = active,
                    ListPage::Edit => tabs[2] = active,
                    _ => {}
                }
                let component = if user_list(list, &ctx.props().user) {
                    match view {
                        ListPage::View => html! { <ListView list={list.clone()}/> },
                        ListPage::List => {
                            html! { <ListItems user={Rc::clone(&ctx.props().user)} list={list.clone()}/> }
                        }
                        ListPage::Edit => {
                            html! { <Edit logged_in={ctx.props().user.is_some()} list={list.clone()}/> }
                        }
                        ListPage::RandomMatches => html! { <RandomMatches id={list.id.clone()}/> },
                        ListPage::RandomRounds => html! { <RandomRounds id={list.id.clone()}/> },
                        ListPage::RandomTournament => {
                            html! { <RandomTournamentLoader list={list.clone()}/> }
                        }
                        ListPage::Tournament => html! { <TournamentLoader list={list.clone()}/> },
                    }
                } else {
                    match view {
                        ListPage::View => html! { <ListView list={list.clone()}/> },
                        ListPage::List => {
                            html! { <ListItems user={Rc::clone(&ctx.props().user)} list={list.clone()}/> }
                        }
                        // TODO: move this up?
                        _ => not_found(),
                    }
                };
                let toggle = match view {
                    ListPage::RandomMatches => "Random Matches",
                    ListPage::RandomRounds => "Random Rounds",
                    ListPage::Tournament => "Tournament",
                    ListPage::RandomTournament => "Random Tournament",
                    _ => "Rank",
                };
                let toggle_class = match (toggle, ctx.props().dropdown) {
                    ("Rank", false) => "nav-link dropdown-toggle",
                    ("Rank", true) => "nav-link dropdown-toggle show",
                    (_, false) => "nav-link active dropdown-toggle",
                    (_, true) => "nav-link active dropdown-toggle show",
                };
                let menu_class = if ctx.props().dropdown {
                    "dropdown-menu show"
                } else {
                    "dropdown-menu"
                };
                // TODO: handle GROUP BY queries
                let dropdown_html = if let ListMode::View(_) = list.mode {
                    html! {}
                } else {
                    html! {
                        <li class="nav-item dropdown">
                            <a class={toggle_class} href="#" onclick={(*ctx.props().show_dropdown).clone()}>{toggle}</a>
                            <ul class={menu_class}>
                                <li><Link<ListsRoute> classes="dropdown-item" to={ListsRoute::Tournament{ id: list.id.clone() }}>{"Tournament"}</Link<ListsRoute>></li>
                                <li><Link<ListsRoute, RouteQuery> classes="dropdown-item" to={ListsRoute::Tournament{ id: list.id.clone() }} query={Some(&[("mode", "random")][..])}>{"Random Tournament"}</Link<ListsRoute, RouteQuery>></li>
                                <li><Link<ListsRoute> classes="dropdown-item" to={ListsRoute::Match{ id: list.id.clone() }}>{"Random Matches"}</Link<ListsRoute>></li>
                                <li><Link<ListsRoute, RouteQuery> classes="dropdown-item" to={ListsRoute::Match{ id: list.id.clone() }} query={Some(&[("mode", "rounds")][..])}>{"Random Rounds"}</Link<ListsRoute, RouteQuery>></li>
                            </ul>
                        </li>
                    }
                };
                let user = user_list(list, &ctx.props().user);
                html! {
                  <Content
                    heading={list.name.clone()}
                    nav={html! {
                      <ul class="navbar-nav me-auto">
                        <li class="nav-item">
                          <Link<ListsRoute> classes={tabs[0]} to={ListsRoute::View{id: list.id.clone()}}>{"View"}</Link<ListsRoute>>
                        </li>
                        <li class="nav-item">
                          <Link<ListsRoute> classes={tabs[1]} to={ListsRoute::List{id: list.id.clone()}}>{"Items"}</Link<ListsRoute>>
                        </li>
                        if user {
                          {dropdown_html}
                          <li class="nav-item">
                            <Link<ListsRoute> classes={tabs[2]} to={ListsRoute::Edit{id: list.id.clone()}}>{"Settings"}</Link<ListsRoute>>
                          </li>
                        }
                      </ul>
                    }}
                    content={html! {
                      <>
                        if !user {
                          <h3>{&format!("{}'s list", list.user_id)}</h3>
                        }
                        {component}
                      </>
                    }}/>
                }
            }
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            ListMsg::Load(list) => {
                self.state = ListState::Success(list);
            }
            ListMsg::NotFound => {
                self.state = ListState::NotFound;
            }
        }
        true
    }

    // Navigation within the list page doesn't update the component so we need to implement changed
    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        // Should we compare IDs instead
        if ctx.props().view != old_props.view {
            let id = match &ctx.props().view {
                ListsRoute::List { id }
                | ListsRoute::View { id }
                | ListsRoute::Edit { id }
                | ListsRoute::Match { id }
                | ListsRoute::Tournament { id } => id.clone(),
            };
            ctx.link().send_future(async move {
                ListMsg::Load(crate::fetch_list(&id).await.unwrap().unwrap())
            });
        }
        // Rank dropdown breaks if this is set to false
        true
    }
}

fn nav_content(nav: Html, content: Html) -> Html {
    html! {
        <>
            <nav class="navbar navbar-expand navbar-bg py-2">
                <div class="container-fluid">
                    {nav}
                </div>
            </nav>
            <div class="main-bg container-fluid flex-grow-1 pt-3 overflow-y-auto">
                {content}
            </div>
        </>
    }
}

enum ContentMsg {
    Toggle,
}

#[derive(PartialEq, Properties)]
struct ContentProps {
    heading: String,
    nav: Html,
    content: Html,
}

struct Content {
    collapse: bool,
}

impl Component for Content {
    type Message = ContentMsg;
    type Properties = ContentProps;

    fn create(_: &Context<Self>) -> Self {
        Content { collapse: true }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let class = if self.collapse {
            "collapse navbar-collapse"
        } else {
            "navbar-collapse"
        };
        html! {
          <>
            <nav class="navbar navbar-expand-sm navbar-bg py-2" style="background-color: #2fb380;">
              <div class="container-fluid">
                <a class="navbar-brand" href="#">{&ctx.props().heading}</a>
                <button class="navbar-toggler" type="button" onclick={ctx.link().callback(|_| ContentMsg::Toggle)}>
                  if self.collapse {
                    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-plus-lg" viewBox="0 0 16 16">
                      <path fill-rule="evenodd" d="M8 2a.5.5 0 0 1 .5.5v5h5a.5.5 0 0 1 0 1h-5v5a.5.5 0 0 1-1 0v-5h-5a.5.5 0 0 1 0-1h5v-5A.5.5 0 0 1 8 2"/>
                    </svg>
                  } else {
                    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-dash-lg" viewBox="0 0 16 16">
                      <path fill-rule="evenodd" d="M2 8a.5.5 0 0 1 .5-.5h11a.5.5 0 0 1 0 1h-11A.5.5 0 0 1 2 8"/>
                    </svg>
                  }
                </button>
                <div {class}>
                  {ctx.props().nav.clone()}
                </div>
              </div>
            </nav>
            <div class="main-bg container-fluid flex-grow-1 overflow-y-auto">
              {ctx.props().content.clone()}
            </div>
          </>
        }
    }

    fn update(&mut self, _: &Context<Self>, _: Self::Message) -> bool {
        self.collapse = !self.collapse;
        true
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

async fn fetch_list(id: &str) -> Result<Option<List>, JsValue> {
    let window = window();
    let request = query(&format!("/api/lists/{}", id), "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    if resp.status() == 404 {
        return Ok(None);
    }
    let json = JsFuture::from(resp.json()?).await?;
    Ok(Some(serde_wasm_bindgen::from_value(json).unwrap()))
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

async fn query_list(list: &List, qs: Option<String>) -> Result<Option<DataFrame>, JsValue> {
    let window = window();
    let url = if let Some(qs) = qs {
        format!("/api/lists/{}/query?query={}", list.id, qs)
    } else {
        format!("/api/lists/{}/query", list.id)
    };
    let request = query(&url, "GET").unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    if [400, 500].contains(&resp.status()) {
        return Err(JsFuture::from(resp.text()?).await?);
    }
    Ok(serialize_into_df(resp).await?.map(|mut items| {
        if let Some(id_col) = items.column("id") {
            let ids: HashSet<_> = list.items.iter().map(|i| i.id.as_str()).collect();
            // inner join
            items.remove(
                id_col
                    .as_string::<i64>()
                    .iter()
                    .map(|id| ids.contains(id.unwrap()))
                    .collect(),
            );
        }
        items
    }))
}

async fn get_items(id: &str) -> Result<Items, JsValue> {
    let window = window();
    let request = query(&format!("/api/lists/{}/items", id), "GET").unwrap();
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

async fn find_items(search: &str) -> Result<Option<DataFrame>, JsValue> {
    let window = window();
    let request = query(&format!("/api/items?q=search&query={}", search), "GET")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    if [400, 500].contains(&resp.status()) {
        return Err(JsFuture::from(resp.text()?).await?);
    }
    serialize_into_df(resp).await
}

async fn serialize_into_df(resp: Response) -> Result<Option<DataFrame>, JsValue> {
    let buf = Uint8Array::new(&JsFuture::from(resp.array_buffer()?).await?).to_vec();
    if buf.is_empty() {
        return Ok(None);
    }
    let mut buf = Cursor::new(buf);
    Ok(Some(DataFrame::from(&mut buf)))
}

async fn delete_items(ids: &[String]) -> Result<(), JsValue> {
    let window = window();
    let request = query(&format!("/api/items?ids={}", ids.join(",")), "DELETE")?;
    JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
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

fn user_list(list: &List, user: &Option<User>) -> bool {
    Some(&list.user_id) == user.as_ref().as_ref().map(|u| &u.user_id)
        || (user.is_none() && list.user_id == "demo")
}

fn not_found() -> Html {
    html! {
        <h1>{"Not found"}</h1>
    }
}

fn window() -> Window {
    web_sys::window().expect("no global `window` exists")
}
