//! What this device is keeping offline, and how much room it has left.
//!
//! The honest version: which sets are pinned and why, how much the files come to, whether the
//! browser has agreed to keep them, and one button to fetch everything rather than waiting for
//! the pin policy to get round to it.

use std::collections::BTreeSet;

use aurum_core::blobs::policy::is_auto_pinned;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::app::use_workspace;
use crate::blobs::store::{BlobStore, estimate, request_persistence};
use crate::blobs::wanted::{Kind, Released, keep_again, released};
use crate::blobs::{BlobQueue, PinReason};
use crate::db::live::live_query;
use crate::db::records::{BlobRecord, Set as SetRecord, Sheet, Song, alive};

fn megabytes(bytes: i64) -> String {
    if bytes < 1024 * 1024 {
        return format!("{} KB", (bytes as f64 / 1024.0).round() as i64);
    }

    format!(
        "{} MB",
        (bytes as f64 / 1024.0 / 1024.0 * 10.0).round() / 10.0
    )
}

/// Whether the app itself can be opened with no network — which a private window cannot do,
/// because it registers no service worker (PWA acceptance criterion 8).
pub async fn offline_ready() -> bool {
    let Some(container) = web_sys::window().map(|window| window.navigator().service_worker())
    else {
        return false;
    };

    wasm_bindgen_futures::JsFuture::from(container.get_registration())
        .await
        .map(|registration| !registration.is_undefined() && !registration.is_null())
        .unwrap_or(false)
}

/// Whether the browser has agreed to keep this origin's data. `None` where it will not say.
pub async fn persisted_now() -> Option<bool> {
    use wasm_bindgen::{JsCast, JsValue};

    let storage = web_sys::window()?.navigator().storage();
    let method = js_sys::Reflect::get(storage.as_ref(), &JsValue::from_str("persisted"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;

    let promise = js_sys::Reflect::apply(&method, storage.as_ref(), &js_sys::Array::new())
        .ok()?
        .dyn_into::<js_sys::Promise>()
        .ok()?;

    Some(
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .ok()?
            .is_truthy(),
    )
}

#[component]
pub fn StoragePage() -> impl IntoView {
    let context = use_workspace();
    let workspace = context.workspace;
    let me = context.me;
    let db = context.db;
    let api = StoredValue::new(context.api.clone());

    let workspace_id = Signal::derive(move || workspace.get().id);

    let held = RwSignal::new(Released::default());
    let space = RwSignal::new(None::<(i64, i64)>);
    let persisted = RwSignal::new(None::<bool>);
    let ready = RwSignal::new(None::<bool>);
    let fetching = RwSignal::new(false);

    Effect::new(move |_| held.set(released(&workspace_id.get())));

    Effect::new(move |_| {
        spawn_local(async move {
            ready.set(Some(offline_ready().await));
        });
    });

    let cached = live_query(&["blobs"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => db.all::<BlobRecord>("blobs").await.unwrap_or_default(),
                None => Vec::new(),
            }
        }
    });

    let sheets = live_query(&["sheets"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => alive(db.all::<Sheet>("sheets").await.unwrap_or_default()),
                None => Vec::new(),
            }
        }
    });

    let sets = live_query(&["sets"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => alive(db.all::<SetRecord>("sets").await.unwrap_or_default()),
                None => Vec::new(),
            }
        }
    });

    let songs = live_query(&["songs"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => alive(db.all::<Song>("songs").await.unwrap_or_default()),
                None => Vec::new(),
            }
        }
    });

    let blobs = Signal::derive(move || cached.get().unwrap_or_default());
    let all_sheets = Signal::derive(move || sheets.get().unwrap_or_default());
    let all_sets = Signal::derive(move || sets.get().unwrap_or_default());
    let all_songs = Signal::derive(move || songs.get().unwrap_or_default());

    // The browser's own numbers are re-read whenever the cache changes: a download that just
    // landed should move the figure on this screen.
    Effect::new(move |_| {
        blobs.track();

        spawn_local(async move {
            space.set(estimate().await);
            persisted.set(persisted_now().await);
        });
    });

    let of_reason = move |reason: PinReason| -> Vec<BlobRecord> {
        blobs
            .get()
            .into_iter()
            .filter(|row| row.pin_reason == reason.as_str())
            .collect()
    };

    let pinned = Signal::derive(move || of_reason(PinReason::Pinned));
    let opportunistic = Signal::derive(move || of_reason(PinReason::Opportunistic));

    // A sheet with a file on the server that this device does not hold.
    let uncached = Signal::derive(move || {
        let held = blobs.get();

        all_sheets
            .get()
            .into_iter()
            .filter(|sheet| sheet.sha256.is_some())
            .filter(|sheet| !held.iter().any(|row| row.sheet_id == sheet.id))
            .collect::<Vec<Sheet>>()
    });

    let everything_bytes = Signal::derive(move || {
        uncached
            .get()
            .iter()
            .map(|sheet| sheet.size.unwrap_or(0))
            .sum()
    });

    let kept = Signal::derive(move || {
        let letting_go = held.get();
        let now = crate::now_ms();

        all_sets
            .get()
            .into_iter()
            .filter(|set| is_auto_pinned(set.pinned == 1, set.scheduled_for.as_deref(), now))
            .filter(|set| !letting_go.sets.iter().any(|id| id == &set.id))
            .collect::<Vec<SetRecord>>()
    });

    let queue = move || -> Option<BlobQueue> {
        let db = db.get()?;
        let store = BlobStore::new(db.clone(), db.workspace_id());

        let workspace_id = db.workspace_id().to_owned();

        Some(BlobQueue::new(db, store, api.get_value(), &workspace_id))
    };

    // Ask for persistence first: a pinned download that the browser may clear next week is not
    // the promise this screen makes (business rule 11).
    let refresh_pins = move || {
        spawn_local(async move {
            let (Some(db), Some(queue)) = (db.get(), queue()) else {
                return;
            };

            let user_id = me.with_untracked(|me| me.id.clone());
            let part = crate::prefs::user::read(&db, &user_id).await.part;

            persisted.set(Some(request_persistence().await));

            let letting_go = released(&workspace_id.get_untracked());
            let wanted = crate::blobs::wanted::compute(&db, &user_id, part, &letting_go).await;

            queue.run(&wanted).await;
        });
    };

    let download_everything = move |_| {
        if fetching.get_untracked() {
            return;
        }

        fetching.set(true);

        spawn_local(async move {
            if let Some(queue) = queue() {
                // Every sheet in the workspace, not just the pinned ones — asked for
                // deliberately, with the size shown first.
                let wanted: BTreeSet<String> = all_sheets
                    .get_untracked()
                    .into_iter()
                    .filter(|sheet| sheet.sha256.is_some())
                    .map(|sheet| sheet.id)
                    .collect();

                queue.run(&wanted).await;
            }

            fetching.set(false);
        });
    };

    let clear_opportunistic = move |_| {
        spawn_local(async move {
            if let Some(db) = db.get() {
                let store = BlobStore::new(db.clone(), db.workspace_id());
                let _ = store.evict(0).await;
            }
        });
    };

    view! {
        <div class="mx-auto max-w-3xl p-4">
            <A href="/library" attr:class="text-sm underline">"← Library"</A>
            <h2 class="mb-1 mt-3 text-2xl font-semibold" data-testid="screen-title">
                "Offline storage"
            </h2>
            <p class="mb-4 text-sm text-slate-500">
                "Songs, charts and sets are always kept on this device — they are text, and small. \
                 Sheet PDFs are kept when they are pinned or coming up."
            </p>

            <dl class="mb-6 space-y-1 text-sm" data-testid="storage-summary">
                <Row label="Workspace" value=Signal::derive(move || workspace.get().name) />
                <Row
                    label="Pinned files"
                    value=Signal::derive(move || {
                        let rows = pinned.get();
                        let bytes = rows.iter().map(|row| row.size).sum();

                        format!("{} · {}", rows.len(), megabytes(bytes))
                    })
                />
                <Row
                    label="Opened recently"
                    value=Signal::derive(move || {
                        let rows = opportunistic.get();
                        let bytes = rows.iter().map(|row| row.size).sum();

                        format!("{} · {}", rows.len(), megabytes(bytes))
                    })
                />
                <Row
                    label="Not downloaded"
                    value=Signal::derive(move || {
                        format!("{} · {}", uncached.get().len(), megabytes(everything_bytes.get()))
                    })
                />
                <Show when=move || space.get().is_some()>
                    <Row
                        label="Browser storage"
                        value=Signal::derive(move || match space.get() {
                            Some((usage, quota)) => {
                                format!("{} of {} used", megabytes(usage), megabytes(quota))
                            }
                            None => String::new(),
                        })
                    />
                </Show>
                <Row
                    label="Offline use"
                    value=Signal::derive(move || {
                        match ready.get() {
                            Some(false) => "unavailable in this window — a private window cannot \
                                            keep the app itself offline"
                                .to_owned(),
                            Some(true) => "ready — this device can open the app with no network"
                                .to_owned(),
                            None => "checking…".to_owned(),
                        }
                    })
                />
                <Row
                    label="Kept under pressure"
                    value=Signal::derive(move || {
                        if persisted.get() == Some(true) {
                            "yes — the browser has agreed to keep this data".to_owned()
                        } else {
                            "not yet. On iOS an origin that is never opened can be cleared after \
                             a week."
                                .to_owned()
                        }
                    })
                />
            </dl>

            <div class="mb-6 flex flex-wrap gap-3">
                <button
                    class="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900"
                    data-testid="download-pinned"
                    on:click=move |_| refresh_pins()
                >
                    "Download what is pinned"
                </button>
                <button
                    class="rounded border border-slate-300 px-4 py-2 text-sm dark:border-slate-700"
                    data-testid="download-everything"
                    disabled=move || fetching.get() || uncached.get().is_empty()
                    on:click=download_everything
                >
                    {move || {
                        if fetching.get() {
                            "Downloading…".to_owned()
                        } else {
                            format!("Download everything ({})", megabytes(everything_bytes.get()))
                        }
                    }}
                </button>
                <button class="text-sm underline" on:click=clear_opportunistic>
                    "Clear files that were only opened"
                </button>
            </div>

            <h3 class="mb-2 font-semibold">"Sets kept offline"</h3>
            <ul class="space-y-1 text-sm" data-testid="kept-sets">
                <For each=move || kept.get() key=|set| set.id.clone() let:set>
                    <li class="flex gap-2">
                        <A href=format!("/sets/{}", set.id) attr:class="underline">
                            {set.name.clone()}
                        </A>
                        <span class="text-slate-500">
                            {if set.pinned == 1 {
                                "pinned".to_owned()
                            } else {
                                format!(
                                    "coming up — {}",
                                    set.scheduled_for.clone().unwrap_or_default(),
                                )
                            }}
                        </span>
                    </li>
                </For>
                <Show when=move || kept.get().is_empty()>
                    <li class="text-slate-500">
                        "Nothing is pinned and no set is within the next fortnight."
                    </li>
                </Show>
            </ul>

            <Show when=move || {
                let letting_go = held.get();

                !letting_go.sets.is_empty() || !letting_go.songs.is_empty()
            }>
                <h3 class="mb-2 mt-6 font-semibold">"Released on this device"</h3>
                <p class="mb-2 text-sm text-slate-500">
                    "Still pinned for everybody else — this device was simply out of room."
                </p>
                <ul class="space-y-1 text-sm" data-testid="released">
                    <For
                        each=move || {
                            let letting_go = held.get();
                            let sets = all_sets.get();
                            let songs = all_songs.get();

                            letting_go
                                .sets
                                .iter()
                                .map(|id| {
                                    let name = sets
                                        .iter()
                                        .find(|set| &set.id == id)
                                        .map(|set| set.name.clone())
                                        .unwrap_or_else(|| "A set".to_owned());

                                    (Kind::Set, id.clone(), name)
                                })
                                .chain(
                                    letting_go
                                        .songs
                                        .iter()
                                        .map(|id| {
                                            let name = songs
                                                .iter()
                                                .find(|song| &song.id == id)
                                                .map(|song| song.title.clone())
                                                .unwrap_or_else(|| "A song".to_owned());

                                            (Kind::Song, id.clone(), name)
                                        }),
                                )
                                .collect::<Vec<(Kind, String, String)>>()
                        }
                        key=|(kind, id, _)| format!("{}-{id}", label_of(*kind))
                        let:item
                    >
                        <Freed
                            kind=item.0
                            id=item.1.clone()
                            name=item.2.clone()
                            workspace_id=workspace_id
                            held=held
                            on_keep=Callback::new(move |()| refresh_pins())
                        />
                    </For>
                </ul>
            </Show>
        </div>
    }
}

fn label_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Set => "set",
        Kind::Song => "song",
    }
}

/// One row of the released list. Its own component so the click handler stays `Fn`.
#[component]
fn Freed(
    kind: Kind,
    id: String,
    name: String,
    workspace_id: Signal<String>,
    held: RwSignal<Released>,
    on_keep: Callback<()>,
) -> impl IntoView {
    let id = StoredValue::new(id);

    view! {
        <li class="flex gap-2">
            <span>{name}</span>
            <span class="text-xs text-slate-500">{label_of(kind)}</span>
            <button
                class="underline"
                on:click=move |_| {
                    let workspace = workspace_id.get_untracked();

                    keep_again(&workspace, kind, &id.get_value());
                    held.set(released(&workspace));
                    on_keep.run(());
                }
            >
                "Keep it again"
            </button>
        </li>
    }
}

#[component]
fn Row(label: &'static str, value: Signal<String>) -> impl IntoView {
    view! {
        <div class="flex gap-2">
            <dt class="w-44 text-slate-500">{label}</dt>
            <dd>{move || value.get()}</dd>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_files_are_shown_in_kilobytes() {
        assert_eq!(megabytes(0), "0 KB");
        assert_eq!(megabytes(1536), "2 KB");
    }

    #[test]
    fn bigger_files_are_shown_in_megabytes_to_one_decimal() {
        assert_eq!(megabytes(1024 * 1024), "1 MB");
        assert_eq!(megabytes(1024 * 1024 * 3 / 2), "1.5 MB");
    }
}
