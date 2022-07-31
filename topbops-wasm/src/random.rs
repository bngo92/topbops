use rand::prelude::SliceRandom;
use std::collections::HashMap;
use topbops::{ItemMetadata, ItemQuery};
use yew::{html, Callback, Component, Context, Html, MouseEvent, Properties};
use yew_router::history::Location;
use yew_router::scope_ext::RouterScopeExt;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Match,
    Round,
}

#[derive(Clone)]
struct MatchData {
    left: ItemMetadata,
    right: ItemMetadata,
    query: ItemQuery,
}

pub enum Msg {
    LoadRandom(ItemQuery),
    UpdateStats((String, String, String, String)),
}

#[derive(Clone, Eq, PartialEq, Properties)]
pub struct MatchProps {
    pub id: String,
}

pub struct Match {
    mode: Mode,
    random_queue: Vec<topbops::Item>,
    data: Option<MatchData>,
}

impl Component for Match {
    type Message = Msg;
    type Properties = MatchProps;

    fn create(ctx: &Context<Self>) -> Self {
        let query = ctx
            .link()
            .location()
            .unwrap()
            .query::<HashMap<String, String>>()
            .unwrap_or_default();
        let mode = match query.get("mode").map_or("", String::as_str) {
            "rounds" => Mode::Round,
            _ => Mode::Match,
        };
        let id = ctx.props().id.clone();
        ctx.link().send_future(async move {
            let user = String::from("demo");
            let query = crate::query_items(&user, &id).await.unwrap();
            Msg::LoadRandom(query)
        });
        Match {
            mode,
            random_queue: Vec::new(),
            data: None,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let mode = match self.mode {
            Mode::Match => String::from("Random Matches"),
            Mode::Round => String::from("Random Rounds"),
        };
        let Some(MatchData{left, right, query}) = self.data.clone() else { return html! {}; };
        let user = String::from("demo");
        let list = &ctx.props().id;
        let left_param = (
            user.clone(),
            list.clone(),
            left.id.clone(),
            right.id.clone(),
        );
        let right_param = (user, list.clone(), right.id.clone(), left.id.clone());
        html! {
            <Random mode={mode} left={left} on_left_select={ctx.link().callback_once(move |_| Msg::UpdateStats(left_param))} right={right} on_right_select={ctx.link().callback_once(move |_| Msg::UpdateStats(right_param))} query={query}/>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadRandom(query) => {
                let (left, right) = match self.mode {
                    Mode::Round => {
                        match self.random_queue.len() {
                            // Reload the queue if it's empty
                            0 => {
                                let mut items = query.items.clone();
                                items.shuffle(&mut rand::thread_rng());
                                self.random_queue.extend(items);
                            }
                            // Always queue the last song next before reloading
                            1 => {
                                let last = self.random_queue.pop().unwrap();
                                let mut items = query.items.clone();
                                items.shuffle(&mut rand::thread_rng());
                                self.random_queue.extend(items);
                                self.random_queue.push(last);
                            }
                            _ => {}
                        }
                        (
                            self.random_queue.pop().unwrap().metadata.unwrap(),
                            self.random_queue.pop().unwrap().metadata.unwrap(),
                        )
                    }
                    Mode::Match => {
                        let mut queued_scores: Vec<_> = query.items.iter().collect();
                        queued_scores.shuffle(&mut rand::thread_rng());
                        (
                            queued_scores.pop().unwrap().metadata.clone().unwrap(),
                            queued_scores.pop().unwrap().metadata.clone().unwrap(),
                        )
                    }
                };
                self.data = Some(MatchData { left, right, query });
                true
            }
            Msg::UpdateStats((user, list, win, lose)) => {
                ctx.link().send_future(async move {
                    crate::update_stats(&user, &list, &win, &lose)
                        .await
                        .unwrap();
                    let query = crate::query_items(&user, &list).await.unwrap();
                    Msg::LoadRandom(query)
                });
                false
            }
        }
    }
}

enum RandomMsg {
    Left,
    Right,
}

#[derive(PartialEq, Properties)]
struct RandomProps {
    mode: String,
    left: ItemMetadata,
    on_left_select: Callback<MouseEvent>,
    right: ItemMetadata,
    on_right_select: Callback<MouseEvent>,
    query: ItemQuery,
}

struct Random {
    flag: RandomMsg,
}

impl Component for Random {
    type Message = RandomMsg;
    type Properties = RandomProps;

    fn create(_: &Context<Self>) -> Self {
        Random {
            flag: RandomMsg::Left,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let RandomProps {
            mode,
            left,
            right,
            query,
            on_left_select,
            on_right_select,
        } = ctx.props();
        let (left_class, right_class, src) = match self.flag {
            RandomMsg::Left => ("nav-link active", "nav-link", left.iframe.clone()),
            RandomMsg::Right => ("nav-link", "nav-link active", right.iframe.clone()),
        };
        let items: Vec<_> = query
            .items
            .iter()
            .map(|item| item.metadata.clone().unwrap())
            .collect();
        let (left_items, right_items): (Vec<_>, Vec<_>) = items
            .iter()
            .zip(1..)
            .map(|(item, i)| (i, html! {<Item i={i} item={item.clone()}/>}))
            .partition(|(i, _)| i % 2 == 1);
        let items = items
            .into_iter()
            .zip(1..)
            .map(|(item, i)| html! {<Item i={i} item={item}/>});
        let left_items = left_items.into_iter().map(|(_, item)| item);
        let right_items = right_items.into_iter().map(|(_, item)| item);
        html! {
          <div>
            <h1>{mode}</h1>
            <div class="row">
              <div class="col-12 d-lg-none">
                <ul class="nav nav-tabs nav-justified">
                  <li class="nav-item">
                    <a class={left_class} aria-label="Show left item" href="# " onclick={ctx.link().callback(|_| RandomMsg::Left)}>{&left.name}</a>
                  </li>
                  <li class="nav-item">
                    <a class={right_class} href="# " onclick={ctx.link().callback(|_| RandomMsg::Right)}>{&right.name}</a>
                  </li>
                </ul>
                <iframe width="100%" height="380" frameborder="0" {src}></iframe>
              </div>
              <div class="col-md-6 d-none d-lg-block">
                <iframe id="iframe1" width="100%" height="380" frameborder="0" src={left.iframe.clone()}></iframe>
              </div>
              <div class="col-md-6 d-none d-lg-block">
                <iframe id="iframe2" width="100%" height="380" frameborder="0" src={right.iframe.clone()}></iframe>
              </div>
              <div class="col-6">
                <button type="button" class="btn btn-info w-100" onclick={on_left_select.clone()}>{&left.name}</button>
              </div>
              <div class="col-6">
                <button type="button" class="btn btn-warning w-100" onclick={on_right_select.clone()}>{&right.name}</button>
              </div>
            </div>
            <div class="row">
              <div class="col-md-6 d-none d-lg-block">
                <table class="table table-striped">
                  <thead>
                    <tr>
                      <th class="col-1">{"#"}</th>
                      <th class="col-8">{"Track"}</th>
                      <th>{"Record"}</th>
                      <th>{"Score"}</th>
                    </tr>
                  </thead>
                  <tbody>{for left_items}</tbody>
                </table>
              </div>
              <div class="col-md-6 d-none d-lg-block">
                <table class="table table-striped">
                  <thead>
                    <tr>
                      <th class="col-1">{"#"}</th>
                      <th class="col-8">{"Track"}</th>
                      <th>{"Record"}</th>
                      <th>{"Score"}</th>
                    </tr>
                  </thead>
                  <tbody>{for right_items}</tbody>
                </table>
              </div>
              <div class="col-12 d-lg-none">
                <table class="table table-striped">
                  <thead>
                    <tr>
                      <th class="col-1">{"#"}</th>
                      <th class="col-8">{"Track"}</th>
                      <th>{"Record"}</th>
                      <th>{"Score"}</th>
                    </tr>
                  </thead>
                  <tbody>{for items}</tbody>
                </table>
              </div>
            </div>
          </div>
        }
    }

    fn update(&mut self, _: &Context<Self>, msg: Self::Message) -> bool {
        self.flag = msg;
        true
    }
}

#[derive(PartialEq, Properties)]
struct ItemProps {
    i: i32,
    item: ItemMetadata,
}

struct Item;

impl Component for Item {
    type Message = ();
    type Properties = ItemProps;

    fn create(_: &Context<Self>) -> Self {
        Item
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        html! {
          <tr>
            <th>{props.i}</th>
            <td>{&props.item.name}</td>
            <td>{format!("{}-{}", props.item.wins, props.item.losses)}</td>
            <td>{&props.item.score}</td>
          </tr>
        }
    }
}
