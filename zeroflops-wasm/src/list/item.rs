use crate::{
    bootstrap::{Alert, Modal},
    ListsRoute,
};
use js_sys::Error;
use polars::prelude::{col, df, DataFrame, IntoLazy, NamedFrom, TakeRandom};
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
    Load(DataFrame),
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
    state: Vec<ListItem>,
    select_ref: NodeRef,
    mode: ItemMode,
    alert: Option<Result<String, String>>,
    modal: Option<ItemMetadata>,
}

struct ListItem {
    item: ItemMetadata,
    rating_hidden: Option<(Option<u64>, bool)>,
    rating_ref: NodeRef,
    hidden_ref: NodeRef,
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
            ctx.link()
                .send_future(async move { Msg::Load(crate::get_items(&list).await.unwrap()) });
        }
        ListItems {
            state: ctx
                .props()
                .list
                .items
                .iter()
                .map(|i| ListItem {
                    item: i.clone(),
                    rating_hidden: None,
                    rating_ref: NodeRef::default(),
                    hidden_ref: NodeRef::default(),
                })
                .collect(),
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
                .state
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
                .state
                .iter()
                .map(
                    |ListItem {
                         item,
                         rating_hidden,
                         rating_ref,
                         hidden_ref,
                     }| {
                        let open = {
                            let item = item.clone();
                            ctx.link().callback(move |_| Msg::Open(item.clone()))
                        };
                        html! {
                            <div class="row mb-1">
                                <label class="col-9 col-form-label"><a href="#" onclick={open}>{&item.name}</a></label>
                                if let Some((rating, hidden)) = rating_hidden {
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
                .state
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
                let ids = self
                    .state
                    .iter()
                    .map(|i| i.item.id.as_str())
                    .collect::<Vec<_>>();
                let df = query
                    .lazy()
                    .inner_join(df!("id" => ids).unwrap().lazy(), col("id"), col("id"))
                    .collect()
                    .unwrap();
                // polars requires that at least one row is not null
                let ratings = df.column("rating").and_then(|s| s.u64());
                let hidden = df["hidden"].bool().unwrap();
                if let Ok(ratings) = ratings {
                    for i in 0..df.height() {
                        self.state[i].rating_hidden =
                            Some((ratings.get(i), hidden.get(i).unwrap()));
                    }
                } else {
                    for i in 0..df.height() {
                        self.state[i].rating_hidden = Some((None, hidden.get(i).unwrap()));
                    }
                }
                true
            }
            Msg::Save => {
                let mut update_ids = HashMap::new();
                let mut update_indexes = Vec::new();
                for (
                    i,
                    ListItem {
                        item,
                        rating_hidden,
                        rating_ref,
                        hidden_ref,
                    },
                ) in self.state.iter().enumerate()
                {
                    let (rating, hidden) = rating_hidden.as_ref().unwrap();
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
                        let (rating, hidden) = self
                            .state
                            .get_mut(i)
                            .unwrap()
                            .rating_hidden
                            .as_mut()
                            .unwrap();
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
                self.state.remove(i);
                true
            }
        }
    }
}
