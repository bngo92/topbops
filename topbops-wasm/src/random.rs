use topbops::{ItemMetadata, ItemQuery};
use yew::{html, Callback, Component, Context, Html, MouseEvent, Properties};

#[derive(PartialEq, Properties)]
pub struct RandomProps {
    pub mode: String,
    pub left: ItemMetadata,
    pub on_left_select: Callback<MouseEvent>,
    pub right: ItemMetadata,
    pub on_right_select: Callback<MouseEvent>,
    pub query: ItemQuery,
}

pub struct Random;

impl Component for Random {
    type Message = ();
    type Properties = RandomProps;

    fn create(_: &Context<Self>) -> Self {
        Random
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
        let (left_items, right_items): (Vec<_>, Vec<_>) = query
            .items
            .iter()
            .zip(1..)
            .map(|(item, i)| {
                (
                    i,
                    html! {<Item i={i} item={item.metadata.clone().unwrap()}/>},
                )
            })
            .partition(|(i, _)| i % 2 == 1);
        let left_items = left_items.into_iter().map(|(_, item)| item);
        let right_items = right_items.into_iter().map(|(_, item)| item);
        html! {
          <div>
            <h1>{mode}</h1>
            <div class="row">
              <div class="col-6">
                <iframe id="iframe1" width="100%" height="380" frameborder="0" src={left.iframe.clone()}></iframe>
                <button type="button" class="btn btn-info width" onclick={on_left_select.clone()}>{&left.name}</button>
              </div>
              <div class="col-6">
                <iframe id="iframe2" width="100%" height="380" frameborder="0" src={right.iframe.clone()}></iframe>
                <button type="button" class="btn btn-warning width" onclick={on_right_select.clone()}>{&right.name}</button>
              </div>
            </div>
            <div class="row">
              <div class="col-6">
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
              <div class="col-6">
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
            </div>
          </div>
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct ItemProps {
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
