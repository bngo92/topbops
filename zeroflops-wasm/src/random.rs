use crate::base::IframeCompare;
use rand::prelude::SliceRandom;
use std::borrow::Cow;
use yew::{html, Component, Context, Html, Properties};
use zeroflops::{ItemMetadata, ItemQuery};

#[derive(Clone, PartialEq, Properties)]
pub struct MatchProps {
    pub id: String,
}

pub struct RandomMatches;

impl Component for RandomMatches {
    type Message = ();
    type Properties = MatchProps;

    fn create(_: &Context<Self>) -> Self {
        RandomMatches
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <Match id={ctx.props().id.clone()} mode={Mode::Match}/>
        }
    }
}

pub struct RandomRounds;

impl Component for RandomRounds {
    type Message = ();
    type Properties = MatchProps;

    fn create(_: &Context<Self>) -> Self {
        RandomRounds
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <Match id={ctx.props().id.clone()} mode={Mode::Round}/>
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
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
    UpdateStats((String, String)),
}

#[derive(Clone, PartialEq, Properties)]
pub struct MatchComponentProps {
    pub id: String,
    pub mode: Mode,
}

pub struct Match {
    random_queue: Vec<zeroflops::Item>,
    data: Option<MatchData>,
}

impl Component for Match {
    type Message = Msg;
    type Properties = MatchComponentProps;

    fn create(ctx: &Context<Self>) -> Self {
        let id = ctx.props().id.clone();
        ctx.link().send_future(async move {
            let query = crate::query_items(&id).await.unwrap();
            Msg::LoadRandom(query)
        });
        Match {
            random_queue: Vec::new(),
            data: None,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let Some(MatchData { left, right, query }) = self.data.clone() else {
            return html! {};
        };
        let left_param = (left.id.clone(), right.id.clone());
        let on_left_select = ctx
            .link()
            .callback(move |_| Msg::UpdateStats(left_param.clone()));
        let right_param = (right.id.clone(), left.id.clone());
        let on_right_select = ctx
            .link()
            .callback(move |_| Msg::UpdateStats(right_param.clone()));
        let items = query
            .items
            .iter()
            .zip(1..)
            .map(|(item, i)| {
                item.metadata.as_ref().map(|m| {
                    (
                        i,
                        Cow::from(vec![
                            m.name.to_owned(),
                            format!("{}-{}", m.wins, m.losses),
                            m.score.to_string(),
                        ]),
                    )
                })
            })
            .collect();
        html! {
            <div>
                <IframeCompare left={left} {on_left_select} right={right} {on_right_select}/>
                {crate::base::responsive_table_view(&["Track", "Record", "Score"], items)}
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadRandom(query) => {
                let (left, right) = match ctx.props().mode {
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
            Msg::UpdateStats((win, lose)) => {
                let list = ctx.props().id.clone();
                ctx.link().send_future(async move {
                    crate::update_stats(&list, &win, &lose).await.unwrap();
                    let query = crate::query_items(&list).await.unwrap();
                    Msg::LoadRandom(query)
                });
                false
            }
        }
    }
}
