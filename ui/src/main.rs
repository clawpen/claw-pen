mod api;
mod components;
mod types;

use components::dashboard::Dashboard;
use components::login::Login;
use yew::prelude::*;

#[function_component(App)]
fn app() -> Html {
    let authenticated = use_state(|| api::get_token().is_some());

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

    html! {
        <div class="app">
            if *authenticated {
                <header class="header">
                    <h1>{"🦀 Claw Pen"}</h1>
                    <button class="btn-logout" onclick={on_logout}>{"Logout"}</button>
                </header>
                <main class="main">
                    <Dashboard />
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
