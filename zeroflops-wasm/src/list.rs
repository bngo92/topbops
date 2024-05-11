use crate::{ListsRoute, UserProps};
use yew::{html, Component, Context, Html};
use yew_router::{prelude::Link, scope_ext::RouterScopeExt};
use zeroflops::List;

pub mod item;

pub enum ListsMsg {
    Load(Vec<List>),
    Create,
}

pub struct Lists {
    lists: Vec<List>,
}

impl Component for Lists {
    type Message = ListsMsg;
    type Properties = UserProps;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_future(async move {
            let lists = crate::fetch_lists(false).await.unwrap();
            ListsMsg::Load(lists)
        });
        Lists { lists: Vec::new() }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let list_html = self.lists.iter().map(|l| {
            html! {
                <div class="col-12 col-md-6 mb-4">
                    <div class="card">
                        <div class="card-body">
                            <Link<ListsRoute> to={ListsRoute::View{id: l.id.clone()}}>{&l.name}</Link<ListsRoute>>
                        </div>
                    </div>
                </div>
            }
        });
        let disabled = !ctx.props().logged_in;
        let create = ctx.link().callback(|_| ListsMsg::Create);
        crate::nav_content(
            html! {
              <ul class="navbar-nav me-auto">
                <li class="navbar-brand">{"All Lists"}</li>
              </ul>
            },
            html! {
              <div>
                <div class="row mt-3">
                  {for list_html}
                </div>
                <button type="button" class="btn btn-primary" onclick={create} {disabled}>{"Create List"}</button>
              </div>
            },
        )
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            ListsMsg::Load(lists) => {
                self.lists = lists;
                true
            }
            ListsMsg::Create => {
                let navigator = ctx.link().navigator().unwrap();
                ctx.link().send_future_batch(async move {
                    let list = crate::create_list().await.unwrap();
                    navigator.push(&ListsRoute::Edit { id: list.id });
                    None
                });
                false
            }
        }
    }
}
