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

    fn create(ctx: &Context<Self>) -> Self {
        Accordion {
            collapsed: ctx.props().collapsed.unwrap_or(true),
        }
    }

    fn update(&mut self, _: &Context<Self>, _: Self::Message) -> bool {
        self.collapsed = !self.collapsed;
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, _: &Self::Properties) -> bool {
        if let Some(collapsed) = ctx.props().collapsed {
            self.collapsed = collapsed;
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let (button_class, body_class) = if self.collapsed {
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
}

#[derive(Clone, PartialEq, Properties)]
pub struct AlertProps {
    pub result: Result<String, String>,
    pub hide: Callback<MouseEvent>,
}

pub struct Alert;

impl Component for Alert {
    type Message = ();
    type Properties = AlertProps;

    fn create(_: &Context<Self>) -> Self {
        Alert
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let (alert_class, body) = match &ctx.props().result {
            Ok(msg) => ("alert alert-success alert-dismissible", msg),
            Err(msg) => ("alert alert-danger alert-dismissible", msg),
        };
        let onclick = &ctx.props().hide;
        html! {
            <div class={alert_class}>
                {body}
                <button type="button" class="btn-close" {onclick}></button>
            </div>
        }
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

#[derive(Clone, PartialEq, Properties)]
pub struct ModalProps {
    pub header: String,
    pub children: Children,
    pub hide: Callback<MouseEvent>,
}

pub struct Modal;

impl Component for Modal {
    type Message = ();
    type Properties = ModalProps;

    fn create(_: &Context<Self>) -> Self {
        Modal
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onclick = ctx.props().hide.clone();
        html! {
            <div>
                <div class="modal d-block" onclick={onclick.clone()}>
                    <div class="modal-dialog" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                        <div class="modal-content">
                            <div class="modal-header">
                                <h1 class="modal-title">{&ctx.props().header}</h1>
                                <button type="button" class="btn-close" {onclick}></button>
                            </div>
                            {for ctx.props().children.iter()}
                        </div>
                    </div>
                </div>
                <div class="modal-backdrop show"></div>
            </div>
        }
    }
}
