mod components;
mod api;
mod types;

use yew::prelude::*;
use components::dashboard::Dashboard;

#[function_component(App)]
fn app() -> Html {
    html! {
        <div class="app">
            <header class="header">
                <h1>{"🦀 Claw Pen"}</h1>
            </header>
            <main class="main">
                <Dashboard />
            </main>
        </div>
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::Renderer::<App>::new().render();
}
