//! The app frame: which workspace, what state the sync is in, and a way out.
//!
//! The sync chip is the only place the network is ever mentioned. Nothing else in the app waits
//! for it, so nothing else needs to talk about it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{A, Outlet};

use super::use_workspace;

#[component]
pub fn Shell(on_sign_out: Callback<()>) -> impl IntoView {
    let context = use_workspace();
    let workspace = context.workspace;
    let me = context.me;
    let online = context.online;
    let pending = context.pending;
    let local = context.local;

    // One pass now, and one every thirty seconds after — with the device-wide lock inside the
    // engine making sure three open windows do one device's work rather than three copies of it.
    {
        let context = context.clone();

        Effect::new(move |_| {
            let Some(engine) = context.engine.get() else {
                return;
            };

            spawn_local(async move {
                loop {
                    let _ = engine.sync().await;

                    // The count is read after every pass, so the chip tells the truth about
                    // what is still waiting rather than about what the last pass started with.
                    if pending.try_set(engine.pending_count().await).is_none() {
                        return;
                    }

                    gloo_timers::future::TimeoutFuture::new(TICK_MS).await;
                }
            });
        });
    }

    watch_connectivity(online, context.clone());

    let switching = context.clone();

    view! {
        <div class="min-h-dvh bg-white text-slate-900 dark:bg-slate-950 dark:text-slate-100">
            <header class="flex flex-wrap items-center gap-3 border-b border-slate-200 px-4 py-3 dark:border-slate-800">
                <A href="/library" attr:class="text-lg font-semibold">"Aurum"</A>

                <nav class="flex gap-3 text-sm">
                    <A href="/library" attr:class="text-slate-500">"Library"</A>
                    <A href="/sets" attr:class="text-slate-500">"Sets"</A>
                    <A href="/join" attr:class="text-slate-500">"Join session"</A>
                </nav>

                <select
                    class="rounded border border-slate-300 bg-transparent px-2 py-1 text-sm dark:border-slate-700"
                    data-testid="workspace-picker"
                    prop:value=move || workspace.get().id
                    on:change=move |event| switching.set_workspace(&event_target_value(&event))
                >
                    <For
                        each=move || me.get().workspaces
                        key=|option| option.id.clone()
                        let:option
                    >
                        <option value=option.id.clone()>
                            {format!("{} · {}", option.name, option.role)}
                        </option>
                    </For>
                </select>

                <button
                    class=move || format!(
                        "ml-auto rounded-full px-3 py-1 text-xs font-medium {}",
                        if online.get() {
                            "bg-emerald-100 text-emerald-900"
                        } else {
                            "bg-amber-100 text-amber-900"
                        },
                    )
                    data-testid="sync-chip"
                    title="Nothing is blocked while offline; the outbox drains when a connection returns."
                >
                    {move || {
                        let state = if online.get() { "synced" } else { "offline" };
                        let waiting = pending.get();

                        if waiting > 0 {
                            format!("{state} · {waiting} pending")
                        } else {
                            state.to_owned()
                        }
                    }}
                </button>

                <A href="/settings/account" attr:class="text-sm underline">"Account"</A>
                <button class="text-sm underline" on:click=move |_| on_sign_out.run(())>
                    {if local { "Sign in" } else { "Sign out" }}
                </button>
            </header>

            <main class="px-4 py-4">
                <Outlet />
            </main>
        </div>
    }
}

/// How often a window looks for work. The lock inside the engine decides which window does it.
const TICK_MS: u32 = 30_000;

/// Keeps the chip honest about the radio, and pushes as soon as a connection comes back rather
/// than waiting out the rest of the tick.
fn watch_connectivity(online: RwSignal<bool>, context: super::WorkspaceContext) {
    use wasm_bindgen::prelude::*;

    let Some(window) = web_sys::window() else {
        return;
    };

    for (event, state) in [("online", true), ("offline", false)] {
        let context = context.clone();

        let listener = Closure::<dyn Fn()>::new(move || {
            online.set(state);

            if !state {
                return;
            }

            let Some(engine) = context.engine.get_untracked() else {
                return;
            };

            spawn_local(async move {
                let _ = engine.sync().await;
            });
        });

        let _ = window.add_event_listener_with_callback(event, listener.as_ref().unchecked_ref());

        listener.forget();
    }
}
