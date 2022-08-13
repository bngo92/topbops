use topbops::ItemQuery;
use web_sys::HtmlSelectElement;
use yew::{html, Component, Context, Html, NodeRef, Properties};

pub enum Msg {
    Load,
    Update(ItemQuery),
}

pub struct Search {
    search_ref: NodeRef,
    query: Option<ItemQuery>,
}

impl Component for Search {
    type Message = Msg;
    type Properties = ();

    fn create(_: &Context<Self>) -> Self {
        Search {
            search_ref: NodeRef::default(),
            query: None,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let default_search = "SELECT name, user_score FROM tracks";
        let search = ctx.link().callback(|_| Msg::Load);
        html! {
            <div>
                <form>
                    <div class="row">
                        <div class="col-12 col-md-10 col-xl-11 pt-1">
                            <input ref={self.search_ref.clone()} type="text" class="col-12" placeholder={default_search}/>
                        </div>
                        <div class="col-3 col-sm-2 col-md-2 col-xl-1 pe-2">
                            <button type="button" class="col-12 btn btn-success" onclick={search}>{"Search"}</button>
                        </div>
                    </div>
                </form>
                if let Some(query) = &self.query {
                    <Table query={query.clone()}/>
                }
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => {
                let input = self.search_ref.cast::<HtmlSelectElement>().unwrap().value();
                ctx.link().send_future(async move {
                    Msg::Update(crate::find_items(&input).await.unwrap())
                });
                false
            }
            Msg::Update(query) => {
                self.query = Some(query);
                true
            }
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct TableProps {
    query: ItemQuery,
}

struct Table;

impl Component for Table {
    type Message = ();
    type Properties = TableProps;

    fn create(_: &Context<Self>) -> Self {
        Table
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let query = &ctx.props().query;
        html! {
            <div class="table-responsive">
                <table class="table table-striped">
                    <thead>
                        <tr>
                            <th>{"#"}</th>
                            {for query.fields.iter().map(|item| html! {
                                <th>{item}</th>
                            })}
                        </tr>
                    </thead>
                    <tbody>{for query.items.iter().zip(1..).map(|(item, i)| html! {
                        <Row i={i} values={item.values.clone()}/>
                    })}</tbody>
                </table>
            </div>
        }
    }
}

#[derive(Eq, PartialEq, Properties)]
pub struct RowProps {
    i: i32,
    values: Vec<String>,
}

struct Row;

impl Component for Row {
    type Message = ();
    type Properties = RowProps;

    fn create(_: &Context<Self>) -> Self {
        Row
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
          <tr>
            <th>{ctx.props().i}</th>
            {for ctx.props().values.iter().map(|item| html! {
                <td>{item}</td>
            })}
          </tr>
        }
    }
}
