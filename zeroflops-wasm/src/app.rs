use crate::{
    base::Input,
    bootstrap::Modal,
    dataframe::DataFrame,
    docs,
    edit::Edit,
    home::Home,
    integrations::spotify::SpotifyIntegration,
    list,
    list::item::{ItemMode, ListItems},
    plot::DataView,
    random::{RandomMatches, RandomRounds},
    search::Search,
    settings::Settings,
    tournament::{RandomTournamentLoader, TournamentLoader},
    Content, ListsRoute, Route,
};
use std::{collections::HashMap, rc::Rc};
use web_sys::{HtmlSelectElement, MouseEvent};
use yew::{html, Callback, Component, Context, Html, NodeRef, Properties};
use yew_router::{
    prelude::{Link, Redirect, RouterScopeExt},
    BrowserRouter, Switch,
};
use zeroflops::{List, ListMode, User};

type RouteQuery = &'static [(&'static str, &'static str)];

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
        Route::ListsRoot => html! { <list::Lists {logged_in}/> },
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

pub enum Msg {
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

pub struct App {
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
            match crate::get_user().await {
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        let window = crate::window();
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
            match crate::query_list(&list, None).await {
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
                    "CSV" => DataView::Csv,
                    _ => unreachable!(),
                };
            }
            ListViewMsg::Query => {
                let query = self.query_ref.cast::<HtmlSelectElement>().unwrap().value();
                let list = ctx.props().list.clone();
                ctx.link().send_future(async move {
                    match crate::query_list(&list, Some(query)).await {
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
                        <option>{"CSV"}</option>
                    </select>
                </div>
                <Input input_ref={self.query_ref.clone()} onclick={query.clone()} error={self.error.clone()} disabled={matches!(ctx.props().list.mode, ListMode::View(_))}/>
                if let Some(data) = &self.data {
                    {self.view.render(data)}
                }
            </div>
        }
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
    SelectView,
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
    select_ref: NodeRef,
    mode: ItemMode,
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
            select_ref: NodeRef::default(),
            mode: ItemMode::View,
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            ListMsg::Load(list) => {
                self.mode = if let ListMode::View(_) = list.mode {
                    ItemMode::View
                } else {
                    ItemMode::Update
                };
                self.state = ListState::Success(list);
            }
            ListMsg::NotFound => {
                self.state = ListState::NotFound;
            }
            ListMsg::SelectView => {
                self.mode = match self
                    .select_ref
                    .cast::<HtmlSelectElement>()
                    .map(|s| s.value())
                    .as_deref()
                    .unwrap_or("Update")
                {
                    "Update" => ItemMode::Update,
                    "Delete" => ItemMode::Delete,
                    _ => unreachable!(),
                };
            }
        }
        true
    }

    // Navigation within the list page doesn't update the component, so we need to implement changed
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        let list = match &self.state {
            ListState::NotFound => return crate::not_found(),
            ListState::Fetching => return html! {},
            ListState::Success(list) => list,
        };
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
        let mut tabs = ["nav-link"; 3];
        let active = "nav-link active";
        match view {
            ListPage::View => tabs[0] = active,
            ListPage::List => tabs[1] = active,
            ListPage::Edit => tabs[2] = active,
            _ => {}
        }
        let component = if crate::user_list(list, &ctx.props().user) {
            match view {
                ListPage::View => html! { <ListView list={list.clone()}/> },
                ListPage::List => {
                    html! { <ListItems user={Rc::clone(&ctx.props().user)} list={list.clone()} mode={self.mode.clone()}/> }
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
                    html! { <ListItems user={Rc::clone(&ctx.props().user)} list={list.clone()} mode={self.mode.clone()}/> }
                }
                // TODO: move this up?
                _ => crate::not_found(),
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
        let user = crate::user_list(list, &ctx.props().user);
        html! {
          <Content
            heading={list.name.clone()}
            nav={html! {
              <>
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
                if matches!(view, ListPage::List) && !matches!(list.mode, ListMode::View(_)) {
                  <div class="d-flex gap-3 align-items-baseline">
                    <span class="navbar-text text-nowrap">{"Item Mode:"}</span>
                    <select ref={self.select_ref.clone()} class="form-select" onchange={ctx.link().callback(|_| ListMsg::SelectView)}>
                      <option selected=true>{"Update"}</option>
                      <option>{"Delete"}</option>
                    </select>
                  </div>
                }
              </>
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
