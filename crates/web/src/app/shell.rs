//! The app frame: which workspace, what state the sync is in, and a way out.
//!
//! The sync chip is the only place the network is ever mentioned. Nothing else in the app waits
//! for it, so nothing else needs to talk about it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{A, Outlet};

use super::use_workspace;
use crate::blobs::pins::held;
use crate::blobs::store::BlobStore;
use crate::blobs::wanted::{Kind, compute, release, released};
use crate::blobs::{BlobQueue, StorageFull};
use crate::prefs::user;
use crate::pwa::install::InstallBanner;
use crate::pwa::update::UpdateToast;
use crate::settings::SyncPanel;

#[component]
pub fn Shell(on_sign_out: Callback<()>) -> impl IntoView {
    let context = use_workspace();
    let workspace = context.workspace;
    let me = context.me;
    let online = context.online;
    let pending = context.pending;
    let sync_open = RwSignal::new(false);
    let local = context.local;

    // A device that has run out of room, and the file that would not fit.
    let full = RwSignal::new(None::<StorageFull>);

    // One pass now, and one every thirty seconds after — with the device-wide lock inside the
    // engine making sure three open windows do one device's work rather than three copies of it.
    {
        let context = context.clone();

        Effect::new(move |_| {
            // A device working without an account has nothing to sync with. Pretending otherwise
            // would mean a failing request every thirty seconds and a chip that means nothing.
            if local {
                return;
            }

            let (Some(engine), Some(db)) = (context.engine.get(), context.db.get()) else {
                return;
            };

            let api = context.api.clone();
            let user_id = me.get_untracked().id;

            spawn_local(async move {
                let store = BlobStore::new(db.clone(), db.workspace_id());
                let workspace_id = db.workspace_id().to_owned();
                let queue = BlobQueue::new(db.clone(), store, api, &workspace_id);

                loop {
                    let _ = engine.sync().await;

                    // The count is read after every pass, so the chip tells the truth about
                    // what is still waiting rather than about what the last pass started with.
                    if pending.try_set(engine.pending_count().await).is_none() {
                        return;
                    }

                    // Files move after the metadata, and never in front of it: a large sheet
                    // must not delay a key change reaching the rest of the band.
                    let part = user::read(&db, &user_id).await.part;
                    let want = compute(&db, &user_id, part, &released(&workspace_id)).await;
                    let state = queue.run(&want).await;

                    if full.try_set(state.full).is_none() {
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
            <InstallBanner />
            <UpdateToast />

            <Show when=move || full.get().is_some()>
                <OutOfRoom
                    full=Signal::derive(move || full.get().expect("a file that would not fit"))
                    on_close=Callback::new(move |()| full.set(None))
                />
            </Show>

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
                    on:click=move |_| sync_open.set(true)
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

            <Show when=move || sync_open.get()>
                <SyncPanel on_close=Callback::new(move |()| sync_open.set(false)) />
            </Show>
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

            // Coming back on air is a reason to sync; going off air is not, and neither is
            // either one on a device with no account behind it.
            if !state || context.local {
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

/// What a full device says.
///
/// Not "storage error". It names the file that would not fit, lists what is being kept on
/// purpose with what each one costs, and lets the user release one. Nothing pinned is ever
/// thrown away to make room — that decision belongs to the person who pinned it
/// (business rule 10).
#[component]
fn OutOfRoom(full: Signal<StorageFull>, on_close: Callback<()>) -> impl IntoView {
    let context = use_workspace();
    let db = context.db;
    let me = context.me;
    let busy = RwSignal::new(false);
    let version = RwSignal::new(0_u64);

    let pins = LocalResource::new(move || {
        let (db, user_id) = (db.get(), me.get().id);
        let _ = version.get();

        async move {
            match db {
                Some(db) => held(&db, &user_id).await,
                None => Vec::new(),
            }
        }
    });

    // Releasing is about this device only: the set stays pinned for everybody else, and for this
    // user on their phone. Nothing is deleted here — the files stop being protected, the eviction
    // pass reclaims what it needs, and the download that failed is tried again.
    let reclaim = move |kind: Option<(Kind, String)>| {
        let Some(db) = db.get_untracked() else {
            return;
        };

        busy.set(true);

        spawn_local(async move {
            let store = BlobStore::new(db.clone(), db.workspace_id());

            match kind {
                Some((kind, id)) => {
                    release(db.workspace_id(), kind, &id);
                    let _ = store.evict(crate::blobs::OPPORTUNISTIC_BUDGET).await;
                }
                // "Clear files that were only opened": everything unpinned, to the last byte.
                None => {
                    let _ = store.evict(0).await;
                }
            }

            BlobQueue::clear_full();
            busy.set(false);
            version.update(|value| *value += 1);
            on_close.run(());
        });
    };

    view! {
        <div
            class="fixed inset-0 z-30 flex items-center justify-center bg-slate-900/50 p-6"
            data-testid="out-of-room"
        >
            <div class="max-h-full w-[32rem] overflow-auto rounded bg-white p-4 shadow-xl dark:bg-slate-900">
                <h2 class="mb-1 text-lg font-semibold">"This device is out of room"</h2>
                <p class="mb-3 text-sm text-slate-600 dark:text-slate-400">
                    {move || {
                        let full = full.get();

                        format!(
                            "“{}” needs {} and there is nowhere to put it. Nothing you pinned has \
                             been thrown away — releasing one below frees its files on this \
                             device only, and the download is tried again on the next pass.",
                            full.song_title,
                            megabytes(full.size.max(0) as u64),
                        )
                    }}
                </p>

                <ul class="mb-3 divide-y divide-slate-200 text-sm dark:divide-slate-800">
                    {move || {
                        let held = pins.get().unwrap_or_default();

                        if held.is_empty() {
                            return view! {
                                <li class="py-2 text-slate-500">
                                    "Nothing is pinned. The browser itself has no room left for \
                                     this origin."
                                </li>
                            }
                            .into_any();
                        }

                        held.into_iter()
                            .map(|pin| {
                                let kind = match pin.kind {
                                    aurum_core::blobs::policy::PinKind::Set => Kind::Set,
                                    aurum_core::blobs::policy::PinKind::Song => Kind::Song,
                                };
                                let id = pin.id.clone();
                                let coming_up = pin.reason
                                    == aurum_core::blobs::policy::PinReason::ComingUp;

                                view! {
                                    <li class="flex items-baseline gap-2 py-2">
                                        <span>{pin.name.clone()}</span>
                                        <span class="text-xs text-slate-500">
                                            {format!(
                                                "{} · {}",
                                                if kind == Kind::Set { "set" } else { "song" },
                                                megabytes(pin.bytes),
                                            )}
                                        </span>

                                        <span class="ml-auto flex items-center gap-2">
                                            {coming_up.then(|| view! {
                                                <span class="text-xs text-slate-500">
                                                    "kept because it is coming up"
                                                </span>
                                            })}
                                            <button
                                                class="underline"
                                                prop:disabled=move || busy.get()
                                                on:click=move |_| {
                                                    reclaim(Some((kind, id.clone())))
                                                }
                                            >
                                                "Release"
                                            </button>
                                        </span>
                                    </li>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </ul>

                <div class="flex flex-wrap items-center gap-3 text-sm">
                    <button
                        class="rounded border border-slate-300 px-3 py-1 dark:border-slate-700"
                        prop:disabled=move || busy.get()
                        on:click=move |_| reclaim(None)
                    >
                        "Clear files that were only opened"
                    </button>
                    <button class="ml-auto underline" on:click=move |_| on_close.run(())>
                        "Not now"
                    </button>
                </div>
            </div>
        </div>
    }
}

fn megabytes(bytes: u64) -> String {
    format!(
        "{} MB",
        ((bytes as f64 / 1024.0 / 1024.0).round() as u64).max(1)
    )
}
