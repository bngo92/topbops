use serde_json::Value;
use std::collections::HashMap;
use topbops::{ItemMetadata, ItemQuery, List, ListMode};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlInputElement, HtmlSelectElement, Request, RequestInit, RequestMode};
use yew::{html, Component, Context, Html, NodeRef, Properties};

enum EditState {
    Fetching,
    Success(
        List,
        Vec<(ItemMetadata, String, String, NodeRef, NodeRef)>,
        NodeRef,
        NodeRef,
    ),
}

pub enum Msg {
    None,
    Load(List, ItemQuery),
    Save,
    SaveSuccess(Vec<(usize, HashMap<String, Value>)>),
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
            EditState::Success(list, items, name_ref, favorite_ref) => {
                let html = items.iter().map(|(item, rating, hidden, rating_ref, hidden_ref)| {
                    let checked = hidden == "true";
                    html! {
                        <div class="row mb-1">
                            <label class="col-12 col-lg-8 col-xl-7 col-form-label">{&item.name}</label>
                            <div class="col-3 col-lg-2 col-xl-1">
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
                let save = ctx.link().callback(|_| Msg::Save);
                html! {
                    <div>
                        if matches!(list.mode, ListMode::External) {
                            <h1>{&list.name}</h1>
                        }
                        <form>
                            if !matches!(list.mode, ListMode::External) {
                                <div class="row mb-3">
                                    <div class="col-md-6">
                                        <input class="form-control" value={Some(list.name.clone())} ref={name_ref.clone()} placeholder="Name"/>
                                    </div>
                                </div>
                            }
                            <div class="row">
                                <label class="col-sm-1">{"Favorite"}</label>
                                <div class="col-sm-5">
                                    <div class="form-check">
                                        <input ref={favorite_ref} class="form-check-input" type="checkbox" {checked}/>
                                    </div>
                                </div>
                            </div>
                        </form>
                        <div class="row">
                            <p class="col-lg-8 col-xl-7"></p>
                            <p class="col-3 col-lg-2 col-xl-1"><strong>{"Rating"}</strong></p>
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
                                <div class="col-12 col-lg-10 col-xl-8">
                                    <iframe width="100%" height="380" frameborder="0" {src}></iframe>
                                </div>
                            </div>
                        }
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
                self.state =
                    EditState::Success(list, items, NodeRef::default(), NodeRef::default());
                true
            }
            Msg::Save => {
                let EditState::Success(list, items, name_ref, favorite_ref) = &mut self.state else { unreachable!() };
                if !matches!(list.mode, ListMode::External) {
                    list.name = name_ref.cast::<HtmlInputElement>().unwrap().value();
                }
                list.favorite = favorite_ref.cast::<HtmlInputElement>().unwrap().checked();
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
                let EditState::Success(_, items, _, _) = &mut self.state else { unreachable!() };
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
        }
    }
}
