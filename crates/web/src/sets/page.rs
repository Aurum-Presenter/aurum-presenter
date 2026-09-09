//! The sets list and the set editor.
//!
//! A key override here is a band decision and overrides everyone's personal key. Anything a
//! member changes for themselves stays on their own device.

use aurum_core::blobs::policy::{AUTO_PIN_DAYS, is_auto_pinned};
use aurum_core::chart::notes::Key;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use serde_json::{Map, Value, json};

use super::repository::{ITEM_TYPES, SetInput, Sets, parse_list};
use super::resolved::{ResolvedItem, use_resolved_set};
use crate::app::use_workspace;
use crate::db::live::live_query;
use crate::present::store::{Sessions, take_snapshot};
use crate::present::theme::workspace_theme;
// Aliased: `Set` is also the reactive trait that gives a signal its `set` method.
use crate::db::records::{Set as SetRecord, Song, alive};

/// The twelve keys, measured from C — the same list the chart controls offer.
fn twelve() -> Vec<Key> {
    let c = Key::parse("C").expect("C is a key");

    (0..12).map(|semitones| c.transposed(semitones)).collect()
}

fn field(name: &str, value: Value) -> Map<String, Value> {
    json!({ name: value })
        .as_object()
        .cloned()
        .unwrap_or_default()
}

fn use_sets() -> Option<Sets> {
    let context = use_workspace();

    Some(Sets::new(
        context.db.get_untracked()?,
        context.engine.get_untracked()?,
    ))
}

/// The sets list: what is coming up, what has been, and what is kept on this device.
#[component]
pub fn SetsPage() -> impl IntoView {
    let context = use_workspace();
    let navigate = StoredValue::new(use_navigate());
    let can_edit = context.can_edit();
    let db = context.db;

    let name = RwSignal::new(String::new());
    let date = RwSignal::new(String::new());

    let rows = live_query(&["sets"], move || {
        let db = db.get();

        async move {
            let Some(db) = db else {
                return Vec::new();
            };

            let mut sets: Vec<SetRecord> = alive(db.all("sets").await.unwrap_or_default());

            // Most recent date first, and a set with no date sorts to the bottom rather than
            // to the top, where it would sit in front of this Sunday.
            sets.sort_by(|left, right| {
                right
                    .scheduled_for
                    .as_deref()
                    .unwrap_or("")
                    .cmp(left.scheduled_for.as_deref().unwrap_or(""))
                    .then_with(|| left.name.cmp(&right.name))
            });

            sets
        }
    });

    let sets = Signal::derive(move || rows.get().unwrap_or_default());

    let create = move |event: web_sys::SubmitEvent| {
        event.prevent_default();

        let (Some(repository), navigate) = (use_sets(), navigate.get_value()) else {
            return;
        };

        let input = SetInput {
            name: name.get_untracked(),
            scheduled_for: Some(date.get_untracked()).filter(|value| !value.is_empty()),
            ..SetInput::default()
        };

        if input.name.trim().is_empty() {
            return;
        }

        spawn_local(async move {
            if let Ok(id) = repository.create(&input).await {
                name.set(String::new());
                date.set(String::new());
                navigate(&format!("/sets/{id}"), Default::default());
            }
        });
    };

    view! {
        <div class="mx-auto max-w-3xl p-4">
            <h2 class="mb-3 text-2xl font-semibold" data-testid="screen-title">"Sets"</h2>

            <Show when=move || can_edit>
                <form class="mb-5 flex flex-wrap gap-2" on:submit=create>
                    <input
                        class="min-w-48 flex-1 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
                        data-testid="new-set-name"
                        placeholder="New set — Sunday morning, the Anchor, …"
                        prop:value=move || name.get()
                        on:input=move |event| name.set(event_target_value(&event))
                    />
                    <input
                        class="rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
                        data-testid="new-set-date"
                        type="date"
                        prop:value=move || date.get()
                        on:input=move |event| date.set(event_target_value(&event))
                    />
                    <button class="rounded bg-slate-900 px-4 py-2 text-white dark:bg-slate-100 dark:text-slate-900">
                        "Create"
                    </button>
                </form>

                // Said before the set exists, while there is still a date box to fill in.
                <Show when=move || !name.get().trim().is_empty() && date.get().is_empty()>
                    <p class="mb-3 text-xs text-amber-700 dark:text-amber-400">
                        "Without a date this set is not kept offline automatically — you can pin \
                         it instead."
                    </p>
                </Show>
            </Show>

            <Show
                when=move || !sets.get().is_empty()
                fallback=|| view! {
                    <p class="rounded border border-dashed border-slate-300 py-10 text-center text-sm text-slate-500 dark:border-slate-700">
                        "No sets yet. Create one, or duplicate a past set once you have one."
                    </p>
                }
            >
                <ul
                    class="divide-y divide-slate-200 dark:divide-slate-800"
                    data-testid="set-list"
                >
                    <For each=move || sets.get() key=|set| set.id.clone() let:set>
                        <SetRow set can_edit />
                    </For>
                </ul>
            </Show>
        </div>
    }
}

#[component]
fn SetRow(set: SetRecord, can_edit: bool) -> impl IntoView {
    let navigate = StoredValue::new(use_navigate());
    let id = StoredValue::new(set.id.clone());
    let offline = is_auto_pinned(
        set.pinned == 1,
        set.scheduled_for.as_deref(),
        crate::now_ms(),
    );

    view! {
        <li class="flex items-baseline gap-3 py-2">
            <A href=format!("/sets/{}", set.id) attr:class="font-medium">{set.name.clone()}</A>
            {set.scheduled_for.clone().map(|when| view! {
                <span class="text-sm text-slate-500">{when}</span>
            })}
            {set.venue.clone().map(|venue| view! {
                <span class="text-sm text-slate-500">{venue}</span>
            })}

            <Show when=move || offline>
                <span
                    class="rounded-full bg-sky-100 px-2 text-xs text-sky-900"
                    data-testid="set-offline"
                    title=if set.pinned == 1 {
                        "Pinned for offline".to_owned()
                    } else {
                        format!("Within {AUTO_PIN_DAYS} days, so it is kept offline")
                    }
                >
                    "offline"
                </span>
            </Show>

            <Show when=move || can_edit>
                <span class="ml-auto flex gap-3 text-sm">
                    <button
                        class="underline"
                        on:click=move |_| {
                            let (Some(repository), navigate, id) =
                                (use_sets(), navigate.get_value(), id.get_value())
                            else {
                                return;
                            };

                            spawn_local(async move {
                                if let Ok(Some(copy)) = repository.duplicate(&id).await {
                                    navigate(&format!("/sets/{copy}"), Default::default());
                                }
                            });
                        }
                    >
                        "Duplicate"
                    </button>
                    <button
                        class="underline text-red-700 dark:text-red-400"
                        on:click=move |_| {
                            let (Some(repository), id) = (use_sets(), id.get_value()) else {
                                return;
                            };

                            spawn_local(async move {
                                let _ = repository.remove(&id).await;
                            });
                        }
                    >
                        "Delete"
                    </button>
                </span>
            </Show>
        </li>
    }
}

/// The set editor: the running order, per-item overrides, and the ways out of it.
#[component]
pub fn SetPage() -> impl IntoView {
    let context = use_workspace();
    let params = use_params_map();
    let set_id = Signal::derive(move || params.read().get("set_id").unwrap_or_default());
    let can_edit = context.can_edit();

    let resolved = use_resolved_set(set_id);
    let set = Signal::derive(move || resolved.get().set);
    let items = Signal::derive(move || resolved.get().items);

    let adding = RwSignal::new(false);
    let dragging = RwSignal::new(None::<usize>);
    let presenting = RwSignal::new(false);
    let navigate = StoredValue::new(use_navigate());

    // The snapshot is taken here, once: from this moment the session is immune to anything
    // anyone edits anywhere (acceptance criterion 2).
    let present = move || {
        let (Some(db), Some(engine)) = (context.db.get_untracked(), context.engine.get_untracked())
        else {
            return;
        };

        let (Some(set), items) = (set.get_untracked(), items.get_untracked()) else {
            return;
        };

        presenting.set(true);

        let workspace_id = context.workspace.get_untracked().id;
        let _ = &engine;

        spawn_local(async move {
            let theme = workspace_theme(&db).await;
            let sessions = Sessions::new(db);
            let snapshot = take_snapshot(Some(&set.id), &set.name, &items);

            match sessions.create(&workspace_id, snapshot, theme).await {
                Ok(session) => navigate.get_value()(
                    &format!("/present/{}", session.session_id),
                    Default::default(),
                ),
                Err(_) => presenting.set(false),
            }
        });
    };

    let change = move |changes: Map<String, Value>| {
        let (Some(repository), id) = (use_sets(), set_id.get_untracked()) else {
            return;
        };

        spawn_local(async move {
            let _ = repository.update(&id, changes).await;
        });
    };

    let drop_on = move |index: usize| {
        let (Some(from), Some(repository), id) =
            (dragging.get_untracked(), use_sets(), set_id.get_untracked())
        else {
            return;
        };

        dragging.set(None);

        spawn_local(async move {
            let _ = repository.move_item(&id, from, index).await;
        });
    };

    // Loading and deleted are the same shape of answer, so they share a branch that says which.
    let missing = Signal::derive(move || match set.get() {
        None => Some("Loading…"),
        Some(set) if set.sync.deleted_at.is_some() => Some("This set has been deleted."),
        Some(_) => None,
    });

    view! {
        <Show
            when=move || missing.get().is_none()
            fallback=move || view! {
                <p class="p-6 text-sm text-slate-500">{move || missing.get().unwrap_or_default()}</p>
            }
        >
            <div class="mx-auto max-w-4xl p-4">
                <A href="/sets" attr:class="text-sm underline">"← Sets"</A>

                <div class="mb-4 mt-2 flex flex-wrap items-center gap-3">
                    <Show
                        when=move || can_edit
                        fallback=move || view! {
                            <h2 class="text-2xl font-semibold">
                                {move || set.get().map(|set| set.name).unwrap_or_default()}
                            </h2>
                        }
                    >
                        <input
                            class="rounded border border-transparent bg-transparent text-2xl font-semibold hover:border-slate-300 dark:hover:border-slate-700"
                            data-testid="set-name"
                            prop:value=move || set.get().map(|set| set.name).unwrap_or_default()
                            on:input=move |event| {
                                change(field("name", json!(event_target_value(&event))));
                            }
                        />

                        <input
                            class="rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
                            data-testid="set-date"
                            type="date"
                            prop:value=move || {
                                set.get().and_then(|set| set.scheduled_for).unwrap_or_default()
                            }
                            on:input=move |event| {
                                let value = event_target_value(&event);

                                change(field(
                                    "scheduled_for",
                                    if value.is_empty() { Value::Null } else { json!(value) },
                                ));
                            }
                        />

                        <input
                            class="rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
                            placeholder="Venue"
                            prop:value=move || {
                                set.get().and_then(|set| set.venue).unwrap_or_default()
                            }
                            on:input=move |event| {
                                change(field("venue", json!(event_target_value(&event))));
                            }
                        />

                        <label class="flex items-center gap-1 text-sm text-slate-500">
                            <input
                                type="checkbox"
                                data-testid="set-pinned"
                                prop:checked=move || {
                                    set.get().is_some_and(|set| set.pinned == 1)
                                }
                                on:change=move |event| {
                                    change(field(
                                        "pinned",
                                        json!(i64::from(event_target_checked(&event))),
                                    ));
                                }
                            />
                            "Keep offline"
                        </label>
                    </Show>

                    <Show when=move || {
                        set.get().is_some_and(|set| {
                            set.pinned == 0
                                && is_auto_pinned(false, set.scheduled_for.as_deref(), crate::now_ms())
                        })
                    }>
                        <span class="rounded-full bg-sky-100 px-2 py-1 text-xs text-sky-900">
                            "kept offline — it is coming up"
                        </span>
                    </Show>

                    <Show when=move || !items.get().is_empty()>
                        <span class="ml-auto flex items-center gap-3 text-sm">
                            <A
                                href=move || format!("/sets/{}/read/0", set_id.get())
                                attr:class="underline"
                            >
                                "Read"
                            </A>
                            <A
                                href=move || format!("/sets/{}/print", set_id.get())
                                attr:class="underline"
                            >
                                "Print"
                            </A>

                            <button
                                class="rounded bg-slate-900 px-3 py-1 text-white dark:bg-slate-100 dark:text-slate-900"
                                data-testid="present"
                                prop:disabled=move || presenting.get()
                                on:click=move |_| present()
                            >
                                "Present"
                            </button>
                        </span>
                    </Show>
                </div>

                <Show
                    when=move || !items.get().is_empty()
                    fallback=|| view! {
                        <p class="rounded border border-dashed border-slate-300 py-10 text-center text-sm text-slate-500 dark:border-slate-700">
                            "Nothing in this set yet."
                        </p>
                    }
                >
                    <ol
                        class="divide-y divide-slate-200 dark:divide-slate-800"
                        data-testid="running-order"
                    >
                        {move || items
                            .get()
                            .into_iter()
                            .enumerate()
                            .map(|(index, resolved)| view! {
                                <li
                                    class="py-2"
                                    draggable=if can_edit { "true" } else { "false" }
                                    on:dragstart=move |_| dragging.set(Some(index))
                                    on:dragover=|event| event.prevent_default()
                                    on:drop=move |_| drop_on(index)
                                >
                                    <Item index resolved can_edit />
                                </li>
                            })
                            .collect_view()}
                    </ol>
                </Show>

                <Show when=move || can_edit>
                    <button
                        class="mt-4 rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900"
                        data-testid="add-items"
                        on:click=move |_| adding.set(true)
                    >
                        "Add items"
                    </button>
                </Show>

                <Show when=move || adding.get()>
                    <AddItems
                        set_id
                        on_close=Callback::new(move |()| adding.set(false))
                    />
                </Show>
            </div>
        </Show>
    }
}

#[component]
fn Item(index: usize, resolved: ResolvedItem, can_edit: bool) -> impl IntoView {
    let navigate = StoredValue::new(use_navigate());
    let item = StoredValue::new(resolved.item.clone());
    let id = StoredValue::new(resolved.item.id.clone());
    let song_id = resolved.song.as_ref().map(|song| song.id.clone());
    let is_song = resolved.item.song_id.is_some();

    let change = move |changes: Map<String, Value>| {
        let (Some(repository), id) = (use_sets(), item.get_value().id) else {
            return;
        };

        spawn_local(async move {
            let _ = repository.update_item(&id, changes).await;
        });
    };

    let from_set = resolved.source == aurum_core::chart::effective_key::KeySource::Set;
    let note = StoredValue::new(resolved.item.note.clone());
    let content = resolved.item.content.clone();

    view! {
        <div class="flex flex-wrap items-baseline gap-2">
            <span class="w-6 text-sm text-slate-400">{index + 1}</span>

            {match song_id {
                Some(song_id) => view! {
                    <button
                        class="font-medium underline-offset-2 hover:underline"
                        on:click=move |_| {
                            navigate.get_value()(&format!("/song/{song_id}"), Default::default());
                        }
                    >
                        {resolved.title.clone()}
                    </button>
                }
                .into_any(),

                None => view! {
                    <span class=if resolved.missing {
                        "font-medium text-amber-700 dark:text-amber-400"
                    } else {
                        "font-medium"
                    }>
                        {resolved.title.clone()}
                        <Show when=move || resolved.missing>
                            <span class="ml-2 text-xs">"missing song"</span>
                        </Show>
                    </span>
                }
                .into_any(),
            }}

            {resolved.item.item_type.clone().map(|kind| view! {
                <span class="rounded bg-slate-100 px-2 text-xs text-slate-600 dark:bg-slate-800 dark:text-slate-300">
                    {kind}
                </span>
            })}

            {resolved.key.map(|key| view! {
                <span
                    class="text-sm text-slate-500"
                    data-testid="item-key"
                    title=format!("Key from the {}", resolved.source.as_str())
                >
                    {key.to_string()}
                    <Show when=move || from_set>
                        <span class="ml-1 text-xs">"· set key"</span>
                    </Show>
                </span>
            })}

            <Show when=move || can_edit>
                <span class="ml-auto flex flex-wrap items-center gap-2 text-sm">
                    <Show when=move || is_song>
                        <select
                            class="rounded border border-slate-300 bg-transparent px-1 dark:border-slate-700"
                            data-testid="key-override"
                            title="A key for the whole band, for this set only"
                            prop:value=move || {
                                item.get_value().key_override.unwrap_or_default()
                            }
                            on:change=move |event| {
                                let value = event_target_value(&event);

                                change(field(
                                    "key_override",
                                    if value.is_empty() { Value::Null } else { json!(value) },
                                ));
                            }
                        >
                            <option value="">"each member's key"</option>
                            {twelve()
                                .into_iter()
                                .map(|key| view! {
                                    <option value=key.to_string()>{key.to_string()}</option>
                                })
                                .collect_view()}
                        </select>

                        <select
                            class="rounded border border-slate-300 bg-transparent px-1 dark:border-slate-700"
                            title="Capo for this set"
                            prop:value=move || {
                                item.get_value()
                                    .capo_override
                                    .map(|fret| fret.to_string())
                                    .unwrap_or_default()
                            }
                            on:change=move |event| {
                                change(field(
                                    "capo_override",
                                    json!(event_target_value(&event).parse::<i64>().ok()),
                                ));
                            }
                        >
                            <option value="">"capo: each member"</option>
                            {(0..12)
                                .map(|fret| view! {
                                    <option value=fret.to_string()>{format!("capo {fret}")}</option>
                                })
                                .collect_view()}
                        </select>
                    </Show>

                    <input
                        class="w-40 rounded border border-slate-300 px-2 py-1 text-xs dark:border-slate-700 dark:bg-slate-900"
                        placeholder="Note — start at chorus…"
                        value=note.get_value().unwrap_or_default()
                        on:blur=move |event| {
                            let value = event_target_value(&event);

                            change(field(
                                "note",
                                if value.is_empty() { Value::Null } else { json!(value) },
                            ));
                        }
                    />

                    <button
                        class="text-red-700 underline dark:text-red-400"
                        on:click=move |_| {
                            let Some(repository) = use_sets() else {
                                return;
                            };

                            let id = id.get_value();

                            spawn_local(async move {
                                let _ = repository.remove_item(&id).await;
                            });
                        }
                    >
                        "Remove"
                    </button>
                </span>
            </Show>

            {(!can_edit).then(|| note.get_value()).flatten().map(|note| view! {
                <span class="text-xs text-slate-500">{note}</span>
            })}

            {content.map(|content| view! {
                <p class="basis-full pl-6 text-sm text-slate-500">{content}</p>
            })}
        </div>
    }
}

/// Adding to a set: songs picked from the library in the order they were selected, or one of the
/// non-song items that still take a place in the running order.
#[component]
fn AddItems(set_id: Signal<String>, on_close: Callback<()>) -> impl IntoView {
    let db = use_workspace().db;
    let query = RwSignal::new(String::new());
    let picked = RwSignal::new(Vec::<Song>::new());
    let kind = RwSignal::new(ITEM_TYPES[0].0.to_owned());
    let content = RwSignal::new(String::new());

    let songs = live_query(&["songs"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => alive(db.all::<Song>("songs").await.unwrap_or_default())
                    .into_iter()
                    .filter(|song| song.archived == 0)
                    .collect(),
                None => Vec::new(),
            }
        }
    });

    let matches = Signal::derive(move || {
        let text = query.get().trim().to_lowercase();
        let mut found: Vec<Song> = songs
            .get()
            .unwrap_or_default()
            .into_iter()
            .filter(|song| {
                text.is_empty()
                    || song.title.to_lowercase().contains(&text)
                    || song
                        .artist
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&text)
            })
            .collect();

        found.sort_by(|left, right| left.title.cmp(&right.title));
        found.truncate(60);

        found
    });

    // Selection order is the order they land in, which is what a person building a set expects.
    let toggle = move |song: Song| {
        picked.update(
            |chosen| match chosen.iter().position(|held| held.id == song.id) {
                Some(at) => {
                    chosen.remove(at);
                }
                None => chosen.push(song),
            },
        );
    };

    let add_songs = move |_| {
        let (Some(repository), id, chosen) =
            (use_sets(), set_id.get_untracked(), picked.get_untracked())
        else {
            return;
        };

        spawn_local(async move {
            let _ = repository.add_songs(&id, &chosen).await;

            on_close.run(());
        });
    };

    let add_item = move |_| {
        let (Some(repository), id) = (use_sets(), set_id.get_untracked()) else {
            return;
        };

        let (kind, text) = (kind.get_untracked(), content.get_untracked());

        content.set(String::new());

        spawn_local(async move {
            let _ = repository.add_item(&id, &kind, &text).await;
        });
    };

    view! {
        <div
            class="fixed inset-0 z-20 flex items-center justify-center bg-slate-900/50 p-6"
            on:click=move |_| on_close.run(())
        >
            <div
                class="flex max-h-[80vh] w-[36rem] flex-col rounded bg-white p-4 shadow-lg dark:bg-slate-900"
                data-testid="add-items-dialog"
                on:click=|event| event.stop_propagation()
            >
                <h2 class="mb-2 font-semibold">"Add to the set"</h2>

                <input
                    class="mb-2 rounded border border-slate-300 px-3 py-2 dark:border-slate-700 dark:bg-slate-950"
                    placeholder="Search the library…"
                    prop:value=move || query.get()
                    on:input=move |event| query.set(event_target_value(&event))
                />

                <ul class="mb-3 min-h-24 flex-1 overflow-auto rounded border border-slate-200 dark:border-slate-800">
                    {move || matches
                        .get()
                        .into_iter()
                        .map(|song| {
                            let id = song.id.clone();
                            let order = Signal::derive({
                                let id = id.clone();

                                move || picked.get().iter().position(|held| held.id == id)
                            });

                            view! {
                                <li>
                                    <button
                                        class=move || if order.get().is_some() {
                                            "flex w-full items-baseline gap-2 px-2 py-1 text-left text-sm bg-sky-50 dark:bg-sky-950"
                                        } else {
                                            "flex w-full items-baseline gap-2 px-2 py-1 text-left text-sm"
                                        }
                                        on:click={
                                            let song = song.clone();

                                            move |_| toggle(song.clone())
                                        }
                                    >
                                        <span class="w-5 text-xs text-slate-400">
                                            {move || order
                                                .get()
                                                .map(|at| (at + 1).to_string())
                                                .unwrap_or_default()}
                                        </span>
                                        <span>{song.title.clone()}</span>
                                        {song.artist.clone().map(|artist| view! {
                                            <span class="text-slate-500">{artist}</span>
                                        })}
                                        {song.original_key.clone().map(|key| view! {
                                            <span class="ml-auto text-slate-500">{key}</span>
                                        })}
                                    </button>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>

                <div class="mb-3 flex flex-wrap items-center gap-2 text-sm">
                    <select
                        class="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
                        prop:value=move || kind.get()
                        on:change=move |event| kind.set(event_target_value(&event))
                    >
                        {ITEM_TYPES
                            .into_iter()
                            .map(|(value, label)| view! { <option value=value>{label}</option> })
                            .collect_view()}
                    </select>

                    <input
                        class="flex-1 rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-950"
                        placeholder="Its text — read Psalm 121, roll the video…"
                        prop:value=move || content.get()
                        on:input=move |event| content.set(event_target_value(&event))
                    />

                    <button
                        class="rounded border border-slate-300 px-3 py-1 dark:border-slate-700"
                        on:click=add_item
                    >
                        "Add item"
                    </button>
                </div>

                <div class="flex gap-2">
                    <button
                        class="rounded bg-slate-900 px-4 py-2 text-sm text-white disabled:opacity-40 dark:bg-slate-100 dark:text-slate-900"
                        prop:disabled=move || picked.get().is_empty()
                        on:click=add_songs
                    >
                        {move || match picked.get().len() {
                            0 => "Add songs".to_owned(),
                            1 => "Add 1 song".to_owned(),
                            many => format!("Add {many} songs"),
                        }}
                    </button>
                    <button class="text-sm underline" on:click=move |_| on_close.run(())>
                        "Close"
                    </button>
                </div>
            </div>
        </div>
    }
}

/// The members named on a set. Display only — being named on a set grants nothing.
pub fn members_of(set: &SetRecord) -> Vec<String> {
    parse_list(set.assigned_members.as_deref())
}
