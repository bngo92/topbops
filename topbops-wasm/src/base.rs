use topbops::{ItemMetadata, ItemQuery};
use yew::{html, Callback, Component, Context, Html, MouseEvent, Properties};

pub enum IframeCompareMsg {
    Left,
    Right,
}

#[derive(Clone, PartialEq, Properties)]
pub struct IframeCompareProps {
    pub left: ItemMetadata,
    pub on_left_select: Callback<MouseEvent>,
    pub right: ItemMetadata,
    pub on_right_select: Callback<MouseEvent>,
}

pub struct IframeCompare {
    flag: IframeCompareMsg,
}

impl Component for IframeCompare {
    type Message = IframeCompareMsg;
    type Properties = IframeCompareProps;

    fn create(_: &Context<Self>) -> Self {
        IframeCompare {
            flag: IframeCompareMsg::Left,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let IframeCompareProps {
            left,
            on_left_select,
            right,
            on_right_select,
        } = ctx.props();
        let (left_class, right_class, src) = match self.flag {
            IframeCompareMsg::Left => ("nav-link active", "nav-link", left.iframe.clone()),
            IframeCompareMsg::Right => ("nav-link", "nav-link active", right.iframe.clone()),
        };
        html! {
        <div class="row">
          <div class="col-12 d-lg-none">
            <ul class="nav nav-tabs nav-justified">
              <li class="nav-item">
                <a class={left_class} aria-label="Show left item" href="# " onclick={ctx.link().callback(|_| IframeCompareMsg::Left)}>{&left.name}</a>
              </li>
              <li class="nav-item">
                <a class={right_class} href="# " onclick={ctx.link().callback(|_| IframeCompareMsg::Right)}>{&right.name}</a>
              </li>
            </ul>
            <iframe width="100%" height="380" frameborder="0" {src}></iframe>
          </div>
          <div class="col-md-6 d-none d-lg-block">
            <iframe width="100%" height="380" frameborder="0" src={left.iframe.clone()}></iframe>
          </div>
          <div class="col-md-6 d-none d-lg-block">
            <iframe width="100%" height="380" frameborder="0" src={right.iframe.clone()}></iframe>
          </div>
          <div class="col-6">
            <button type="button" class="btn btn-info text-truncate w-100" onclick={on_left_select.clone()}>{&left.name}</button>
          </div>
          <div class="col-6">
            <button type="button" class="btn btn-warning text-truncate w-100" onclick={on_right_select.clone()}>{&right.name}</button>
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
pub struct TableProps {
    pub query: ItemQuery,
}

pub struct ResponsiveTable;

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
pub struct ItemProps {
    pub i: i32,
    pub item: ItemMetadata,
}

pub struct Item;

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
            <td class="td">{&props.item.name}</td>
            <td>{format!("{}-{}", props.item.wins, props.item.losses)}</td>
            <td>{&props.item.score}</td>
          </tr>
        }
    }
}
