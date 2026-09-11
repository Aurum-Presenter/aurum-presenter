//! What this is and what version of it is running.
//!
//! The version and build time are stamped at build time so a support question can be answered
//! without guessing, and this is also the permanent home of the install action for anyone who
//! dismissed the banner.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::pwa::install::{standalone, use_install};
use crate::settings::storage::{offline_ready, persisted_now};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILT: &str = env!("AURUM_BUILD_TIME");

fn built_at() -> String {
    BUILT
        .parse::<i64>()
        .map(aurum_core::time::format)
        .unwrap_or_else(|_| "unknown".to_owned())
}

#[component]
pub fn AboutPage() -> impl IntoView {
    let install = use_install();
    let worker = RwSignal::new(None::<bool>);
    let persisted = RwSignal::new(None::<bool>);

    Effect::new(move |_| {
        spawn_local(async move {
            worker.set(Some(offline_ready().await));
            persisted.set(persisted_now().await);
        });
    });

    view! {
        <div class="mx-auto max-w-2xl p-4 text-sm">
            <A href="/library" attr:class="underline">"← Library"</A>
            <h2 class="mb-3 mt-3 text-2xl font-semibold" data-testid="screen-title">
                "About Aurum Presenter"
            </h2>

            <dl class="mb-6 space-y-1" data-testid="about">
                <Row label="Version" value=Signal::derive(|| VERSION.to_owned()) />
                <Row label="Built" value=Signal::derive(built_at) />
                <Row
                    label="Running as"
                    value=Signal::derive(|| {
                        if standalone() { "an installed app" } else { "a browser tab" }.to_owned()
                    })
                />
                <Row
                    label="Offline shell"
                    value=Signal::derive(move || {
                        match worker.get() {
                            Some(true) => "installed — the app opens with no network".to_owned(),
                            Some(false) => "not installed yet".to_owned(),
                            None => "checking…".to_owned(),
                        }
                    })
                />
                <Row
                    label="Storage kept under pressure"
                    value=Signal::derive(move || {
                        if persisted.get() == Some(true) { "yes" } else { "not granted" }.to_owned()
                    })
                />
            </dl>

            <Show when=move || !install.installed.get()>
                <div class="mb-6">
                    <button
                        class="rounded bg-slate-900 px-4 py-2 text-white disabled:opacity-40 dark:bg-slate-100 dark:text-slate-900"
                        data-testid="install-app"
                        disabled=move || !install.available.get() && !install.ios
                        on:click=move |_| install.ask()
                    >
                        "Install this app"
                    </button>

                    <Show when=move || !install.available.get() && install.ios>
                        <p class="mt-2 text-slate-500">
                            "On iOS: Share, then “Add to Home Screen”."
                        </p>
                    </Show>

                    <Show when=move || !install.available.get() && !install.ios>
                        <p class="mt-2 text-slate-500">
                            "This browser has not offered an install prompt. It may already be \
                             installed, or it may not support installing web apps."
                        </p>
                    </Show>
                </div>
            </Show>

            <p class="text-slate-500">
                "Charts, sets and the library live on this device and sync when there is a \
                 connection. Nothing here needs the internet to work — that is the point of it."
            </p>

            <ul class="mt-4 space-y-1">
                <li>
                    <A href="/settings/storage" attr:class="underline">"Offline storage"</A>
                </li>
                <li>
                    <A href="/settings/sync/conflicts" attr:class="underline">"Conflicts"</A>
                </li>
                <li>
                    <A href="/library/trash" attr:class="underline">"Trash"</A>
                </li>
            </ul>
        </div>
    }
}

#[component]
fn Row(label: &'static str, value: Signal<String>) -> impl IntoView {
    view! {
        <div class="flex gap-2">
            <dt class="w-56 text-slate-500">{label}</dt>
            <dd>{move || value.get()}</dd>
        </div>
    }
}
