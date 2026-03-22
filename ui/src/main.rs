mod api;
mod components;
mod types;

use components::dashboard::Dashboard;
use components::login::Login;
use components::teams::Teams;
use yew::prelude::*;

#[function_component(App)]
fn app() -> Html {
    let authenticated = use_state(|| api::get_token().is_some());
    let current_view = use_state(|| "agents".to_string());

    let on_login_success = {
        let authenticated = authenticated.clone();
        Callback::from(move |_| {
            authenticated.set(true);
        })
    };

    let on_logout = {
        let authenticated = authenticated.clone();
        Callback::from(move |_| {
            api::clear_token();
            authenticated.set(false);
        })
    };

    let on_view_change = {
        let current_view = current_view.clone();
        Callback::from(move |view: String| {
            current_view.set(view);
        })
    };

    html! {
        <div class="app">
            if *authenticated {
                <header class="header">
                    <h1>{"🦀 Claw Pen"}</h1>
                    <nav class="nav">
                        <button
                            class={format!("nav-link {}", if *current_view == "agents" { "active" } else { "" })}
                            onclick={on_view_change.reform(|_| "agents".to_string())}
                        >
                            {"Agents"}
                        </button>
                        <button
                            class={format!("nav-link {}", if *current_view == "teams" { "active" } else { "" })}
                            onclick={on_view_change.reform(|_| "teams".to_string())}
                        >
                            {"Teams"}
                        </button>
                    </nav>
                    <button class="btn-logout" onclick={on_logout}>{"Logout"}</button>
                </header>
                <main class="main">
                    {match current_view.as_str() {
                        "agents" => html! { <Dashboard /> },
                        "teams" => html! {
                            <Teams auth_token={api::get_token().unwrap_or_default()} />
                        },
                        _ => html! { <Dashboard /> },
                    }}
                </main>
            } else {
                <Login on_success={on_login_success} />
            }
        </div>
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::Renderer::<App>::new().render();
}
