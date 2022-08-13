use serde_json::Value;
use std::collections::HashMap;
use topbops::{ItemMetadata, ItemQuery, List};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlSelectElement, Request, RequestInit, RequestMode};
use yew::{html, Component, Context, Html, NodeRef, Properties};

enum EditState {
    Fetching,
    Success(List, Vec<(ItemMetadata, String, NodeRef)>),
}

pub enum Msg {
    Load(List, ItemQuery),
    Save(Vec<(ItemMetadata, String, NodeRef)>),
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
            EditState::Success(list, items) => {
                let html = items.iter().map(|(item, rating, select_ref)| {
                    html! {
                        <div class="row">
                            <label class="col-12 col-lg-8 col-xl-7 col-form-label">{&item.name}</label>
                            <div class="col-3 col-lg-2 col-xl-1">
                                <select ref={select_ref} class="form-select" {disabled}>
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
                        </div>
                    }
                });
                let items = items.clone();
                let save = ctx.link().callback(move |_| Msg::Save(items.clone()));
                html! {
                    <div>
                        <h1>{&list.name}</h1>
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
            Msg::Load(list, query) => {
                let items = query
                    .items
                    .into_iter()
                    .map(|mut i| {
                        (
                            i.metadata.unwrap(),
                            i.values.pop().unwrap(),
                            NodeRef::default(),
                        )
                    })
                    .collect();
                self.state = EditState::Success(list, items);
                true
            }
            Msg::Save(items) => {
                let updates: HashMap<_, HashMap<_, Value>> = items
                    .into_iter()
                    .filter_map(|(item, rating, select_ref)| {
                        let mut value = select_ref.cast::<HtmlSelectElement>().unwrap().value();
                        if value.is_empty() {
                            value = String::from("null");
                        }
                        if value == rating {
                            None
                        } else {
                            Some((
                                item.id,
                                [(
                                    String::from("rating"),
                                    serde_json::from_str(&value).unwrap(),
                                )]
                                .into_iter()
                                .collect(),
                            ))
                        }
                    })
                    .collect();
                if !updates.is_empty() {
                    let window = web_sys::window().expect("no global `window` exists");
                    let mut opts = RequestInit::new();
                    opts.method("POST");
                    opts.mode(RequestMode::Cors);
                    let updates = JsValue::from_str(&serde_json::to_string(&updates).unwrap());
                    opts.body(Some(&updates));
                    let request =
                        Request::new_with_str_and_init("/api/?action=updateItems", &opts).unwrap();
                    request
                        .headers()
                        .set("Content-Type", "application/json")
                        .unwrap();
                    ctx.link().send_future_batch(async move {
                        JsFuture::from(window.fetch_with_request(&request))
                            .await
                            .unwrap();
                        Vec::new()
                    });
                }
                false
            }
        }
    }
}
