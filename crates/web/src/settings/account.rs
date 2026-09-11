//! The account: who you are, and the second factor.
//!
//! An owner of a band workspace must hold a second factor, so this page is where that invariant
//! is satisfied — and where it refuses to be undone by anyone still holding an ownership.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::workspace::Enrolment;
use crate::app::use_workspace;

#[component]
pub fn AccountPage() -> impl IntoView {
    let context = use_workspace();
    let api = context.api.clone();
    let me = context.me;

    let enrolled = RwSignal::new(me.get_untracked().totp.enrolled);
    let remaining = RwSignal::new(me.get_untracked().totp.recovery_codes_remaining);
    let enrolling = RwSignal::new(None::<Enrolment>);
    let code = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let codes = RwSignal::new(None::<Vec<String>>);
    let problem = RwSignal::new(None::<String>);
    let note = RwSignal::new(None::<String>);

    let begin = {
        let api = api.clone();

        move |_| {
            let api = api.clone();

            problem.set(None);

            spawn_local(async move {
                match api.enrol_totp().await {
                    Ok(started) => enrolling.set(Some(started)),
                    Err(why) => problem.set(Some(why.to_string())),
                }
            });
        }
    };

    let confirm = {
        let api = api.clone();

        move |_| {
            let (api, typed) = (api.clone(), code.get_untracked());

            problem.set(None);

            spawn_local(async move {
                match api.confirm_totp(&typed).await {
                    Ok(answer) => {
                        remaining.set(answer.recovery_codes.len() as i64);
                        codes.set(Some(answer.recovery_codes));
                        enrolled.set(true);
                        enrolling.set(None);
                        code.set(String::new());
                    }
                    Err(why) => problem.set(Some(why.to_string())),
                }
            });
        }
    };

    let regenerate = {
        let api = api.clone();

        move |_| {
            let (api, typed, secret) =
                (api.clone(), code.get_untracked(), password.get_untracked());

            problem.set(None);

            spawn_local(async move {
                match api.new_recovery_codes(&secret, &typed).await {
                    Ok(answer) => {
                        remaining.set(answer.recovery_codes.len() as i64);
                        codes.set(Some(answer.recovery_codes));
                        code.set(String::new());
                        password.set(String::new());
                    }
                    Err(why) => problem.set(Some(why.to_string())),
                }
            });
        }
    };

    let disable = move |_| {
        let (api, typed, secret) = (api.clone(), code.get_untracked(), password.get_untracked());

        problem.set(None);

        spawn_local(async move {
            match api.disable_totp(&secret, &typed).await {
                Ok(()) => {
                    enrolled.set(false);
                    code.set(String::new());
                    password.set(String::new());
                    note.set(Some("Two-factor authentication is off.".to_owned()));
                }
                Err(why) => problem.set(Some(why.to_string())),
            }
        });
    };

    view! {
        <div class="mx-auto max-w-2xl p-4">
            <A href="/library" attr:class="text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline">"← Library"</A>
            <h2 class="mb-1 mt-3 text-2xl font-semibold" data-testid="screen-title">"Account"</h2>
            <p class="mb-4 text-sm text-ink-3">
                {move || {
                    let me = me.get();

                    format!("{} · {}", me.display_name, me.email)
                }}
            </p>

            <Show when=move || problem.get().is_some()>
                <p
                    class="mb-3 rounded-md border border-live/50 bg-live/10 p-2 text-sm text-live-ink"
                    data-testid="account-problem"
                >
                    {move || problem.get()}
                </p>
            </Show>

            <Show when=move || note.get().is_some()>
                <p class="mb-3 rounded-md border border-accent/60 bg-accent/10 p-2 text-sm text-accent">
                    {move || note.get()}
                </p>
            </Show>

            <section class="mb-6">
                <h3 class="mb-1 font-semibold">"Two-factor authentication"</h3>

                <Show when=move || { me.get().totp.required && !enrolled.get() }>
                    <p class="mb-2 rounded-md border border-warn/50 bg-warn/10 p-2 text-sm text-warn">
                        "You own a band workspace, so this account needs a second factor."
                    </p>
                </Show>

                {move || match (enrolled.get(), enrolling.get()) {
                    (true, _) => view! {
                        <div class="text-sm">
                            <p class="mb-2 text-ink-3">
                                {move || format!(
                                    "Enabled. {} recovery code(s) left.",
                                    remaining.get(),
                                )}
                            </p>

                            <div class="mb-2 flex flex-wrap gap-2">
                                <input
                                    class="rounded-md border border-line-strong px-2 py-1"
                                    type="password"
                                    placeholder="your password"
                                    prop:value=move || password.get()
                                    on:input=move |event| password.set(event_target_value(&event))
                                />
                                <input
                                    class="w-28 rounded-md border border-line-strong px-2 py-1 tracking-widest"
                                    inputmode="numeric"
                                    placeholder="000000"
                                    prop:value=move || code.get()
                                    on:input=move |event| code.set(event_target_value(&event))
                                />
                                <button
                                    class="rounded-md border border-line-strong px-3"
                                    on:click=regenerate.clone()
                                >
                                    "New recovery codes"
                                </button>
                                <button
                                    class="rounded-md border border-live/50 px-3 text-live-ink"
                                    data-testid="disable-totp"
                                    on:click=disable.clone()
                                >
                                    "Turn off"
                                </button>
                            </div>

                            <p class="text-xs text-ink-3">
                                "Turning it off is refused while you own a band workspace — hand \
                                 ownership over first, or it would leave the band without one."
                            </p>
                        </div>
                    }
                    .into_any(),

                    (false, None) => view! {
                        <button
                            class="rounded-md bg-accent px-4 py-2 text-sm text-on-accent"
                            data-testid="enrol-totp"
                            on:click=begin.clone()
                        >
                            "Set up two-factor authentication"
                        </button>
                    }
                    .into_any(),

                    (false, Some(started)) => view! {
                        <div class="text-sm">
                            <p class="mb-2 text-ink-3">
                                "Add this to your authenticator, then type the six digits it shows."
                            </p>
                            <p
                                class="mb-2 break-all rounded-md bg-raised p-2 font-mono text-xs"
                                data-testid="totp-secret"
                            >
                                {started.secret.clone()}
                            </p>
                            <p class="mb-2 break-all text-xs text-ink-4">
                                {started.provisioning_uri.clone()}
                            </p>

                            <div class="flex gap-2">
                                <input
                                    class="w-28 rounded-md border border-line-strong px-2 py-1 tracking-widest"
                                    inputmode="numeric"
                                    placeholder="000000"
                                    data-testid="totp-code"
                                    prop:value=move || code.get()
                                    on:input=move |event| code.set(event_target_value(&event))
                                />
                                <button
                                    class="rounded-md bg-accent px-3 text-on-accent"
                                    data-testid="confirm-totp"
                                    on:click=confirm.clone()
                                >
                                    "Confirm"
                                </button>
                            </div>
                        </div>
                    }
                    .into_any(),
                }}
            </section>

            <Show when=move || codes.get().is_some()>
                <section
                    class="mb-6 rounded-md border border-line-strong p-3"
                    data-testid="recovery-codes"
                >
                    <h3 class="mb-1 font-semibold">"Recovery codes"</h3>
                    <p class="mb-2 text-sm text-ink-3">
                        "Shown once. Each works one time, in place of a code from your \
                         authenticator. Keep them somewhere that is not this device."
                    </p>
                    <ul class="mb-2 grid grid-cols-2 gap-1 font-mono text-sm">
                        {move || codes
                            .get()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|value| view! { <li>{value}</li> })
                            .collect_view()}
                    </ul>
                </section>
            </Show>

            <ul class="space-y-1 text-sm">
                <li><A href="/settings/members" attr:class="text-ink-3 hover:text-ink underline-offset-2 hover:underline">
                    "Members of this workspace"
                </A></li>
                <li><A href="/settings/storage" attr:class="text-ink-3 hover:text-ink underline-offset-2 hover:underline">"Offline storage"</A></li>
                <li><A href="/settings/about" attr:class="text-ink-3 hover:text-ink underline-offset-2 hover:underline">"About"</A></li>
            </ul>
        </div>
    }
}
