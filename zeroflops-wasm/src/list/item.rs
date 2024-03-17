use crate::{
    bootstrap::{Alert, Modal},
    dataframe::DataFrame,
    ListsRoute,
};
use arrow::{array::AsArray, datatypes::UInt64Type};
use js_sys::Error;
use serde_json::Value;
use std::{collections::HashMap, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    HtmlInputElement, HtmlSelectElement, Request, RequestInit, RequestMode, Response, Url,
};
use yew::{html, Component, Context, Html, NodeRef, Properties};
use yew_router::prelude::Link;
use zeroflops::{Id, ItemMetadata, List, ListMode, SourceType, Spotify, User};

pub enum Msg {
    None,
    Load(Option<DataFrame>),
    Save,
    SaveError(String),
    HideAlert,
    SaveSuccess(Vec<(usize, HashMap<String, Value>)>),
    Push,
    Open(ItemMetadata),
    HideModal,
    SelectView,
    Delete((String, usize)),
    DeleteSuccess(usize),
}

#[derive(PartialEq, Properties)]
pub struct ListProps {
    pub user: Rc<Option<User>>,
    pub list: List,
}

pub struct ListItems {
    items: Vec<ListItem>,
    state: Option<Vec<State>>,
    select_ref: NodeRef,
    mode: ItemMode,
    alert: Option<Result<String, String>>,
    modal: Option<ItemMetadata>,
}

struct ListItem {
    item: ItemMetadata,
    rating_ref: NodeRef,
    hidden_ref: NodeRef,
}

#[derive(Clone, Default)]
struct State {
    rating: Option<u64>,
    hidden: bool,
}

enum ItemMode {
    View,
    Update,
    Delete,
}

impl Component for ListItems {
    type Message = Msg;
    type Properties = ListProps;

    fn create(ctx: &Context<Self>) -> Self {
        let list = ctx.props().list.clone();
        if !matches!(list.mode, ListMode::View(_)) {
            ctx.link().send_future(async move {
                Msg::Load(
                    crate::query_list(
                        &list,
                        Some("SELECT id, rating, hidden FROM item".to_owned()),
                    )
                    .await
                    .unwrap(),
                )
            });
        }
        ListItems {
            items: ctx
                .props()
                .list
                .items
                .iter()
                .map(|i| ListItem {
                    item: i.clone(),
                    rating_ref: NodeRef::default(),
                    hidden_ref: NodeRef::default(),
                })
                .collect(),
            state: None,
            select_ref: NodeRef::default(),
            mode: if let ListMode::View(_) = ctx.props().list.mode {
                ItemMode::View
            } else {
                ItemMode::Update
            },
            alert: None,
            modal: None,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let disabled = ctx.props().user.is_none();
        let list = &ctx.props().list;
        let source_html = list.sources.iter().map(|source| {
            let raw_id = match &source.source_type {
                SourceType::Spotify(Spotify::Playlist(Id { raw_id, .. }))
                | SourceType::Spotify(Spotify::Album(Id { raw_id, .. }))
                | SourceType::Setlist(Id { raw_id, .. })
                    if Url::new(raw_id).is_ok() =>
                {
                    Some(raw_id.clone())
                }
                _ => None,
            };
            html! {
                if let SourceType::ListItems(id) = &source.source_type {
                    <div class="mb-2"><Link<ListsRoute> to={ListsRoute::View { id: id.clone() }}>{&source.name}</Link<ListsRoute>></div>
                } else if let Some(href) = raw_id {
                    <div class="mb-2"><a {href}>{&source.name}</a></div>
                } else {
                    <p class="mb-2">{&source.name}</p>
                }
            }
        });
        let html: Html = match self.mode {
            ItemMode::View => self
                .items
                .iter()
                .map(|ListItem { item, .. }| {
                    let open = {
                        let item = item.clone();
                        ctx.link().callback(move |_| Msg::Open(item.clone()))
                    };
                    html! {
                        <div class="row mb-1">
                            <label class="col col-form-label"><a href="#" onclick={open}>{&item.name}</a></label>
                        </div>
                    }
                })
                .collect(),
            ItemMode::Update => self
                .items
                .iter()
                .enumerate()
                .map(
                    |(i, ListItem {
                        item,
                        rating_ref,
                        hidden_ref,
                    })| {
                        let open = {
                            let item = item.clone();
                            ctx.link().callback(move |_| Msg::Open(item.clone()))
                        };
                        html! {
                            <div class="row mb-1">
                                <label class="col-9 col-form-label"><a href="#" onclick={open}>{&item.name}</a></label>
                                if let Some(State { rating, hidden }) = self.state.as_ref().and_then(|s| s.get(i)) {
                                    <div class="col-2">
                                        <select ref={rating_ref} class="form-select" {disabled}>
                                            <option selected={rating.is_none()}></option>
                                            <option selected={*rating == Some(0)}>{"0"}</option>
                                            <option selected={*rating == Some(1)}>{"1"}</option>
                                            <option selected={*rating == Some(2)}>{"2"}</option>
                                            <option selected={*rating == Some(3)}>{"3"}</option>
                                            <option selected={*rating == Some(4)}>{"4"}</option>
                                            <option selected={*rating == Some(5)}>{"5"}</option>
                                            <option selected={*rating == Some(6)}>{"6"}</option>
                                            <option selected={*rating == Some(7)}>{"7"}</option>
                                            <option selected={*rating == Some(8)}>{"8"}</option>
                                            <option selected={*rating == Some(9)}>{"9"}</option>
                                            <option selected={*rating == Some(10)}>{"10"}</option>
                                        </select>
                                    </div>
                                    <div class="col-1 d-flex justify-content-center">
                                        <input ref={hidden_ref} class="form-check-input mt-2" type="checkbox" checked={*hidden}/>
                                    </div>
                                }
                            </div>
                        }
                    },
                )
                .collect(),
            ItemMode::Delete => self
                .items
                .iter()
                .enumerate()
                .map(|(i, ListItem { item, .. })| {
                    let open = {
                        let item = item.clone();
                        ctx.link().callback(move |_| Msg::Open(item.clone()))
                    };
                    let delete = {
                        let id = item.id.clone();
                        ctx.link().callback(move |_| Msg::Delete((id.clone(), i)))
                    };
                    html! {
                        <div class="row mb-1">
                            <label class="col col-form-label"><a href="#" onclick={open}>{&item.name}</a></label>
                            <div class="col-auto">
                                <button type="button" class="btn btn-danger" onclick={delete} {disabled}>{"Delete"}</button>
                            </div>
                        </div>
                    }
                })
                .collect(),
        };
        let save = ctx.link().callback(|_| Msg::Save);
        let push = ctx.link().callback(|_| Msg::Push);
        let push_available = if let Some(user) = &*ctx.props().user {
            if let Ok((Some(source), _)) = list.get_unique_source() {
                source == "spotify" && user.spotify_user.is_some()
            } else {
                false
            }
        } else {
            false
        };
        let hide = ctx.link().callback(|_| Msg::HideAlert);
        html! {
            <div>
                if let Some(item) = &self.modal {
                    <Modal header={item.name.clone()} hide={ctx.link().callback(|_| Msg::HideModal)}>
                        if let Some(iframe) = &item.iframe {
                            <iframe width="100%" height="380" frameborder="0" src={iframe.clone()}></iframe>
                        }
                    </Modal>
                }
                if matches!(ctx.props().list.mode, ListMode::View(_)) {
                    <div class="row mb-3">
                        <label class="col-auto col-form-label">
                            <strong>{"Item Mode:"}</strong>
                        </label>
                        <div class="col-auto">
                            <select ref={self.select_ref.clone()} class="form-select" onchange={ctx.link().callback(|_| Msg::SelectView)}>
                                <option selected=true>{"Update"}</option>
                                <option>{"Delete"}</option>
                            </select>
                        </div>
                    </div>
                }
                if let Some(src) = list.iframe.clone() {
                    <div class="row">
                        <div class="col-12 col-xl-11">
                            <iframe width="100%" height="380" frameborder="0" {src}></iframe>
                        </div>
                    </div>
                }
                if let ItemMode::Update = self.mode {
                    <div class="row">
                        <p class="col-2 offset-9"><strong>{"Rating"}</strong></p>
                        <p class="col-1"><strong>{"Hidden"}</strong></p>
                    </div>
                }
                <form>
                    <div class="overflow-y-auto mb-3" style="max-height: 800px">
                        {html}
                    </div>
                    if let Some(result) = self.alert.clone() {
                        <button type="button" class="btn btn-success mb-3" onclick={save} {disabled}>{"Save"}</button>
                        <Alert {result} {hide}/>
                    } else {
                        <button type="button" class="btn btn-success" onclick={save} {disabled}>{"Save"}</button>
                    }
                </form>
                <hr/>
                <h4>{"Data Sources"}</h4>
                {for source_html}
                if !matches!(list.mode, ListMode::External) {
                    <button type="button" class="btn btn-success" onclick={push} disabled={!push_available}>{"Push"}</button>
                }
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => false,
            Msg::Load(query) => {
                let Some(query) = query else {
                    return false;
                };
                let index: HashMap<_, _> = self
                    .items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (item.item.id.as_str().to_owned(), i))
                    .collect();
                let ids = query.column("id").unwrap().as_string::<i64>();
                // NullArray does not cast correctly
                let ratings = if let Some(ratings) = query
                    .column("rating")
                    .unwrap()
                    .as_primitive_opt::<UInt64Type>()
                {
                    ratings.into_iter().collect()
                } else {
                    vec![None; query.column("rating").unwrap().len()]
                };
                let hidden = query.column("hidden").unwrap().as_boolean();
                let mut state = vec![State::default(); self.items.len()];
                for ((id, &rating), hidden) in ids.iter().zip(ratings.iter()).zip(hidden.iter()) {
                    state[index[id.unwrap()]] = State {
                        rating,
                        hidden: hidden.unwrap(),
                    };
                }
                self.state = Some(state);
                true
            }
            Msg::Save => {
                let mut update_ids = HashMap::new();
                let mut update_indexes = Vec::new();
                for (
                    i,
                    (
                        ListItem {
                            item,
                            rating_ref,
                            hidden_ref,
                        },
                        rating_hidden,
                    ),
                ) in self
                    .items
                    .iter()
                    .zip(self.state.as_ref().unwrap().iter())
                    .enumerate()
                {
                    let State { rating, hidden } = rating_hidden;
                    let mut updates = HashMap::new();
                    let value = rating_ref
                        .cast::<HtmlSelectElement>()
                        .unwrap()
                        .value()
                        .parse::<u64>()
                        .ok();
                    if value != *rating {
                        updates.insert(String::from("rating"), value.into());
                    }
                    let value =
                        Value::Bool(hidden_ref.cast::<HtmlInputElement>().unwrap().checked());
                    #[allow(clippy::cmp_owned)]
                    if value != *hidden {
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
                        match JsFuture::from(window.fetch_with_request(&request)).await {
                            Ok(resp) => {
                                let resp_value: Response = resp.dyn_into().unwrap();
                                if resp_value.status() >= 400 {
                                    Msg::SaveError(
                                        JsFuture::from(resp_value.text().unwrap())
                                            .await
                                            .unwrap()
                                            .as_string()
                                            .unwrap(),
                                    )
                                } else {
                                    Msg::SaveSuccess(update_indexes)
                                }
                            }
                            Err(e) => {
                                Msg::SaveError(e.dyn_into::<Error>().unwrap().to_string().into())
                            }
                        }
                    });
                }
                false
            }
            Msg::SaveError(e) => {
                self.alert = Some(Err(e));
                true
            }
            Msg::HideAlert => {
                self.alert = None;
                true
            }
            // Update the rating and hidden state values if the save request is successful.
            // We check if the values are the same to avoid no-op requests.
            Msg::SaveSuccess(updates) => {
                for (i, update) in updates {
                    for (k, v) in update {
                        let State { rating, hidden } =
                            self.state.as_mut().unwrap().get_mut(i).unwrap();
                        match k.as_str() {
                            "rating" => {
                                *rating = v.as_u64();
                            }
                            "hidden" => {
                                *hidden = v.as_bool().unwrap();
                            }
                            _ => unimplemented!(),
                        }
                    }
                }
                self.alert = Some(Ok("Save successful".to_owned()));
                true
            }
            Msg::Push => {
                let id = ctx.props().list.id.clone();
                ctx.link().send_future(async move {
                    crate::push_list(&id).await.unwrap();
                    Msg::None
                });
                false
            }
            Msg::Open(item) => {
                self.modal = Some(item);
                true
            }
            Msg::HideModal => {
                self.modal = None;
                true
            }
            Msg::SelectView => {
                self.mode = match self
                    .select_ref
                    .cast::<HtmlSelectElement>()
                    .map(|s| s.value())
                    .as_deref()
                    .unwrap_or("Update")
                {
                    "Update" => ItemMode::Update,
                    "Delete" => ItemMode::Delete,
                    _ => unreachable!(),
                };
                true
            }
            Msg::Delete((id, i)) => {
                ctx.link().send_future(async move {
                    crate::delete_items(&[id]).await.unwrap();
                    Msg::DeleteSuccess(i)
                });
                false
            }
            Msg::DeleteSuccess(i) => {
                self.items.remove(i);
                if let Some(state) = &mut self.state {
                    state.remove(i);
                }
                true
            }
        }
    }
}
