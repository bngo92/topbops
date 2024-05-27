use crate::{bootstrap::Accordion, UserProps};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlSelectElement, Response};
use yew::{html, Component, Context, Html, NodeRef};
use zeroflops::{
    spotify::{Playlists, RecentTracks},
    Spotify,
};

pub enum Msg {
    None,
    LoadRecentTracks(RecentTracks),
    LoadPlaylists(Playlists),
    Import,
    ImportTrack(String),
}

pub struct SpotifyIntegration {
    import_ref: NodeRef,
    recent_tracks: Option<RecentTracks>,
    playlists: Option<Playlists>,
}

impl Component for SpotifyIntegration {
    type Message = Msg;
    type Properties = UserProps;

    fn create(ctx: &Context<Self>) -> Self {
        if ctx.props().logged_in {
            ctx.link()
                .send_future(async { Msg::LoadRecentTracks(get_recent_tracks().await.unwrap()) });
            ctx.link()
                .send_future(async { Msg::LoadPlaylists(get_playlists().await.unwrap()) });
        }
        SpotifyIntegration {
            import_ref: NodeRef::default(),
            recent_tracks: None,
            playlists: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => {}
            Msg::LoadRecentTracks(tracks) => self.recent_tracks = Some(tracks),
            Msg::LoadPlaylists(playlists) => self.playlists = Some(playlists),
            Msg::Import => {
                let input = self.import_ref.cast::<HtmlSelectElement>().unwrap().value();
                // TODO: handle bad input
                let (source, id) = match crate::parse_spotify_source(input) {
                    Some(Spotify::Playlist(id)) => ("spotify:playlist", id),
                    Some(Spotify::Album(id)) => ("spotify:album", id),
                    Some(Spotify::Track(id)) => ("spotify:track", id),
                    None => {
                        return false;
                    }
                };
                ctx.link().send_future(async move {
                    crate::import_list(source, &id.id).await.unwrap();
                    Msg::None
                });
            }
            Msg::ImportTrack(input) => {
                // TODO: handle bad input
                let (source, id) = match crate::parse_spotify_source(input) {
                    Some(Spotify::Track(id)) => ("spotify:track", id),
                    _ => {
                        return false;
                    }
                };
                ctx.link().send_future(async move {
                    crate::import_list(source, &id.id).await.unwrap();
                    // TODO: refresh row
                    Msg::None
                });
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let default_import =
            "https://open.spotify.com/playlist/5MztFbRbMpyxbVYuOSfQV9?si=9db089ab25274efa";
        let track_html = if let Some(tracks) = &self.recent_tracks {
            tracks
                .tracks
                .iter()
                .map(|i| {
                    let url = i.url.clone();
                    let import_track = ctx.link().callback(move |_| Msg::ImportTrack(url.clone()));
                    html! {
                        <div class="row">
                            <div class="col">
                                 <a href={i.url.clone()}>{&i.name}</a>
                                 if i.user_score.is_none() {
                                     <button type="button" class="btn btn-success" onclick={import_track}>{"Import"}</button>
                                 }
                            </div>
                            <div class="col-1">{i.rating}</div>
                            <div class="col-1">{i.user_score}</div>
                        </div>
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let import = ctx.link().callback(|_| Msg::Import);
        crate::nav_content(
            html! {
              <ul class="navbar-nav me-auto">
                <li class="navbar-brand">{"Spotify"}</li>
              </ul>
            },
            html! {
              <div>
                <Accordion header={"Recent Tracks"} collapsed={false}>
                  if ctx.props().logged_in {
                    <div class="row">
                      <div class="col"></div>
                      <div class="col-1"><strong>{"Rating"}</strong></div>
                      <div class="col-1"><strong>{"User Score"}</strong></div>
                    </div>
                    {for track_html}
                  } else {
                    <p>{"Create an account to view and import tracks that were recently played in Spotify"}</p>
                  }
                </Accordion>
                <Accordion header={"Saved Playlists"} collapsed={false}>
                  if ctx.props().logged_in {
                    if let Some(playlists) = &self.playlists {
                      {for playlists.items.iter().map(|i| html! {<div><a href={i.external_urls["spotify"].clone()}>{&i.name}</a></div>})}
                    }
                  } else {
                    <p>{"Create an account to import playlists from Spotify"}</p>
                  }
                </Accordion>
                <h2>{"Import from Spotify link"}</h2>
                <form>
                  <div class="row">
                    <div class="col-12 col-md-8 col-lg-9">
                      <input ref={self.import_ref.clone()} type="text" class="w-100 h-100" value={default_import}/>
                    </div>
                    <div class="col-2 col-lg-1 pe-2">
                      <button type="button" class="btn btn-success" onclick={import} disabled={!ctx.props().logged_in}>{"Import"}</button>
                    </div>
                  </div>
                </form>
              </div>
            },
        )
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
