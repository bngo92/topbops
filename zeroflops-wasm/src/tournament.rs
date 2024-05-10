use crate::base::IframeCompare;
use rand::prelude::SliceRandom;
use std::{borrow::Cow, collections::HashMap};
use web_sys::HtmlSelectElement;
use yew::{html, Callback, Component, Context, Html, NodeRef, Properties};
use zeroflops::{ItemMetadata, List};

#[derive(Eq, PartialEq, Properties)]
pub struct TournamentLoaderProps {
    pub list: List,
}

pub struct TournamentLoader;

impl Component for TournamentLoader {
    type Message = ();
    type Properties = TournamentLoaderProps;

    fn create(_: &Context<Self>) -> Self {
        TournamentLoader
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let mut items: Vec<_> = (0..ctx.props().list.items.len()).collect();
        items.sort_by_key(|&i| -ctx.props().list.items[i].score);
        let previous_ranks = ctx
            .props()
            .list
            .items
            .iter()
            .map(|i| (i.id.clone(), i.rank))
            .collect();
        let bracket = TournamentBracket::new(items, usize::MAX);
        let state = TournamentFields {
            state: TournamentState::Tournament,
            view_state: ViewState::Tournament,
            list: ctx.props().list.clone(),
            previous_ranks,
            bracket,
        };
        html! {
            <Tournament {state}/>
        }
    }
}

pub struct RandomTournamentLoader;

impl Component for RandomTournamentLoader {
    type Message = ();
    type Properties = TournamentLoaderProps;

    fn create(_: &Context<Self>) -> Self {
        RandomTournamentLoader
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let mut items: Vec<_> = (0..ctx.props().list.items.len()).collect();
        items.shuffle(&mut rand::thread_rng());
        let previous_ranks = ctx
            .props()
            .list
            .items
            .iter()
            .map(|i| (i.id.clone(), i.rank))
            .collect();
        let bracket = TournamentBracket::new(items, usize::MAX);
        let state = TournamentFields {
            state: TournamentState::Tournament,
            view_state: ViewState::Tournament,
            list: ctx.props().list.clone(),
            previous_ranks,
            bracket,
        };
        html! {
            <Tournament {state}/>
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<T: Clone> {
    pub item: T,
    pub disabled: bool,
    depth: usize,
    pair: usize,
}

/// Generate a balanced binary tree with enough leaves for all items.
///
/// Items are ordered such that high seeds are matched with low seeds or have bye rounds.
/// The tree is generated by splitting leaf nodes:
///
/// ```text
///      *
///     / \
///    1   2
///
///      *
///     / \
///   *     *
///  / \   / \
/// 1   4 3   2
/// ```
///
/// The tree is actually generated by precalculating indexes instead of iteratively splitting leaf nodes.
/// The tree is also represented as a flat vec.
/// Spaces between nodes represent results between children nodes.
///
/// Start:
/// 1
/// *
/// 4
/// *
/// 3
/// *
/// 2
///
/// End:
/// 1
/// 1
/// 4
/// 1
/// 3
/// 2
/// 2
#[derive(Clone, Eq, PartialEq)]
pub struct TournamentBracket<T: Clone> {
    depth: usize,
    complete_depth: usize,
    initial_data: Vec<Option<Node<T>>>,
    // TODO: reduce number of copies
    pub data: Vec<Option<Node<T>>>,
    finished: Vec<Option<T>>,
    finished_index: usize,
}

impl<T: Clone> TournamentBracket<T> {
    pub fn new(items: Vec<T>, default: T) -> TournamentBracket<T> {
        let complete_depth = (items.len() as f64).log2();
        let depth = complete_depth.ceil() as u32;

        // Build arrays of steps between items with ascending seeds
        // Steps for the next level can be calculated from the previous level
        let mut top = Vec::new();
        let mut next_top = Vec::new();
        let mut bottom = Vec::new();
        let mut next_bottom = Vec::new();
        for d in 0..depth + 1 {
            let len = (2 << d) - 2;
            let mut current = 0;
            interleave(&mut next_top, &mut top);
            for next_i in &top {
                let i = len - 2 * current;
                next_top.push(i);
                current += i + next_i;
            }
            let i = len - 2 * current;
            next_top.push(i);
            current += i - 2;
            interleave(&mut next_bottom, &mut bottom);
            for next_i in &bottom {
                let i = len - 2 * current;
                next_bottom.push(i);
                current += i + next_i;
            }
            let i = len - 2 * current;
            next_bottom.push(i);
        }

        // All nodes with even indexes are leaf nodes
        // The tree is otherwise complete (all other levels are filled) so create nodes at odd
        // indexes
        let mut data: Vec<_> = [
            None,
            Some(Node {
                item: default,
                disabled: true,
                depth: usize::MAX,
                pair: usize::MAX,
            }),
        ]
        .into_iter()
        .cycle()
        .take((2 << depth) - 1)
        .collect();

        // Create leaf nodes in the first two layers
        let items_len = items.len();
        let len = (1 << depth) - items_len;
        let iter = std::iter::once(0)
            .chain(Interleave::new(next_top.into_iter(), top.into_iter()))
            .chain(std::iter::once(-2))
            .chain(Interleave::new(next_bottom.into_iter(), bottom.into_iter()));
        let mut current = 0;
        for (i, (item, step)) in items.into_iter().zip(iter).enumerate() {
            current += step;
            let index = if len > i {
                if current % 4 == 0 {
                    current + 1
                } else {
                    current - 1
                }
            } else {
                current
            };
            data[index as usize] = Some(Node {
                item,
                disabled: false,
                depth: usize::MAX,
                pair: usize::MAX,
            });
        }

        // Iterate over the final set of nodes and assign depth and pair values
        for i in 0..data.len() {
            if let Some(item) = data[i].clone() {
                // This block is only entered once for each node pair
                if item.depth == usize::MAX {
                    let depth = i.trailing_ones() as usize;
                    data[i].as_mut().unwrap().depth = depth;
                    let pair = i + (2 << depth);
                    if pair < data.len() {
                        data[i].as_mut().unwrap().pair = pair;
                        data[pair].as_mut().unwrap().depth = depth;
                        data[pair].as_mut().unwrap().pair = i;
                    }
                }
            }
        }

        TournamentBracket {
            depth: depth as usize,
            complete_depth: complete_depth as usize,
            initial_data: data.clone(),
            data,
            finished: vec![None; items_len],
            finished_index: items_len - 1,
        }
    }

    fn winner(&self) -> &Option<T> {
        &self.finished[0]
    }
}

impl TournamentBracket<usize> {
    /// Assign the node with the index i to win their round.
    ///
    /// The current node pair is disabled and the parent node is updated and enabled.
    fn update<'a>(
        &mut self,
        i: usize,
        lut: &'a mut [ItemMetadata],
    ) -> Option<(&'a ItemMetadata, &'a ItemMetadata)> {
        if let Some(item) = self.data[i].clone() {
            if !item.disabled && !self.data[item.pair].as_ref().unwrap().disabled {
                self.data[i].as_mut().unwrap().disabled = true;
                self.data[item.pair].as_mut().unwrap().disabled = true;
                lut[self.data[item.pair].as_mut().unwrap().item].rank = Some(
                    (1 << (self.complete_depth - self.data[item.pair].as_ref().unwrap().depth)) + 1,
                );
                self.finished[self.finished_index] = self.data[item.pair].as_ref().map(|i| i.item);
                self.finished_index -= 1;
                let win = self.data[i].as_ref().unwrap().item;
                let parent = self.data[(i + item.pair) / 2].as_mut().unwrap();
                if parent.pair == usize::MAX {
                    lut[win].rank = Some(1);
                    self.finished[self.finished_index] = Some(win);
                }
                parent.item = win;
                parent.disabled = false;
                return Some((&lut[win], &lut[self.data[item.pair].as_ref().unwrap().item]));
            }
        }
        None
    }
}

fn interleave(src: &mut Vec<i32>, dst: &mut Vec<i32>) {
    *dst = Interleave::new(src.drain(..).map(|i| -i), std::mem::take(dst).into_iter()).collect();
}

struct Interleave<I: Iterator, J: Iterator<Item = I::Item>> {
    iter1: I,
    iter2: J,
    flag: bool,
}

impl<I: Iterator, J: Iterator<Item = I::Item>> Interleave<I, J> {
    fn new(iter1: I, iter2: J) -> Interleave<I, J> {
        Interleave {
            iter1,
            iter2,
            flag: false,
        }
    }
}

impl<I: Iterator, J: Iterator<Item = I::Item>> Iterator for Interleave<I, J> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        self.flag = !self.flag;
        match self.flag {
            true => self.iter1.next(),
            false => self.iter2.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let h1 = self.iter1.size_hint();
        let h2 = self.iter2.size_hint();
        (h1.0 + h2.0, h1.1.zip(h1.1).map(|(h1, h2)| h1 + h2))
    }
}

#[derive(Clone, PartialEq)]
pub struct TournamentFields {
    state: TournamentState,
    view_state: ViewState,
    list: List,
    previous_ranks: HashMap<String, Option<i32>>,
    bracket: TournamentBracket<usize>,
}

#[derive(Clone, PartialEq)]
enum TournamentState {
    Tournament,
    Match,
}

#[derive(Clone, PartialEq)]
enum ViewState {
    Tournament,
    List,
}

pub enum Msg {
    Update(usize),
    Toggle,
    SelectView,
    Reset,
}

#[derive(PartialEq, Properties)]
pub struct TournamentProps {
    pub state: TournamentFields,
}

pub struct Tournament {
    state: TournamentFields,
    select_ref: NodeRef,
}

impl Component for Tournament {
    type Message = Msg;
    type Properties = TournamentProps;

    fn create(ctx: &Context<Self>) -> Self {
        Tournament {
            state: ctx.props().state.clone(),
            select_ref: NodeRef::default(),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let fields = &self.state;
        let (toggle, html) = match &fields.state {
            TournamentState::Tournament => ("Match Mode", {
                let winner = if let Some(winner) = fields.bracket.winner() {
                    fields.list.items.get(*winner)
                } else {
                    None
                };
                html! {
                    <div>
                        if let Some(winner) = winner {
                            <h2>{format!("Winner: {}", winner.name)}</h2>
                            // TODO: only show if iframe exists
                            <div class="row">
                                <div class="col-6">
                                    <iframe width="100%" height="380" frameborder="0" src={winner.iframe.clone()}></iframe>
                                </div>
                            </div>
                        }
                        <div class="overflow-scroll">
                        {tournament_bracket_view(&fields.bracket, &fields.list.items, ctx.link().callback(Msg::Update), false)}
                        </div>
                        if let Some(src) = fields.list.iframe.clone() {
                            <div class="row">
                                <div class="col-12 col-lg-10 col-xl-8">
                                    <iframe width="100%" height="380" frameborder="0" {src}></iframe>
                                </div>
                            </div>
                        }
                    </div>
                }
            }),
            TournamentState::Match => ("Tournament Mode", self.match_view(fields, ctx)),
        };
        html! {
            <div>
                <div class="d-flex gap-3">
                    <button type="button" class="btn btn-primary mb-1" onclick={ctx.link().callback(|_| Msg::Toggle)} style="width: 156.33px">{toggle}</button>
                    <button type="button" class="btn btn-danger mb-1" onclick={ctx.link().callback(|_| Msg::Reset)}>{"Reset"}</button>
                </div>
                {html}
            </div>
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let fields = &mut self.state;
        match msg {
            Msg::Update(i) => {
                if let Some((win, lose)) = fields.bracket.update(i, &mut fields.list.items) {
                    let id = fields.list.id.clone();
                    let win = win.id.clone();
                    let lose = lose.id.clone();
                    let updated_ranks = if fields.bracket.winner().is_some() {
                        fields.list.items.iter().map(|i| i.rank).collect()
                    } else {
                        Vec::new()
                    };
                    ctx.link().send_future_batch(async move {
                        crate::update_stats(&id, &win, &lose).await.unwrap();
                        if !updated_ranks.is_empty() {
                            // TODO: handle state syncing better
                            let mut list = crate::fetch_list(&id).await.unwrap().unwrap();
                            for (item, rank) in &mut list.items.iter_mut().zip(updated_ranks) {
                                item.rank = rank;
                            }
                            crate::update_list(&list).await.unwrap();
                        }
                        Vec::new()
                    });
                }
            }
            Msg::Toggle => {
                fields.state = match fields.state {
                    TournamentState::Tournament => TournamentState::Match,
                    TournamentState::Match => TournamentState::Tournament,
                };
            }
            Msg::SelectView => {
                fields.view_state = match self
                    .select_ref
                    .cast::<HtmlSelectElement>()
                    .map(|s| s.value())
                    .as_deref()
                    .unwrap_or("Tournament View")
                {
                    "Tournament View" => ViewState::Tournament,
                    "List View" => ViewState::List,
                    _ => unreachable!(),
                };
            }
            Msg::Reset => {
                fields.bracket.data = fields.bracket.initial_data.clone();
                for item in &mut fields.bracket.finished {
                    *item = None;
                }
                fields.bracket.finished_index = fields.bracket.finished.len() - 1;
            }
        }
        true
    }
}

impl Tournament {
    fn match_view(&self, fields: &TournamentFields, ctx: &Context<Self>) -> Html {
        let winner = if let Some(winner) = fields.bracket.winner() {
            fields.list.items.get(*winner)
        } else {
            None
        };
        let select = if let Some(winner) = winner {
            html! {
                <div>
                    <h2>{format!("Winner: {}", winner.name)}</h2>
                    <div class="row">
                        <div class="col-6">
                            <iframe width="100%" height="380" frameborder="0" src={winner.iframe.clone()}></iframe>
                        </div>
                    </div>
                </div>
            }
        } else {
            // TODO: save last position instead of always starting from the beginning
            let mut start_i = 0;
            let mut step = 2;
            let mut found = None;
            'found: while start_i != fields.bracket.data.len() / 2 {
                let mut i = start_i;
                while i < fields.bracket.data.len() {
                    if let Some(item) = &fields.bracket.data[i] {
                        if !item.disabled {
                            let pair = fields.bracket.data[item.pair].as_ref().unwrap();
                            if !pair.disabled {
                                let left_callback = ctx.link().callback(Msg::Update);
                                let on_left_select = Callback::from(move |_| left_callback.emit(i));
                                let right_callback = ctx.link().callback(Msg::Update);
                                let pair_i = item.pair;
                                let on_right_select =
                                    Callback::from(move |_| right_callback.emit(pair_i));
                                found = Some((
                                    fields.list.items[item.item].clone(),
                                    on_left_select,
                                    fields.list.items[pair.item].clone(),
                                    on_right_select,
                                ));
                                break 'found;
                            }
                        }
                    }
                    i += step;
                }
                start_i += step / 2;
                step *= 2;
            }
            let (left, on_left_select, right, on_right_select) = found.unwrap();
            html! {<IframeCompare {left} {on_left_select} {right} {on_right_select}/>}
        };
        let view = if let ViewState::Tournament = fields.view_state {
            html! {
                <div class="overflow-scroll">
                {tournament_bracket_view(&fields.bracket, &fields.list.items, ctx.link().callback(Msg::Update), true)}
                </div>
            }
        } else {
            let items = fields
                .bracket
                .finished
                .iter()
                .map(|i| {
                    i.as_ref().map(|i| {
                        let i = &fields.list.items[*i];
                        (
                            i.rank.unwrap(),
                            Cow::from(vec![
                                i.name.clone(),
                                fields.previous_ranks[&i.id]
                                    .map(|i| i.to_string())
                                    .unwrap_or_default(),
                                i.score.to_string(),
                            ]),
                        )
                    })
                })
                .collect();
            crate::base::responsive_table_view(&["Track", "Prev. Rank", "Score"], items)
        };
        html! {
            <div>
                {select}
                <div class="row mt-4">
                    <div class="col-auto">
                        <select ref={self.select_ref.clone()} class="form-select" onchange={ctx.link().callback(|_| Msg::SelectView)}>
                            <option selected={matches!(fields.view_state, ViewState::Tournament)}>{"Tournament View"}</option>
                            <option selected={matches!(fields.view_state, ViewState::List)}>{"List View"}</option>
                        </select>
                    </div>
                </div>
                {view}
            </div>
        }
    }
}

fn tournament_bracket_view(
    bracket: &TournamentBracket<usize>,
    lut: &[ItemMetadata],
    on_click_select: Callback<usize>,
    disabled: bool,
) -> Html {
    // We want to limit the width of tournament buttons to between 168px and 1/6 of a bootstrap
    // container
    // 168px is the minimum width that avoids truncating Bop To The Top
    let depth = std::cmp::max(bracket.depth + 1, 6);
    let row_width = format!("min-width: {}px", 168 * depth);
    let offsets: Vec<_> = std::iter::once(None)
        .chain((1..depth).map(|i| {
            Some(html! {<div style={format!("width: {}%", 100. * i as f64 / depth as f64)}></div>})
        }))
        .collect();
    let col_width = format!("width: {}%", 100. / depth as f64);
    bracket
        .data
        .iter()
        .enumerate()
        .map(|(i, item)| {
            if let Some(item) = item {
                let onclick = on_click_select.clone();
                let onclick = Callback::from(move |_| onclick.emit(i));
                let title = if item.item == usize::MAX {
                    String::new()
                } else {
                    lut[item.item].name.clone()
                };
                let disabled = disabled || item.disabled;
                html! {
                    <div class="row" style={row_width.clone()}>
                    {for offsets[item.depth].clone()}
                        <div style={col_width.clone()}>
                            <button type="button" class="btn btn-success text-truncate w-100" style="height: 38px" {disabled} {onclick}>{title}</button>
                        </div>
                    </div>
                }
            } else {
                html! { <div style="height: 38px"></div> }
            }
        })
        .collect()
}
