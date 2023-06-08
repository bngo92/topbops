use topbops::{List, ListMode, Source, SourceType, Spotify};
use web_sys::{HtmlInputElement, HtmlSelectElement};
use yew::{html, Component, Context, Html, NodeRef, Properties};
use yew_router::scope_ext::RouterScopeExt;

use crate::Route;

pub enum Msg {
    None,
    AddSource,
    DeleteSource(usize),
    DeleteNewSource(usize),
    Save,
    Delete,
}

// TODO: need to refresh list after edit
#[derive(Eq, PartialEq, Properties)]
pub struct EditProps {
    pub list: List,
}

pub struct Edit {
    state: (List, Vec<(NodeRef, NodeRef)>, NodeRef, NodeRef, NodeRef),
}

impl Component for Edit {
    type Message = Msg;
    type Properties = EditProps;

    fn create(ctx: &Context<Self>) -> Self {
        Edit {
            state: (
                ctx.props().list.clone(),
                Vec::new(),
                NodeRef::default(),
                NodeRef::default(),
                NodeRef::default(),
            ),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let disabled = crate::get_user().is_none();
        let (list, new_sources, name_ref, external_ref, favorite_ref) = &self.state;
        let source_html = list.sources.iter().enumerate().map(|(i, source)| {
            let onclick = ctx.link().callback(move |_| Msg::DeleteSource(i));
            html! {
                <div class="row mb-1">
                    <label class="col-9 col-sm-10 col-form-label">{&source.name}</label>
                    <div class="col-2">
                        <button type="button" class="btn btn-danger" {onclick}>{"Delete"}</button>
                    </div>
                </div>
            }
        });
        let new_source_html = new_sources.iter().enumerate().map(|(i, (source, id))| {
            let onclick = ctx.link().callback(move |_| Msg::DeleteNewSource(i));
            html! {
                <div class="row mb-1">
                    <div class="col-4 col-sm-3 col-md-2">
                        <select ref={source} class="form-select">
                            <option>{"Custom"}</option>
                            <option>{"Spotify"}</option>
                            <option>{"Setlist"}</option>
                        </select>
                    </div>
                    <input class="col-9 col-sm-7 col-md-8 col-form-label" ref={id}/>
                    <div class="col-2">
                        <button type="button" class="btn btn-danger" {onclick}>{"Delete"}</button>
                    </div>
                </div>
            }
        });
        let checked = list.favorite;
        let add_source = ctx.link().callback(|_| Msg::AddSource);
        let save = ctx.link().callback(|_| Msg::Save);
        let delete = ctx.link().callback(|_| Msg::Delete);
        html! {
            <div>
                <h4>{"List Settings"}</h4>
                <form>
                    <div class="form-floating mb-3 col-md-8">
                        <input class="form-control" id="name" value={Some(list.name.clone())} ref={name_ref.clone()} placeholder="Name"/>
                        <label for="name">{"List name"}</label>
                    </div>
                    if let ListMode::User(external_id) = &list.mode {
                        <div class="form-floating mb-3 col-md-8">
                            <input class="form-control" id="externalId" value={external_id.as_ref().map(|id| id.raw_id.clone())} ref={external_ref.clone()} placeholder="External ID"/>
                            <label for="externalId">{"External ID"}</label>
                        </div>
                    }
                    <div class="form-check">
                        <label class="form-check-label" for="favorite">{"Favorite"}</label>
                        <input ref={favorite_ref} class="form-check-input" type="checkbox" id="favorite" {checked}/>
                    </div>
                </form>
                <hr/>
                <h4>{"Data Sources"}</h4>
                {for source_html}
                {for new_source_html}
                <button type="button" class="btn btn-primary" onclick={add_source}>{"Add source"}</button>
                <button type="button" class="btn btn-success" onclick={save} {disabled}>{"Save"}</button>
                <div>
                    <button type="button" class="btn btn-danger" onclick={delete}>{"Delete list"}</button>
                </div>
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => false,
            Msg::AddSource => {
                let (_, new_sources, _, _, _) = &mut self.state;
                new_sources.push((NodeRef::default(), NodeRef::default()));
                true
            }
            Msg::DeleteSource(i) => {
                let (list, _, _, _, _) = &mut self.state;
                list.sources.remove(i);
                true
            }
            Msg::DeleteNewSource(i) => {
                let (_, new_sources, _, _, _) = &mut self.state;
                new_sources.remove(i);
                true
            }
            Msg::Save => {
                let (list, new_refs, name_ref, external_ref, favorite_ref) = &mut self.state;
                if !matches!(list.mode, ListMode::External) {
                    list.name = name_ref.cast::<HtmlInputElement>().unwrap().value();
                }
                if let ListMode::User(external_id) = &mut list.mode {
                    let id = external_ref.cast::<HtmlInputElement>().unwrap().value();
                    if id.is_empty() {
                        *external_id = None;
                    } else if let Some(Spotify::Playlist(id)) = crate::parse_spotify_source(id) {
                        *external_id = Some(id);
                    }
                }
                list.favorite = favorite_ref.cast::<HtmlInputElement>().unwrap().checked();
                for (source, id) in new_refs {
                    let source = source.cast::<HtmlSelectElement>().unwrap().value();
                    let id = id.cast::<HtmlInputElement>().unwrap().value();
                    match &*source {
                        "Spotify" => {
                            if let Some(source) = crate::parse_spotify_source(id) {
                                list.sources.push(Source {
                                    source_type: SourceType::Spotify(source),
                                    name: String::new(),
                                });
                            } else {
                                return false;
                            }
                        }
                        "Custom" => {
                            if let Ok(json) = serde_json::from_str(&id) {
                                list.sources.push(Source {
                                    source_type: SourceType::Custom(json),
                                    name: String::new(),
                                });
                            } else {
                                return false;
                            }
                        }
                        "Setlist" => {
                            if let Some(id) = crate::parse_setlist_source(id) {
                                list.sources.push(Source {
                                    source_type: SourceType::Setlist(id),
                                    name: String::new(),
                                });
                            } else {
                                return false;
                            }
                        }
                        _ => {
                            return false;
                        }
                    };
                }
                let list = list.clone();
                ctx.link().send_future(async move {
                    crate::update_list(&list.id, list.clone()).await.unwrap();
                    Msg::None
                });
                false
            }
            Msg::Delete => {
                let id = self.state.0.id.clone();
                if crate::window()
                    .confirm_with_message(&format!("Delete {id}?"))
                    .unwrap()
                {
                    let navigator = ctx.link().navigator().unwrap();
                    ctx.link().send_future(async move {
                        crate::delete_list(&id).await.unwrap();
                        navigator.push(&Route::Home);
                        Msg::None
                    });
                }
                false
            }
        }
    }
}
