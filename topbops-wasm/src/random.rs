use crate::base::IframeCompare;
use rand::prelude::SliceRandom;
use std::collections::HashMap;
use topbops::{ItemMetadata, ItemQuery};
use yew::{html, Component, Context, Html, Properties};
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
        let on_left_select = ctx
            .link()
            .callback_once(move |_| Msg::UpdateStats(left_param));
        let right_param = (user, list.clone(), right.id.clone(), left.id.clone());
        let on_right_select = ctx
            .link()
            .callback_once(move |_| Msg::UpdateStats(right_param));
        html! {
            <div>
                <h1>{mode}</h1>
                <IframeCompare left={left} {on_left_select} right={right} {on_right_select}/>
                <ResponsiveTable query={query}/>
            </div>
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

#[derive(PartialEq, Properties)]
struct TableProps {
    query: ItemQuery,
}

struct ResponsiveTable;

impl Component for ResponsiveTable {
    type Message = ();
    type Properties = TableProps;

    fn create(_: &Context<Self>) -> Self {
        ResponsiveTable
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let items: Vec<_> = ctx
            .props()
            .query
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
        }
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
