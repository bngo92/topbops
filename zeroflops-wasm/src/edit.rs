use web_sys::{HtmlInputElement, HtmlSelectElement};
use yew::{html, Component, Context, Html, NodeRef, Properties};
use yew_router::scope_ext::RouterScopeExt;
use zeroflops::{Id, List, ListMode, Source, SourceType, Spotify};

use crate::Route;

pub enum Msg {
    None,
    AddSource,
    DeleteSource(usize),
    Save,
    Delete,
}

// TODO: need to refresh list after edit
#[derive(Eq, PartialEq, Properties)]
pub struct EditProps {
    pub list: List,
}

pub struct Edit {
    list: List,
    sources: Vec<(NodeRef, NodeRef, Option<SourceType>)>,
    name_ref: NodeRef,
    external_ref: NodeRef,
    query_ref: NodeRef,
    favorite_ref: NodeRef,
}

impl Component for Edit {
    type Message = Msg;
    type Properties = EditProps;

    fn create(ctx: &Context<Self>) -> Self {
        let mut list = ctx.props().list.clone();
        let sources = list
            .sources
            .drain(..)
            .map(|s| (NodeRef::default(), NodeRef::default(), Some(s.source_type)))
            .collect();
        Edit {
            list,
            sources,
            name_ref: NodeRef::default(),
            external_ref: NodeRef::default(),
            query_ref: NodeRef::default(),
            favorite_ref: NodeRef::default(),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let disabled = crate::get_user().is_none();
        let source_html = self.sources
            .iter()
            .enumerate()
            .map(|(i, (source_ref, id, source))| {
                let mut selected = [false; 3];
                let value = match source {
                    None => {
                        selected[1] = true;
                        String::new()
                    }
                    Some(SourceType::Custom(value)) => {
                        selected[0] = true;
                        value.to_string()
                    }
                    Some(
                        SourceType::Spotify(Spotify::Playlist(Id { raw_id, .. }))
                        | SourceType::Spotify(Spotify::Album(Id { raw_id, .. })),
                    ) => {
                        selected[1] = true;
                        raw_id.clone()
                    }
                    Some(SourceType::Setlist(Id { raw_id, .. })) => {
                        selected[2] = true;
                        raw_id.clone()
                    }
                };
                let onclick = ctx.link().callback(move |_| Msg::DeleteSource(i));
                html! {
                    <div class="row mb-1">
                        <div class="col-4 col-sm-3 col-md-2">
                            <select ref={source_ref} class="form-select">
                                <option selected={selected[0]}>{"Custom"}</option>
                                <option selected={selected[1]}>{"Spotify"}</option>
                                <option selected={selected[2]}>{"Setlist"}</option>
                            </select>
                        </div>
                        <input class="col-9 col-sm-7 col-md-8" {value} ref={id}/>
                        <div class="col-auto">
                            <button type="button" class="btn btn-danger" {onclick}>{"Delete"}</button>
                        </div>
                    </div>
                }
            });
        let mode = match self.list.mode {
            ListMode::User(_) => "User",
            ListMode::External => "External",
            ListMode::View => "View",
        };
        let checked = self.list.favorite;
        let add_source = ctx.link().callback(|_| Msg::AddSource);
        let save = ctx.link().callback(|_| Msg::Save);
        let delete = ctx.link().callback(|_| Msg::Delete);
        html! {
            <div>
                <h4>{"List Settings"}</h4>
                <form>
                    <div class="form-floating mb-2 col-md-8">
                        if let ListMode::External = &self.list.mode {
                            <input type="text" readonly=true class="form-control-plaintext" id="name" value={self.list.name.clone()} placeholder=""/>
                        } else {
                            <input type="text" class="form-control" id="name" value={self.list.name.clone()} ref={&self.name_ref} placeholder=""/>
                        }
                        <label for="name">{"List name"}</label>
                    </div>
                    <div class="form-floating mb-2 col-md-8">
                        <input type="text" readonly=true class="form-control-plaintext" id="mode" value={mode} placeholder=""/>
                        <label for="mode">{"List mode"}</label>
                    </div>
                    if let ListMode::User(external_id) = &self.list.mode {
                        <div class="form-floating mb-3 col-md-8">
                            <input class="form-control" id="externalId" value={external_id.as_ref().map(|id| id.raw_id.clone())} ref={&self.external_ref} placeholder="External ID"/>
                            <label for="externalId">{"External ID"}</label>
                        </div>
                    }
                    <div class="form-floating mb-3 col-md-8">
                        <input class="form-control" id="query" value={self.list.query.clone()} ref={&self.query_ref} placeholder="External ID"/>
                        <label for="query">{"Query"}</label>
                    </div>
                    <div class="form-check">
                        <label class="form-check-label" for="favorite">{"Favorite"}</label>
                        <input ref={&self.favorite_ref} class="form-check-input" type="checkbox" id="favorite" {checked}/>
                    </div>
                </form>
                <hr/>
                <h4>{"Data Sources"}</h4>
                <div class="mb-3">
                    {for source_html}
                </div>
                <div class="row mb-3">
                    <div class="col-auto">
                        <button type="button" class="btn btn-primary" onclick={add_source}>{"Add source"}</button>
                    </div>
                    <div class="col-auto">
                        <button type="button" class="btn btn-success" onclick={save} {disabled}>{"Save all settings"}</button>
                    </div>
                </div>
                <hr/>
                <h4>{"Delete List"}</h4>
                <div>
                    <button type="button" class="btn btn-danger" onclick={delete}>{"Delete"}</button>
                </div>
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => false,
            Msg::AddSource => {
                self.sources
                    .push((NodeRef::default(), NodeRef::default(), None));
                true
            }
            Msg::DeleteSource(i) => {
                self.sources.remove(i);
                true
            }
            Msg::Save => {
                if !matches!(self.list.mode, ListMode::External) {
                    self.list.name = self.name_ref.cast::<HtmlInputElement>().unwrap().value();
                }
                if let ListMode::User(external_id) = &mut self.list.mode {
                    let id = self
                        .external_ref
                        .cast::<HtmlInputElement>()
                        .unwrap()
                        .value();
                    if id.is_empty() {
                        *external_id = None;
                    } else if let Some(Spotify::Playlist(id)) = crate::parse_spotify_source(id) {
                        *external_id = Some(id);
                    }
                }
                self.list.query = self.query_ref.cast::<HtmlInputElement>().unwrap().value();
                self.list.favorite = self
                    .favorite_ref
                    .cast::<HtmlInputElement>()
                    .unwrap()
                    .checked();
                self.list.sources.clear();
                for (source, id, _) in &self.sources {
                    let source = source.cast::<HtmlSelectElement>().unwrap().value();
                    let id = id.cast::<HtmlInputElement>().unwrap().value();
                    match &*source {
                        "Spotify" => {
                            if let Some(source) = crate::parse_spotify_source(id) {
                                self.list.sources.push(Source {
                                    source_type: SourceType::Spotify(source),
                                    name: String::new(),
                                });
                            } else {
                                return false;
                            }
                        }
                        "Custom" => {
                            if let Ok(json) = serde_json::from_str(&id) {
                                self.list.sources.push(Source {
                                    source_type: SourceType::Custom(json),
                                    name: String::new(),
                                });
                            } else {
                                return false;
                            }
                        }
                        "Setlist" => {
                            if let Some(id) = crate::parse_setlist_source(id) {
                                self.list.sources.push(Source {
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
                let list = self.list.clone();
                ctx.link().send_future(async move {
                    crate::update_list(&list).await.unwrap();
                    Msg::None
                });
                false
            }
            Msg::Delete => {
                let id = self.list.id.clone();
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
