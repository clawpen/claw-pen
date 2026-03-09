use crate::components::chat::ChatPanel;
use crate::types::AgentContainer;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct TabbedChatProps {
    pub open_agents: Vec<AgentContainer>,
    pub on_close_tab: Callback<String>, // agent_id
}

pub struct TabbedChat {
    active_index: usize,
}

pub enum TabbedChatMsg {
    CloseTab(String),
    SelectTab(usize),
}

impl Component for TabbedChat {
    type Message = TabbedChatMsg;
    type Properties = TabbedChatProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            active_index: 0,
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        // Reset active index if it's out of bounds
        if self.active_index >= ctx.props().open_agents.len() && !ctx.props().open_agents.is_empty() {
            self.active_index = ctx.props().open_agents.len() - 1;
        }
        true
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            TabbedChatMsg::CloseTab(agent_id) => {
                ctx.props().on_close_tab.emit(agent_id);
                true
            }
            TabbedChatMsg::SelectTab(index) => {
                if index < ctx.props().open_agents.len() {
                    self.active_index = index;
                }
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        html! {
            <div class="tabbed-chat">
                // Tab bar
                <div class="tab-bar">
                    {for ctx.props().open_agents.iter().enumerate().map(|(i, agent)| {
                        let is_active = i == self.active_index;
                        let on_select = link.callback(move |_| TabbedChatMsg::SelectTab(i));
                        let agent_id = agent.id.clone();
                        let on_close = link.callback(move |e: MouseEvent| {
                            e.stop_propagation();
                            TabbedChatMsg::CloseTab(agent_id.clone())
                        });
                        html! {
                            <div 
                                class={if is_active { "tab active" } else { "tab" }}
                                onclick={on_select}
                            >
                                <span class="tab-name">{&agent.name}</span>
                                <button class="tab-close" onclick={on_close}>{"×"}</button>
                            </div>
                        }
                    })}
                </div>

                // Chat content - now using real ChatPanel
                <div class="tab-content">
                    if let Some(agent) = ctx.props().open_agents.get(self.active_index) {
                        <ChatPanel 
                            agent={agent.clone()}
                            on_close={ctx.props().on_close_tab.reform({
                                let id = agent.id.clone();
                                move |_| id.clone()
                            })}
                        />
                    } else {
                        <div class="chat-placeholder">
                            <p class="empty-message">{"Select an agent to start chatting"}</p>
                        </div>
                    }
                </div>
            </div>
        }
    }
}
