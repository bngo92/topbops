use std::borrow::Cow;
use yew::{html, Callback, Component, Context, Html, MouseEvent, Properties};
use zeroflops::ItemMetadata;

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

pub fn responsive_table_view(
    header: &[&str],
    items: Vec<Option<(i32, Cow<'_, [String]>)>>,
) -> Html {
    let (left_items, right_items): (Vec<_>, Vec<_>) = items
        .iter()
        .cloned()
        .zip(1..)
        .partition(|(_, i)| i % 2 == 1);
    let left_items = left_items.into_iter().map(|(item, _)| item);
    let right_items = right_items.into_iter().map(|(item, _)| item);
    html! {
        <div class="row">
            <div class="col-md-6 d-none d-lg-block">
            {table_view(header, left_items)}
            </div>
            <div class="col-md-6 d-none d-lg-block">
            {table_view(header, right_items)}
            </div>
            <div class="col-12 d-lg-none">
            {table_view(header, items.into_iter())}
            </div>
        </div>
    }
}

pub fn table_view<'a>(
    header: &[&str],
    items: impl Iterator<Item = Option<(i32, Cow<'a, [String]>)>>,
) -> Html {
    html! {
        <div class="table-responsive">
            <table class="table table-striped mb-0">
                <thead>
                    <tr>
                        <th>{"#"}</th>
                        {for header.iter().map(|item| html! {
                            <th>{item}</th>
                        })}
                    </tr>
                </thead>
                <tbody>{for items.map(|item| item_view(item, header.len()))}</tbody>
            </table>
        </div>
    }
}

fn item_view(item: Option<(i32, Cow<[String]>)>, len: usize) -> Html {
    if let Some((i, item)) = item {
        html! {
            <tr>
                <th>{i}</th>
                {for item.iter().take(len).map(|item| html! {
                    <td class="text-truncate max-width">{item}</td>
                })}
            </tr>
        }
    } else {
        html! {
            <tr style="height: 41.5px">
                <th></th>
                <td class="td"></td>
                <td></td>
                <td></td>
            </tr>
        }
    }
}
