use yew::{html, Callback, Children, Component, Context, Html, MouseEvent, Properties};

pub enum AccordionMsg {
    Toggle,
}

#[derive(Clone, PartialEq, Properties)]
pub struct AccordionProps {
    pub children: Children,
    pub header: String,
    pub on_toggle: Option<Callback<MouseEvent>>,
    pub collapsed: Option<bool>,
}

pub struct Accordion {
    collapsed: bool,
}

impl Component for Accordion {
    type Message = AccordionMsg;
    type Properties = AccordionProps;

    fn create(_: &Context<Self>) -> Self {
        Accordion { collapsed: true }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let (button_class, body_class) = if ctx.props().collapsed.unwrap_or(self.collapsed) {
            ("accordion-button collapsed", "accordion-collapse collapse")
        } else {
            ("accordion-button", "accordion-collapse collapse show")
        };
        let onclick = if let Some(on_toggle) = &ctx.props().on_toggle {
            on_toggle.clone()
        } else {
            ctx.link().callback(|_| AccordionMsg::Toggle)
        };
        html! {
            <div class="accordion mb-3">
                <div class="accordion-item">
                    <h2 class="accordion-header">
                        <button class={button_class} {onclick}>{&ctx.props().header}</button>
                    </h2>
                    <div class={body_class}>
                    {for ctx.props().children.iter() }
                    </div>
                </div>
            </div>
        }
    }

    fn update(&mut self, _: &Context<Self>, _: Self::Message) -> bool {
        self.collapsed = !self.collapsed;
        true
    }
}

#[derive(Clone, PartialEq, Properties)]
pub struct CollapseProps {
    pub children: Children,
    pub collapsed: bool,
}

pub struct Collapse;

impl Component for Collapse {
    type Message = ();
    type Properties = CollapseProps;

    fn create(_: &Context<Self>) -> Self {
        Collapse
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let body_class = if ctx.props().collapsed {
            "collapse"
        } else {
            "collapse show"
        };
        html! {
            <div class={body_class}>
                <div class="card card-body bg-light">
                {for ctx.props().children.iter() }
                </div>
            </div>
        }
    }
}
