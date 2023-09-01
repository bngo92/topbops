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
    DeleteAll,
}

// TODO: need to refresh list after edit
#[derive(Eq, PartialEq, Properties)]
pub struct EditProps {
    pub logged_in: bool,
    pub list: List,
}

pub struct Edit {
    counter: i32,
    list: List,
    sources: Vec<(i32, NodeRef, NodeRef, Option<SourceType>)>,
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
        let sources: Vec<_> = list
            .sources
            .drain(..)
            .enumerate()
            .map(|(i, s)| {
                (
                    i as i32,
                    NodeRef::default(),
                    NodeRef::default(),
                    Some(s.source_type),
                )
            })
            .collect();
        Edit {
            counter: sources.len() as i32,
            list,
            sources,
            name_ref: NodeRef::default(),
            external_ref: NodeRef::default(),
            query_ref: NodeRef::default(),
            favorite_ref: NodeRef::default(),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let disabled = !ctx.props().logged_in;
        let source_html = self.sources
            .iter()
            .enumerate()
            .map(|(i, (key, source_ref, id, source))| {
                let mut selected = [false; 4];
                match source {
                    None => selected[1] = true,
                    Some(SourceType::Custom(_)) => selected[0] = true,
                    Some(SourceType::Spotify(_)) => selected[1] = true,
                    Some(SourceType::Setlist(_)) => selected[2] = true,
                    Some(SourceType::ListItems(_)) => selected[3] = true,
                };
                let onclick = ctx.link().callback(move |_| Msg::DeleteSource(i));
                html! {
                    <div class="row mb-1" key={*key}>
                        <div class="col-4 col-sm-3 col-md-2">
                            <select ref={source_ref} class="form-select">
                                <option selected={selected[0]}>{"Custom"}</option>
                                <option selected={selected[1]}>{"Spotify"}</option>
                                <option selected={selected[2]}>{"Setlist"}</option>
                                <option selected={selected[3]}>{"List Items"}</option>
                            </select>
                        </div>
                        <input class="col-9 col-sm-7 col-md-8" ref={id}/>
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
        let add_source = ctx.link().callback(|_| Msg::AddSource);
        let save = ctx.link().callback(|_| Msg::Save);
        let delete = ctx.link().callback(|_| Msg::Delete);
        let delete_all = ctx.link().callback(|_| Msg::DeleteAll);
        html! {
            <div>
                <h4>{"List Settings"}</h4>
                <form>
                    <div class="form-floating mb-2 col-md-8">
                        if let ListMode::External = &self.list.mode {
                            <input type="text" readonly=true class="form-control-plaintext" id="name" value={self.list.name.clone()} placeholder=""/>
                        } else {
                            <input type="text" class="form-control" id="name" ref={&self.name_ref} placeholder=""/>
                        }
                        <label for="name">{"List name"}</label>
                    </div>
                    <div class="form-floating mb-2 col-md-8">
                        <input type="text" readonly=true class="form-control-plaintext" id="mode" value={mode} placeholder=""/>
                        <label for="mode">{"List mode"}</label>
                    </div>
                    if let ListMode::User(_) = &self.list.mode {
                        <div class="form-floating mb-3 col-md-8">
                            <input class="form-control" id="externalId" ref={&self.external_ref} placeholder="External ID"/>
                            <label for="externalId">{"External ID"}</label>
                        </div>
                    }
                    <div class="form-floating mb-3 col-md-8">
                        <input class="form-control" id="query" ref={&self.query_ref} placeholder="External ID"/>
                        <label for="query">{"Query"}</label>
                    </div>
                    <div class="form-check">
                        <label class="form-check-label" for="favorite">{"Favorite"}</label>
                        <input ref={&self.favorite_ref} class="form-check-input" type="checkbox" id="favorite"/>
                    </div>
                </form>
                <hr/>
                <h4>{"Data Sources"}</h4>
                <div class="mb-3">
                    {for source_html}
                </div>
                <div class="d-flex gap-3">
                    <button type="button" class="btn btn-primary" onclick={add_source}>{"Add source"}</button>
                    <button type="button" class="btn btn-success" onclick={save} {disabled}>{"Save all settings"}</button>
                </div>
                <hr/>
                <h4>{"Delete List"}</h4>
                <div class="d-flex gap-3">
                    <button type="button" class="btn btn-danger" onclick={delete}>{"Delete"}</button>
                    <button type="button" class="btn btn-danger" onclick={delete_all}>{"Delete All"}</button>
                </div>
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => false,
            Msg::AddSource => {
                self.sources
                    .push((self.counter, NodeRef::default(), NodeRef::default(), None));
                self.counter += 1;
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
                for (_, source, id, _) in &self.sources {
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
                        "List Items" => {
                            self.list.sources.push(Source {
                                source_type: SourceType::ListItems(id),
                                name: String::new(),
                            });
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
                    ctx.link().send_future_batch(async move {
                        crate::delete_list(&id).await.unwrap();
                        navigator.push(&Route::Home);
                        None
                    });
                }
                false
            }
            Msg::DeleteAll => {
                let id = self.list.id.clone();
                let items: Vec<_> = self.list.items.iter().map(|i| i.id.clone()).collect();
                if crate::window()
                    .confirm_with_message(&format!("Delete all items in {id} and list?"))
                    .unwrap()
                {
                    let navigator = ctx.link().navigator().unwrap();
                    ctx.link().send_future_batch(async move {
                        crate::delete_items(&items).await.unwrap();
                        crate::delete_list(&id).await.unwrap();
                        navigator.push(&Route::Home);
                        None
                    });
                }
                false
            }
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            if !matches!(self.list.mode, ListMode::External) {
                self.name_ref
                    .cast::<HtmlInputElement>()
                    .unwrap()
                    .set_value(&self.list.name);
            }
            if let ListMode::User(Some(external_id)) = &self.list.mode {
                self.external_ref
                    .cast::<HtmlInputElement>()
                    .unwrap()
                    .set_value(&external_id.raw_id);
            }
            self.query_ref
                .cast::<HtmlInputElement>()
                .unwrap()
                .set_value(&self.list.query);
            if self.list.favorite {
                self.favorite_ref
                    .cast::<HtmlInputElement>()
                    .unwrap()
                    .set_checked(true);
            }
            for (_, _, id, source) in self.sources.iter() {
                let value = match source {
                    None => String::new(),
                    Some(SourceType::Custom(value)) => value.to_string(),
                    Some(
                        SourceType::Spotify(Spotify::Playlist(Id { raw_id, .. }))
                        | SourceType::Spotify(Spotify::Album(Id { raw_id, .. }))
                        | SourceType::Spotify(Spotify::Track(Id { raw_id, .. })),
                    ) => raw_id.clone(),
                    Some(SourceType::Setlist(Id { raw_id, .. })) => raw_id.clone(),
                    Some(SourceType::ListItems(id)) => id.clone(),
                };
                id.cast::<HtmlInputElement>().unwrap().set_value(&value);
            }
        }
    }
}
