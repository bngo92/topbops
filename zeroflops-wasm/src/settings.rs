use yew::{html, Component, Context, Html, Properties};
use zeroflops::User;

#[derive(Eq, PartialEq, Properties)]
pub struct SettingsProps {
    pub user: User,
}

pub struct Settings;

impl Component for Settings {
    type Message = ();
    type Properties = SettingsProps;

    fn create(_: &Context<Self>) -> Self {
        Settings
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let window = crate::window();
        let location = window.location();
        // TODO: let you remove integrations
        // Should we link to Google profile?
        crate::nav_content(
            html! {
              <ul class="navbar-nav me-auto">
                <li class="navbar-brand">{"Settings"}</li>
              </ul>
            },
            html! {
              <div>
                <h1>{"Integrations"}</h1>
                <h2>{"Spotify"}</h2>
                if let (Some(url), Some(user)) = (&ctx.props().user.spotify_url, &ctx.props().user.spotify_user) {
                  <a href={url.clone()}>{&user}</a>
                } else {
                  <a class="btn btn-success" href={format!("https://accounts.spotify.com/authorize?client_id=ee3d1b4f8d80477ea48743a511ef3018&redirect_uri={}/api/login&response_type=code&scope=playlist-modify-public playlist-modify-private user-read-recently-played playlist-read-private", location.origin().unwrap().as_str())}>{"Log in with Spotify"}</a>
                }
                <h2>{"Google"}</h2>
                if let Some(google_email) = &ctx.props().user.google_email {
                  <p>{google_email}</p>
                } else {
                  <a class="btn btn-success" href={format!("https://accounts.google.com/o/oauth2/v2/auth?client_id=1038220726403-n55jha2cvprd8kdb4akdfvo0uiok4p5u.apps.googleusercontent.com&redirect_uri={}/api/login/google&response_type=code&scope=email", location.origin().unwrap().as_str())}>{"Log in with Google"}</a>
                }
              </div>
            },
        )
    }
}
