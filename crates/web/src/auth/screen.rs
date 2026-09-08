//! Signing in, signing up, and getting back in.
//!
//! The one screen that has to work before the router exists, because it is what decides whether
//! there is anything to route to. A password-reset link is picked out of the address bar
//! directly, for the same reason.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::local;
use crate::api::Api;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    SignIn,
    Register,
    Forgot,
    Reset,
    Totp,
}

impl Mode {
    fn heading(self) -> &'static str {
        match self {
            Mode::SignIn => "Sign in",
            Mode::Register => "Create an account",
            Mode::Forgot => "Reset your password",
            Mode::Reset => "Choose a new password",
            Mode::Totp => "Two-factor code",
        }
    }
}

fn path() -> String {
    web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_default()
}

fn reset_token() -> Option<String> {
    path()
        .strip_prefix("/auth/reset/")
        .map(str::to_owned)
        .filter(|token| !token.is_empty())
}

#[component]
pub fn AuthScreen(
    api: Api,
    /// Called once a session exists. The app reloads the account and shows the library.
    on_signed_in: Callback<()>,
    on_local_mode: Callback<local::LocalMode>,
) -> impl IntoView {
    let token = reset_token();
    let invited = path().starts_with("/invite/");

    let mode = RwSignal::new(if token.is_some() {
        Mode::Reset
    } else {
        Mode::SignIn
    });
    let email = RwSignal::new(String::new());
    let display_name = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let code = RwSignal::new(String::new());
    let challenge = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let note = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();

        let api = api.clone();
        let token = token.clone();

        error.set(None);
        busy.set(true);

        spawn_local(async move {
            let outcome = match mode.get_untracked() {
                Mode::SignIn => match api
                    .sign_in(&email.get_untracked(), &password.get_untracked())
                    .await
                {
                    Ok(answer) => {
                        // A correct password alone is not a session once a second factor is
                        // enrolled; the server hands back a challenge instead.
                        match answer.challenge_id {
                            Some(id) if answer.totp_required => {
                                challenge.set(id);
                                mode.set(Mode::Totp);
                            }
                            _ => on_signed_in.run(()),
                        }

                        Ok(())
                    }
                    Err(error) => Err(error),
                },

                Mode::Register => api
                    .register(
                        &email.get_untracked(),
                        &display_name.get_untracked(),
                        &password.get_untracked(),
                    )
                    .await
                    .map(|_| on_signed_in.run(())),

                Mode::Totp => api
                    .answer_challenge(&challenge.get_untracked(), &code.get_untracked())
                    .await
                    .map(|_| on_signed_in.run(())),

                Mode::Forgot => api.forgot_password(&email.get_untracked()).await.map(|()| {
                    // Deliberately the same message whether or not the address is registered.
                    note.set(Some(
                        "If that address has an account, a reset link is on its way.".to_owned(),
                    ));
                    mode.set(Mode::SignIn);
                }),

                Mode::Reset => api
                    .reset_password(
                        token.as_deref().unwrap_or_default(),
                        &password.get_untracked(),
                    )
                    .await
                    .map(|()| {
                        note.set(Some("Your password is set. Sign in with it.".to_owned()));
                        mode.set(Mode::SignIn);

                        if let Some(history) = web_sys::window().and_then(|w| w.history().ok()) {
                            let _ = history.replace_state_with_url(
                                &wasm_bindgen::JsValue::NULL,
                                "",
                                Some("/"),
                            );
                        }
                    }),
            };

            if let Err(problem) = outcome {
                error.set(Some(problem.to_string()));
            }

            busy.set(false);
        });
    };

    view! {
        <div class="flex min-h-dvh items-center justify-center bg-white p-6 text-slate-900">
            <form class="w-80 space-y-3" on:submit=submit>
                <h1 class="text-xl font-semibold">{move || mode.get().heading()}</h1>

                <Show when=move || invited && mode.get() == Mode::SignIn>
                    <p class="rounded border border-sky-300 bg-sky-50 p-2 text-sm text-sky-900">
                        "Sign in with the address the invitation was sent to, and it will be waiting."
                    </p>
                </Show>

                <Show when=move || note.get().is_some()>
                    <p class="text-sm text-sky-700">{move || note.get()}</p>
                </Show>
                <Show when=move || error.get().is_some()>
                    <p class="text-sm text-red-600" data-testid="auth-error">{move || error.get()}</p>
                </Show>

                <Show
                    when=move || mode.get() == Mode::Totp
                    fallback=move || view! {
                        <Show when=move || mode.get() != Mode::Reset>
                            <input
                                class="w-full rounded border border-slate-300 px-3 py-2"
                                type="email"
                                autocomplete="email"
                                placeholder="Email"
                                prop:value=move || email.get()
                                on:input=move |event| email.set(event_target_value(&event))
                            />
                        </Show>

                        <Show when=move || mode.get() == Mode::Register>
                            <input
                                class="w-full rounded border border-slate-300 px-3 py-2"
                                placeholder="Your name"
                                prop:value=move || display_name.get()
                                on:input=move |event| display_name.set(event_target_value(&event))
                            />
                        </Show>

                        <Show when=move || mode.get() != Mode::Forgot>
                            <input
                                class="w-full rounded border border-slate-300 px-3 py-2"
                                type="password"
                                autocomplete=move || if mode.get() == Mode::SignIn {
                                    "current-password"
                                } else {
                                    "new-password"
                                }
                                placeholder=move || if mode.get() == Mode::Reset {
                                    "New password"
                                } else {
                                    "Password"
                                }
                                prop:value=move || password.get()
                                on:input=move |event| password.set(event_target_value(&event))
                            />
                        </Show>
                    }
                >
                    <p class="text-sm text-slate-500">
                        "Enter the six-digit code from your authenticator, or one of your recovery codes."
                    </p>
                    <input
                        class="w-full rounded border border-slate-300 px-3 py-2 tracking-widest"
                        inputmode="numeric"
                        autocomplete="one-time-code"
                        placeholder="000000"
                        prop:value=move || code.get()
                        on:input=move |event| code.set(event_target_value(&event))
                    />
                </Show>

                <button
                    class="w-full rounded bg-slate-900 py-2 text-white disabled:opacity-50"
                    disabled=move || busy.get()
                >
                    {move || if busy.get() { "Just a moment…" } else { "Continue" }}
                </button>

                <Show when=move || mode.get() == Mode::SignIn>
                    <p class="border-t border-slate-200 pt-3 text-center text-sm text-slate-500">
                        "Or "
                        <button
                            type="button"
                            class="underline"
                            on:click=move |_| on_local_mode.run(local::start("My songs"))
                        >
                            "use it on this device without an account"
                        </button>
                        ". Everything works; you can sign in later and keep what you made."
                    </p>
                </Show>

                <div class="flex justify-between text-sm text-slate-500">
                    <Show when=move || mode.get() == Mode::SignIn>
                        <button type="button" class="underline" on:click=move |_| mode.set(Mode::Register)>
                            "Create an account"
                        </button>
                        <button type="button" class="underline" on:click=move |_| mode.set(Mode::Forgot)>
                            "Forgotten password"
                        </button>
                    </Show>

                    <Show when=move || matches!(mode.get(), Mode::Register | Mode::Forgot)>
                        <button type="button" class="underline" on:click=move |_| mode.set(Mode::SignIn)>
                            "Back to sign in"
                        </button>
                    </Show>
                </div>
            </form>
        </div>
    }
}
