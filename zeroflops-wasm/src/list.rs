use crate::{Route, UserProps};
use web_sys::HtmlSelectElement;
use yew::{html, Component, Context, Html, NodeRef};
use yew_router::prelude::Link;
use yew_router::scope_ext::RouterScopeExt;
use zeroflops::{List, Spotify};

pub mod item;

pub enum ListsMsg {
    Load(Vec<List>),
    Create,
    Import,
}

pub struct Lists {
    lists: Vec<List>,
    import_ref: NodeRef,
}

impl Component for Lists {
    type Message = ListsMsg;
    type Properties = UserProps;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_future(async move {
            let lists = crate::fetch_lists(false).await.unwrap();
            ListsMsg::Load(lists)
        });
        Lists {
            lists: Vec::new(),
            import_ref: NodeRef::default(),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let list_html = self.lists.iter().map(|l| {
            html! {
                <div class="col-12 col-md-6 mb-4">
                    <div class="card">
                        <div class="card-body">
                            <Link<Route> to={Route::View{id: l.id.clone()}}>{&l.name}</Link<Route>>
                        </div>
                    </div>
                </div>
            }
        });
        let disabled = !ctx.props().logged_in;
        let default_import = if disabled {
            "Not supported in demo"
        } else {
            "https://open.spotify.com/playlist/5MztFbRbMpyxbVYuOSfQV9?si=9db089ab25274efa"
        };
        let create = ctx.link().callback(|_| ListsMsg::Create);
        let import = ctx.link().callback(|_| ListsMsg::Import);
        html! {
          <div>
            <h1>{"All Lists"}</h1>
            <div class="row mt-3">
              {for list_html}
            </div>
            <button type="button" class="btn btn-primary" onclick={create} {disabled}>{"Create List"}</button>
            <h1>{"My Spotify Playlists"}</h1>
            <form>
              <div class="row">
                <div class="col-12 col-md-8 col-lg-9">
                  <input ref={self.import_ref.clone()} type="text" class="w-100 h-100" value={default_import} {disabled}/>
                </div>
                <div class="col-2 col-lg-1 pe-2">
                  <button type="button" class="btn btn-success" onclick={import} {disabled}>{"Save"}</button>
                </div>
              </div>
            </form>
          </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            ListsMsg::Load(lists) => {
                self.lists = lists;
                true
            }
            ListsMsg::Create => {
                let navigator = ctx.link().navigator().unwrap();
                ctx.link().send_future_batch(async move {
                    let list = crate::create_list().await.unwrap();
                    navigator.push(&Route::Edit { id: list.id });
                    None
                });
                false
            }
            ListsMsg::Import => {
                let input = self.import_ref.cast::<HtmlSelectElement>().unwrap().value();
                // TODO: handle bad input
                let (source, id) = match crate::parse_spotify_source(input) {
                    Some(Spotify::Playlist(id)) => ("spotify:playlist", id),
                    Some(Spotify::Album(id)) => ("spotify:album", id),
                    None => {
                        return false;
                    }
                };
                ctx.link().send_future(async move {
                    crate::import_list(source, &id.id).await.unwrap();
                    let lists = crate::fetch_lists(false).await.unwrap();
                    ListsMsg::Load(lists)
                });
                false
            }
        }
    }
}
