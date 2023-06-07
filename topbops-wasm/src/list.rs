use serde_json::Value;
use std::collections::HashMap;
use topbops::{ItemMetadata, ItemQuery, List, ListMode};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlInputElement, HtmlSelectElement, Request, RequestInit, RequestMode};
use yew::{html, Component, Context, Html, NodeRef, Properties};

enum ListState {
    Fetching,
    Success(Vec<(ItemMetadata, String, String, NodeRef, NodeRef)>),
}

pub enum Msg {
    None,
    Load(ItemQuery),
    Save,
    SaveSuccess(Vec<(usize, HashMap<String, Value>)>),
    Push,
}

#[derive(Eq, PartialEq, Properties)]
pub struct ListProps {
    pub list: List,
}

pub struct ListItems {
    state: ListState,
}

impl Component for ListItems {
    type Message = Msg;
    type Properties = ListProps;

    fn create(ctx: &Context<Self>) -> Self {
        let id = ctx.props().list.id.clone();
        ctx.link()
            .send_future(async move { Msg::Load(crate::query_items(&id).await.unwrap()) });
        ListItems {
            state: ListState::Fetching,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let disabled = crate::get_user().is_none();
        match &self.state {
            ListState::Fetching => html! {},
            ListState::Success(items) => {
                let list = &ctx.props().list;
                let source_html = list.sources.iter().map(|source| {
                    html! {
                        <div class="row mb-1">
                            <label class="col-9 col-sm-10 col-form-label">{&source.name}</label>
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
                let save = ctx.link().callback(|_| Msg::Save);
                let push = ctx.link().callback(|_| Msg::Push);
                html! {
                    <div>
                        if let Some(src) = list.iframe.clone() {
                            <div class="row">
                                <div class="col-12 col-xl-11">
                                    <iframe width="100%" height="380" frameborder="0" {src}></iframe>
                                </div>
                            </div>
                        }
                        <div class="row">
                            <p class="col-2 offset-9"><strong>{"Rating"}</strong></p>
                            <p class="col-1"><strong>{"Hidden"}</strong></p>
                        </div>
                        <form>
                            {for html}
                            <button type="button" class="btn btn-success" onclick={save} {disabled}>{"Save"}</button>
                        </form>
                        <hr/>
                        <h4>{"Data Sources"}</h4>
                        {for source_html}
                        if !matches!(list.mode, ListMode::External) {
                            <button type="button" class="btn btn-primary" onclick={push} {disabled}>{"Push"}</button>
                        }
                    </div>
                }
            }
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::None => false,
            Msg::Load(query) => {
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
                self.state = ListState::Success(items);
                true
            }
            Msg::Save => {
                let ListState::Success(items) = &mut self.state else { unreachable!() };
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
                let ListState::Success(items) = &mut self.state else { unreachable!() };
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
                let id = ctx.props().list.id.clone();
                ctx.link().send_future(async move {
                    crate::push_list(&id).await.unwrap();
                    Msg::None
                });
                false
            }
        }
    }
}
