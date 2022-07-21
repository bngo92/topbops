#![feature(async_closure)]
use rand::prelude::SliceRandom;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use yew::{html, Callback, Component, Context, Html, MouseEvent, Properties};

pub struct App;

impl Component for App {
    type Message = ();
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        App
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
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
    LoadHome,
}

pub struct State {
    current_page: Page,
}

impl Component for State {
    type Message = StateMsg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        State {
            current_page: Page::Login,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        if let Page::Login = self.current_page {
            html! {
                <Login on_demo_select={ctx.link().callback(|_| StateMsg::LoadHome)}/>
            }
        } else {
            html! {
                <Home/>
            }
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        self.current_page = Page::Home;
        true
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

    fn create(ctx: &Context<Self>) -> Self {
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

pub struct Home;

impl Component for Home {
    type Message = ();
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
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
            <div class="row"></div>
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

// Called by our JS entry point to run the example
#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue> {
    yew::start_app::<App>();
    Ok(())
}
