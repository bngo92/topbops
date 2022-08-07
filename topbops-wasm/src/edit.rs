use std::collections::HashMap;
use topbops::{ItemQuery, List};
use yew::{html, Component, Context, Html, Properties};

enum EditState {
    Fetching,
    Success(List, ItemQuery),
}

pub enum Msg {
    Load(List, ItemQuery),
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
            let user = "demo";
            let (list, query) =
                futures::future::join(crate::fetch_list(user, &id), crate::query_items(user, &id))
                    .await;
            Msg::Load(list.unwrap(), query.unwrap())
        });
        Edit {
            state: EditState::Fetching,
        }
    }

    fn view(&self, _: &Context<Self>) -> Html {
        match &self.state {
            EditState::Fetching => html! {},
            EditState::Success(list, query) => {
                let mut map = HashMap::new();
                for row in &query.items {
                    map.insert(
                        &row.metadata.as_ref().unwrap().id,
                        row.values.last().unwrap(),
                    );
                }
                let html = list.items.iter().map(|item| {
                    let i = map[&item.id];
                    html! {
                        <div class="row">
                            <label class="col-12 col-lg-8 col-xl-7 col-form-label">{&item.name}</label>
                            <div class="col-3 col-lg-2 col-xl-1">
                                <select class="form-select" disabled=true>
                                    <option selected={i == "null"}></option>
                                    <option selected={i == "0"}>{"0"}</option>
                                    <option selected={i == "1"}>{"1"}</option>
                                    <option selected={i == "2"}>{"2"}</option>
                                    <option selected={i == "3"}>{"3"}</option>
                                    <option selected={i == "4"}>{"4"}</option>
                                    <option selected={i == "5"}>{"5"}</option>
                                    <option selected={i == "6"}>{"6"}</option>
                                    <option selected={i == "7"}>{"7"}</option>
                                    <option selected={i == "8"}>{"8"}</option>
                                    <option selected={i == "9"}>{"9"}</option>
                                    <option selected={i == "10"}>{"10"}</option>
                                </select>
                            </div>
                        </div>
                    }
                });
                html! {
                    <div>
                        <h1>{&list.name}</h1>
                        {for html}
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

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        let Msg::Load(list, query) = msg;
        self.state = EditState::Success(list, query);
        true
    }
}
