#![feature(iter_intersperse)]
use crate::{app::App, dataframe::DataFrame};
use arrow::array::AsArray;
use js_sys::Uint8Array;
use regex::Regex;
use std::{collections::HashSet, io::Cursor};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response, Window};
use yew::{html, Component, Context, Html, Properties};
use yew_router::Routable;
use zeroflops::{Id, Items, List, ListMode, Lists, Spotify, User};

mod app;
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

#[derive(Eq, PartialEq, Properties)]
pub struct UserProps {
    logged_in: bool,
}

pub fn parse_spotify_source(input: String) -> Option<Spotify> {
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

pub fn parse_setlist_source(input: String) -> Option<Id> {
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

    fn update(&mut self, _: &Context<Self>, _: Self::Message) -> bool {
        self.collapse = !self.collapse;
        true
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
            <div class="main-bg container-fluid flex-grow-1 pt-3 overflow-y-auto">
              {ctx.props().content.clone()}
            </div>
          </>
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

async fn create_list(query: Option<String>) -> Result<List, JsValue> {
    let window = window();
    let request = Request::new_with_str_and_init(
        &if let Some(query) = query {
            format!("/api/lists?query={query}")
        } else {
            String::from("/api/lists")
        },
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
