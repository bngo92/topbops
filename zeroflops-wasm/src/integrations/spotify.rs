use crate::bootstrap::Accordion;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;
use yew::{html, Component, Context, Html};
use zeroflops::spotify::{Playlists, RecentTracks};

pub enum Msg {
    LoadRecentTracks(RecentTracks),
    LoadPlaylists(Playlists),
}

pub struct Spotify {
    recent_tracks: Option<RecentTracks>,
    playlists: Option<Playlists>,
}

impl Component for Spotify {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link()
            .send_future(async { Msg::LoadRecentTracks(get_recent_tracks().await.unwrap()) });
        ctx.link()
            .send_future(async { Msg::LoadPlaylists(get_playlists().await.unwrap()) });
        Spotify {
            recent_tracks: None,
            playlists: None,
        }
    }

    fn view(&self, _: &Context<Self>) -> Html {
        html! {
            <div>
                <h1>{"Spotify"}</h1>
                <Accordion header={"Recent Tracks"} collapsed={false}>
                    <div class="row">
                        <div class="col"></div>
                        <div class="col-1"><strong>{"Rating"}</strong></div>
                        <div class="col-1"><strong>{"User Score"}</strong></div>
                    </div>
                    // TODO: add buttons
                    if let Some(tracks) = &self.recent_tracks {
                        {for tracks.tracks.iter().map(
                            |i| html! {
                                <div class="row">
                                    <div class="col"><a href={i.url.clone()}>{&i.name}</a></div>
                                    <div class="col-1">{i.rating}</div>
                                    <div class="col-1">{i.user_score}</div>
                                </div>
                            })}
                    }
                </Accordion>
                <Accordion header={"Saved Playlists"} collapsed={false}>
                    if let Some(playlists) = &self.playlists {
                        {for playlists.items.iter().map(|i| html! {<div><a href={i.external_urls["spotify"].clone()}>{&i.name}</a></div>})}
                    }
                </Accordion>
            </div>
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadRecentTracks(tracks) => self.recent_tracks = Some(tracks),
            Msg::LoadPlaylists(playlists) => self.playlists = Some(playlists),
        }
        true
    }
}

async fn get_recent_tracks() -> Result<RecentTracks, JsValue> {
    let window = crate::window();
    let request = crate::query("/api/spotify/recentTracks", "GET").unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}

async fn get_playlists() -> Result<Playlists, JsValue> {
    let window = crate::window();
    let request = crate::query("/api/spotify/playlists", "GET").unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let json = JsFuture::from(resp.json()?).await?;
    Ok(serde_wasm_bindgen::from_value(json).unwrap())
}
