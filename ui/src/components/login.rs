use crate::api::{self, AuthStatus};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct LoginProps {
    pub on_success: Callback<()>,
}

#[function_component(Login)]
pub fn login(props: &LoginProps) -> Html {
    let auth_status = use_state(|| None::<AuthStatus>);
    let password = use_state(String::new);
    let error = use_state(String::new);
    let loading = use_state(|| false);
    let is_setup = use_state(|| false);

    // Fetch auth status on mount
    let auth_status_clone = auth_status.clone();
    let is_setup_clone = is_setup.clone();
    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            match api::get_auth_status().await {
                Ok(status) => {
                    // If auth is disabled, just proceed
                    if !status.auth_enabled {
                        api::store_token("dev-mode");
                    }
                    // If no admin exists, this is first-time setup
                    is_setup_clone.set(!status.has_admin);
                    auth_status_clone.set(Some(status));
                }
                Err(e) => {
                    web_sys::console::log_1(&format!("Failed to get auth status: {}", e).into());
                }
            }
        });
        || ()
    });

    let on_password_input = {
        let password = password.clone();
        Callback::from(move |e: InputEvent| {
            let value = e
                .target_unchecked_into::<web_sys::HtmlInputElement>()
                .value();
            password.set(value);
        })
    };

    let on_submit = {
        let password = password.clone();
        let error = error.clone();
        let loading = loading.clone();
        let on_success = props.on_success.clone();
        let is_setup = *is_setup;

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let password = (*password).clone();
            if password.is_empty() {
                error.set("Please enter a password".to_string());
                return;
            }

            let error = error.clone();
            let loading = loading.clone();
            let on_success = on_success.clone();
            loading.set(true);
            error.set(String::new());

            wasm_bindgen_futures::spawn_local(async move {
                let result = if is_setup {
                    // First-time setup: register then login
                    match api::register(&password).await {
                        Ok(()) => api::login(&password).await,
                        Err(e) => Err(e),
                    }
                } else {
                    api::login(&password).await
                };

                loading.set(false);

                match result {
                    Ok(token_response) => {
                        api::store_token(&token_response.access_token);
                        on_success.emit(());
                    }
                    Err(e) => {
                        error.set(e);
                    }
                }
            });
        })
    };

    html! {
        <div class="login-container">
            <div class="login-box">
                <h1>{"🦀 Claw Pen"}</h1>
                if let Some(_status) = &*auth_status {
                    if *is_setup {
                        <div class="setup-message">
                            <h2>{"Welcome!"}</h2>
                            <p>{"Create an admin password to get started."}</p>
                        </div>
                    } else {
                        <h2>{"Login"}</h2>
                    }
                } else {
                    <div class="loading">{"Loading..."}</div>
                }

                if auth_status.is_some() {
                    <form onsubmit={on_submit}>
                        <div class="form-group">
                            <input
                                type="password"
                                placeholder={if *is_setup { "Create password (min 8 chars)" } else { "Password" }}
                                value={(*password).clone()}
                                oninput={on_password_input}
                                disabled={*loading}
                            />
                        </div>

                        if !error.is_empty() {
                            <div class="error-message">{&*error}</div>
                        }

                        <button type="submit" class="btn-primary" disabled={*loading}>
                            if *loading {
                                {"Logging in..."}
                            } else if *is_setup {
                                {"Create Account"}
                            } else {
                                {"Login"}
                            }
                        </button>
                    </form>
                }
            </div>
        </div>
    }
}
