use serde_json::Value;
use std::collections::HashMap;
use topbops::{ItemMetadata, ItemQuery, List, ListMode, Source, SourceType, Spotify};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlInputElement, HtmlSelectElement, Request, RequestInit, RequestMode};
use yew::{html, Component, Context, Html, NodeRef, Properties};

enum EditState {
    Fetching,
    Success(
        List,
        Vec<(NodeRef, NodeRef)>,
        Vec<(ItemMetadata, String, String, NodeRef, NodeRef)>,
        NodeRef,
        NodeRef,
        NodeRef,
    ),
}

pub enum Msg {
    None,
    Load(List, ItemQuery),
    AddSource,
    DeleteSource(usize),
    DeleteNewSource(usize),
    Save,
    SaveSuccess(Vec<(usize, HashMap<String, Value>)>),
    Push,
}

#[derive(Eq, PartialEq, Properties)]
pub struct EditProps {
    pub id: String,
}

pub struct Edit {
    state: EditState,
}

impl Component for Edit {
    type Message = Msg;
    type Properties = EditProps;

    fn create(ctx: &Context<Self>) -> Self {
        let id = ctx.props().id.clone();
        ctx.link().send_future(async move {
            let (list, query) =
                futures::future::join(crate::fetch_list(&id), crate::query_items(&id)).await;
            Msg::Load(list.unwrap(), query.unwrap())
        });
        Edit {
            state: EditState::Fetching,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let disabled = crate::get_user().is_none();
        match &self.state {
            EditState::Fetching => html! {},
            EditState::Success(list, new_sources, items, name_ref, external_ref, favorite_ref) => {
                let source_html = list.sources.iter().enumerate().map(|(i, source)| {
                    let onclick = ctx.link().callback(move |_| Msg::DeleteSource(i));
                    html! {
                        <div class="row mb-1">
                            <label class="col-9 col-sm-10 col-form-label">{&source.name}</label>
                            if !matches!(list.mode, ListMode::External) {
                                <div class="col-2">
                                    <button type="button" class="btn btn-danger" {onclick}>{"Delete"}</button>
                                </div>
                            }
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
                                </select>
                            </div>
                            <input class="col-9 col-sm-7 col-md-8 col-form-label" ref={id}/>
                            <div class="col-2">
                                <button type="button" class="btn btn-danger" {onclick}>{"Delete"}</button>
                            </div>
                        </div>
                    }
                });
                let html = items.iter().map(|(item, rating, hidden, rating_ref, hidden_ref)| {
                    let checked = hidden == "true";
                    html! {
                        <div class="row mb-1">
                            <label class="col-9 col-form-label">{&item.name}</label>
                            <div class="col-2">
                                <select ref={rating_ref} class="form-select" {disabled}>
                                    <option selected={rating == "null"}></option>
                                    <option selected={rating == "0"}>{"0"}</option>
                                    <option selected={rating == "1"}>{"1"}</option>
                                    <option selected={rating == "2"}>{"2"}</option>
                                    <option selected={rating == "3"}>{"3"}</option>
                                    <option selected={rating == "4"}>{"4"}</option>
                                    <option selected={rating == "5"}>{"5"}</option>
                                    <option selected={rating == "6"}>{"6"}</option>
                                    <option selected={rating == "7"}>{"7"}</option>
                                    <option selected={rating == "8"}>{"8"}</option>
                                    <option selected={rating == "9"}>{"9"}</option>
                                    <option selected={rating == "10"}>{"10"}</option>
                                </select>
                            </div>
                            <div class="col-1 d-flex justify-content-center">
                                <input ref={hidden_ref} class="form-check-input mt-2" type="checkbox" {checked}/>
                            </div>
                        </div>
                    }
                });
                let checked = list.favorite;
                let add_source = ctx.link().callback(|_| Msg::AddSource);
                let save = ctx.link().callback(|_| Msg::Save);
                let push = ctx.link().callback(|_| Msg::Push);
                let content = html! {
                    <div class="col-lg-10 col-xl-8">
                        if matches!(list.mode, ListMode::External) {
                            <h1>{&list.name}</h1>
                        }
                        <h4>{"List Settings"}</h4>
                        <form>
                            if !matches!(list.mode, ListMode::External) {
                                <div class="form-floating mb-3 col-md-6">
                                    <input class="form-control" id="name" value={Some(list.name.clone())} ref={name_ref.clone()} placeholder="Name"/>
                                    <label for="name">{"List name"}</label>
                                </div>
                                if let ListMode::User(external_id) = &list.mode {
                                    <div class="form-floating mb-3 col-md-6">
                                        <input class="form-control" id="externalId" value={external_id.clone()} ref={external_ref.clone()} placeholder="External ID"/>
                                        <label for="externalId">{"External ID"}</label>
                                    </div>
                                }
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
                        // TODO: Add edit mode and toggle
                        if !matches!(list.mode, ListMode::External) {
                            <button type="button" class="btn btn-primary" onclick={add_source}>{"Add source"}</button>
                            <button type="button" class="btn btn-secondary" onclick={push} {disabled}>{"Push"}</button>
                        }
                        <hr/>
                        <div class="row">
                            <h4 class="col-9">{"Items"}</h4>
                            <p class="col-2"><strong>{"Rating"}</strong></p>
                            <p class="col-1"><strong>{"Hidden"}</strong></p>
                        </div>
                        <form>
                            {for html}
                            <div class="col-12 mb-3">
                                <button type="button" class="btn btn-success" onclick={save} {disabled}>{"Save"}</button>
                            </div>
                        </form>
                        if let Some(src) = list.iframe.clone() {
                            <div class="row">
                                <div class="col-12 col-lg-10">
                                    <iframe width="100%" height="380" frameborder="0" {src}></iframe>
                                </div>
                            </div>
                        }
                        </div>
                };
                html! {
                    <div class="row">
                        {content}
                    </div>
                }
            }
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => false,
            Msg::Load(list, query) => {
                let items = query
                    .items
                    .into_iter()
                    .map(|mut i| {
                        (
                            i.metadata.unwrap(),
                            i.values.pop().unwrap(),
                            i.values.pop().unwrap(),
                            NodeRef::default(),
                            NodeRef::default(),
                        )
                    })
                    .collect();
                self.state = EditState::Success(
                    list,
                    Vec::new(),
                    items,
                    NodeRef::default(),
                    NodeRef::default(),
                    NodeRef::default(),
                );
                true
            }
            Msg::AddSource => {
                let EditState::Success(_, new_sources, _, _, _, _) = &mut self.state else { unreachable!() };
                new_sources.push((NodeRef::default(), NodeRef::default()));
                true
            }
            Msg::DeleteSource(i) => {
                let EditState::Success(list, _, _, _, _, _) = &mut self.state else { unreachable!() };
                list.sources.remove(i);
                true
            }
            Msg::DeleteNewSource(i) => {
                let EditState::Success(_, new_sources, _, _, _, _) = &mut self.state else { unreachable!() };
                new_sources.remove(i);
                true
            }
            Msg::Save => {
                let EditState::Success(list, new_refs, items, name_ref, external_ref, favorite_ref) = &mut self.state else { unreachable!() };
                if !matches!(list.mode, ListMode::External) {
                    list.name = name_ref.cast::<HtmlInputElement>().unwrap().value();
                }
                if let ListMode::User(external_id) = &mut list.mode {
                    let id = external_ref.cast::<HtmlInputElement>().unwrap().value();
                    if id.is_empty() {
                        *external_id = None;
                    } else if let Some(Spotify::Playlist(id)) = crate::parse_spotify_source(&id) {
                        *external_id = Some(id);
                    }
                }
                list.favorite = favorite_ref.cast::<HtmlInputElement>().unwrap().checked();
                for (source, id) in new_refs {
                    let source = source.cast::<HtmlSelectElement>().unwrap().value();
                    let id = id.cast::<HtmlInputElement>().unwrap().value();
                    match &*source {
                        "Spotify" => {
                            if let Some(source) = crate::parse_spotify_source(&id) {
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
                let mut update_ids = HashMap::new();
                let mut update_indexes = Vec::new();
                for (i, (item, rating, hidden, rating_ref, hidden_ref)) in items.iter().enumerate()
                {
                    let mut updates = HashMap::new();
                    let mut value = rating_ref.cast::<HtmlSelectElement>().unwrap().value();
                    if value.is_empty() {
                        value = String::from("null");
                    }
                    if value != *rating {
                        updates.insert(
                            String::from("rating"),
                            serde_json::from_str(&value).unwrap(),
                        );
                    }
                    let value =
                        Value::Bool(hidden_ref.cast::<HtmlInputElement>().unwrap().checked());
                    #[allow(clippy::cmp_owned)]
                    if value.to_string() != *hidden {
                        updates.insert(String::from("hidden"), value);
                    }
                    if !updates.is_empty() {
                        update_ids.insert(item.id.clone(), updates.clone());
                        update_indexes.push((i, updates));
                    }
                }
                if !update_ids.is_empty() {
                    let window = web_sys::window().expect("no global `window` exists");
                    let mut opts = RequestInit::new();
                    opts.method("POST");
                    opts.mode(RequestMode::Cors);
                    let updates = JsValue::from_str(&serde_json::to_string(&update_ids).unwrap());
                    opts.body(Some(&updates));
                    let request =
                        Request::new_with_str_and_init("/api/?action=updateItems", &opts).unwrap();
                    request
                        .headers()
                        .set("Content-Type", "application/json")
                        .unwrap();
                    ctx.link().send_future(async move {
                        JsFuture::from(window.fetch_with_request(&request))
                            .await
                            .unwrap();
                        Msg::SaveSuccess(update_indexes)
                    });
                }
                false
            }
            // Update the rating and hidden state values if the save request is successful.
            // We check if the values are the same to avoid no-op requests.
            Msg::SaveSuccess(updates) => {
                let EditState::Success(_, _, items, _,_ , _) = &mut self.state else { unreachable!() };
                for (i, update) in updates {
                    for (k, v) in update {
                        let (_item, rating, hidden, _rating_ref, _hidden_ref) = &mut items[i];
                        let v = v.to_string();
                        match k.as_str() {
                            "rating" => {
                                *rating = v;
                            }
                            "hidden" => {
                                *hidden = v;
                            }
                            _ => unimplemented!(),
                        }
                    }
                }
                false
            }
            Msg::Push => {
                let EditState::Success(list, _,_ , _, _, _) = &mut self.state else { unreachable!() };
                let id = list.id.clone();
                ctx.link().send_future(async move {
                    crate::push_list(&id).await.unwrap();
                    Msg::None
                });
                false
            }
        }
    }
}
