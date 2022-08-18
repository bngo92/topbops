use yew::{html, Children, Component, Context, Html, Properties};

pub enum AccordionMsg {
    Toggle,
}

#[derive(Clone, PartialEq, Properties)]
pub struct AccordionProps {
    pub children: Children,
    pub header: String,
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
        let (button_class, body_class) = if self.collapsed {
            ("accordion-button collapsed", "accordion-collapse collapse")
        } else {
            ("accordion-button", "accordion-collapse collapse show")
        };
        html! {
            <div class="accordion">
                <div class="accordion-item">
                    <h2 class="accordion-header">
                        <button class={button_class} onclick={ctx.link().callback(|_| AccordionMsg::Toggle)}>{&ctx.props().header}</button>
                    </h2>
                    <div class={body_class}>
                        <div class="accordion-body">
                        {for ctx.props().children.iter() }
                        </div>
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
