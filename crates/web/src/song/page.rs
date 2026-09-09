//! A song's chart: read it in your key, or edit it.
//!
//! Everything on this page comes out of IndexedDB, so it renders with the radio off. The only
//! thing the network does here is carry the change to everyone else, later.

use aurum_core::chart::effective_key::{KeyInputs, KeySource};
use aurum_core::chart::notes::Key;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use serde_json::json;

use crate::app::use_workspace;
use crate::chart::controls::{ArrangementChoice, ChartControls};
use crate::chart::editor::{ChartEditor, SaveInput};
use crate::chart::view::ChartView;
use crate::db::live::live_query;
use crate::db::records::{Arrangement, Song, alive};
use crate::library::repository::list_of;
use crate::prefs::display::{self, Display};
use crate::prefs::song::{SongPrefs, read as read_prefs, write as write_prefs};
use crate::song::drawer::SongMetadataDrawer;

/// The twelve keys a chart can be written in, measured from C.
fn twelve() -> Vec<Key> {
    let c = Key::parse("C").expect("C is a key");

    (0..12).map(|semitones| c.transposed(semitones)).collect()
}

/// What the page is showing. A memo, not a derived signal: every re-run of this match rebuilds
/// the screen underneath it, and a chart being edited cannot survive its own autosave doing that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Loading,
    Missing,
    Deleted,
    Ready,
}

/// Which arrangement the page opens: the reader's own choice, then the song's default, then
/// whatever comes first.
///
/// One default per song is a client invariant, so a library that somehow holds two still opens
/// deterministically rather than showing whichever row the index happened to return first.
fn chosen_arrangement<'a>(
    rows: &'a [Arrangement],
    chosen: Option<&str>,
) -> Option<&'a Arrangement> {
    rows.iter()
        .find(|row| Some(row.id.as_str()) == chosen)
        .or_else(|| rows.iter().find(|row| row.is_default == 1))
        .or_else(|| rows.first())
}

/// What the page is looking at once the database has answered.
#[derive(Clone, Debug, Default, PartialEq)]
struct Loaded {
    /// `None` here means the query has not answered yet; `Some(None)` means it answered and
    /// there is no such song. Collapsing the two leaves the page saying "Loading…" forever.
    song: Option<Option<Song>>,
    arrangements: Vec<Arrangement>,
    prefs: SongPrefs,
}

#[component]
pub fn SongPage(#[prop(optional)] edit: bool) -> impl IntoView {
    let context = use_workspace();
    let params = use_params_map();
    let song_id = Signal::derive(move || params.read().get("song_id").unwrap_or_default());

    let editing = RwSignal::new(edit);
    let drawer = RwSignal::new(false);
    let respelled = RwSignal::new(false);
    let display = RwSignal::new(display::load());
    // Preferences are read back after every write: they are one row, and a re-read is cheaper
    // than keeping a second copy of them in sync with the one in the database.
    let prefs_version = RwSignal::new(0_u64);

    let db = context.db;
    let me = context.me;

    let loaded = live_query(&["songs", "arrangements", "preferences"], move || {
        let db = db.get();
        let song_id = song_id.get();
        let user_id = me.get().id;

        // Read even when nothing changed in the stores, so a preference write shows immediately.
        let _ = prefs_version.get();

        async move {
            let Some(db) = db else {
                return Loaded::default();
            };

            let song = db.get::<Song>("songs", &song_id).await.unwrap_or_default();
            let mut arrangements: Vec<Arrangement> = alive(
                db.by_index("arrangements", "song_id", &song_id.as_str().into())
                    .await
                    .unwrap_or_default(),
            );

            arrangements.sort_by(|left, right| {
                left.position
                    .cmp(&right.position)
                    .then_with(|| left.name.cmp(&right.name))
            });

            Loaded {
                song: Some(song),
                arrangements,
                prefs: read_prefs(&db, &user_id, &song_id).await,
            }
        }
    });

    let held = Signal::derive(move || loaded.get().unwrap_or_default());
    let song = Signal::derive(move || held.get().song.flatten());
    let arrangements = Signal::derive(move || held.get().arrangements);
    let prefs = Signal::derive(move || held.get().prefs);

    let arrangement = Signal::derive(move || {
        chosen_arrangement(&arrangements.get(), prefs.get().arrangement_id.as_deref()).cloned()
    });

    let written = Signal::derive(move || {
        let song = song.get()?;
        let arrangement = arrangement.get();

        KeyInputs {
            set_override: None,
            preferred: None,
            arrangement_default: arrangement
                .as_ref()
                .and_then(|row| row.default_key.as_deref()),
            song_original: song.original_key.as_deref(),
        }
        .source_key()
    });

    let target = Signal::derive(move || {
        let Some(song) = song.get() else {
            return (None, KeySource::None);
        };

        let arrangement = arrangement.get();
        let prefs = prefs.get();

        let effective = KeyInputs {
            set_override: None,
            preferred: prefs.preferred_key.as_deref(),
            arrangement_default: arrangement
                .as_ref()
                .and_then(|row| row.default_key.as_deref()),
            song_original: song.original_key.as_deref(),
        }
        .effective();

        (effective.key, effective.source)
    });

    // The reader's own capo wins; with none chosen, the arranger's suggestion stands.
    let capo = Signal::derive(move || {
        prefs
            .get()
            .capo
            .or_else(|| arrangement.get().and_then(|row| row.capo_hint))
            .unwrap_or(0)
    });

    let update = move |changes: SongPrefs| {
        let (Some(db), Some(engine)) = (db.get_untracked(), context.engine.get_untracked()) else {
            return;
        };

        let user_id = me.get_untracked().id;
        let song_id = song_id.get_untracked();

        spawn_local(async move {
            let _ = write_prefs(&db, &engine, &user_id, &song_id, &changes).await;

            prefs_version.update(|value| *value += 1);
        });
    };

    let record = move |table: &'static str, id: String, payload: serde_json::Value| {
        let Some(engine) = context.engine.get_untracked() else {
            return;
        };

        spawn_local(async move {
            let _ = engine
                .record(
                    table,
                    &id,
                    "upsert",
                    payload.as_object().cloned().unwrap_or_default(),
                )
                .await;
        });
    };

    let create_arrangement = Callback::new(move |()| {
        let id = crate::new_id();
        let existing = arrangements.get_untracked().len();
        let Some(song) = song.get_untracked() else {
            return;
        };

        record(
            "arrangements",
            id.clone(),
            json!({
                "song_id": song.id,
                "name": if existing == 0 {
                    "Default".to_owned()
                } else {
                    format!("Arrangement {}", existing + 1)
                },
                "body": "",
                "is_default": i64::from(existing == 0),
                "position": existing as i64,
                "source_notation": "chordpro",
            }),
        );

        update(SongPrefs {
            arrangement_id: Some(id),
            ..prefs.get_untracked()
        });
        editing.set(true);
    });

    let save = Callback::new(move |input: SaveInput| {
        let Some(arrangement) = arrangement.get_untracked() else {
            return;
        };

        record(
            "arrangements",
            arrangement.id,
            json!({
                "body": input.body,
                "source_notation": match input.source_notation {
                    aurum_core::chart::over_lyrics::Notation::OverLyrics => "over_lyrics",
                    _ => "chordpro",
                },
                "source_text": input.source_text,
            }),
        );
    });

    let preview = Callback::new(move |body: String| {
        let fallback = Key::parse("C").expect("C is a key");
        let source = written.get().unwrap_or(fallback);

        view! {
            <ChartView
                body=Signal::derive(move || body.clone())
                source=Signal::derive(move || source)
                target=Signal::derive(move || target.get().0.unwrap_or(source))
                capo
                display=display.into()
            />
        }
        .into_any()
    });

    let can_edit = context.can_edit();

    let phase = Memo::new(move |_| match (loaded.get().is_none(), song.get()) {
        (true, _) => Phase::Loading,
        (false, None) => Phase::Missing,
        (false, Some(song)) if song.sync.deleted_at.is_some() => Phase::Deleted,
        (false, Some(_)) => Phase::Ready,
    });

    view! {
        {move || match phase.get() {
            Phase::Loading => {
                view! { <p class="p-6 text-sm text-slate-500">"Loading…"</p> }.into_any()
            }

            Phase::Missing => view! {
                <div class="p-6 text-sm">
                    <p class="text-slate-500">
                        "That song is not in this workspace. It may belong to another one, or it \
                         may have been deleted."
                    </p>
                    <A href="/library" attr:class="underline">"Back to the library"</A>
                </div>
            }
            .into_any(),

            Phase::Deleted => view! {
                <div class="p-6 text-sm">
                    <p class="text-slate-500">"This song has been deleted."</p>
                    <A href="/library/trash" attr:class="underline">"Open Trash"</A>
                </div>
            }
            .into_any(),

            Phase::Ready => view! {
                    <div class="mx-auto max-w-5xl p-4">
                        <A href="/library" attr:class="text-sm underline">"← Library"</A>

                        <SongHeader
                            song=Signal::derive(move || song.get().unwrap_or_default())
                            can_edit
                            on_details=Callback::new(move |()| drawer.set(true))
                        />

                        <Show
                            when=move || written.get().is_some()
                            fallback=move || view! {
                                <NoKeyYet
                                    can_edit
                                    on_pick=Callback::new(move |key: Key| {
                                        record(
                                            "songs",
                                            song_id.get_untracked(),
                                            json!({ "original_key": key.to_string() }),
                                        );
                                    })
                                />
                            }
                        >
                            <ChartControls
                                arrangements=Signal::derive(move || {
                                    arrangements
                                        .get()
                                        .into_iter()
                                        .map(|row| ArrangementChoice {
                                            id: row.id,
                                            name: row.name,
                                        })
                                        .collect()
                                })
                                arrangement_id=Signal::derive(move || {
                                    arrangement.get().map(|row| row.id)
                                })
                                on_arrangement=Callback::new(move |id: String| {
                                    update(SongPrefs {
                                        arrangement_id: Some(id),
                                        ..prefs.get_untracked()
                                    });
                                })
                                target=Signal::derive(move || target.get().0)
                                source=Signal::derive(move || target.get().1)
                                original=written
                                on_key=Callback::new(move |key: Option<Key>| {
                                    update(SongPrefs {
                                        preferred_key: key.map(|key| key.to_string()),
                                        ..prefs.get_untracked()
                                    });
                                })
                                capo
                                on_capo=Callback::new(move |next: i64| {
                                    update(SongPrefs {
                                        capo: Some(next),
                                        ..prefs.get_untracked()
                                    });
                                })
                                display=display.into()
                                on_display=Callback::new(move |next: Display| {
                                    display::save(&next);
                                    display.set(next);
                                })
                                respelled=respelled.into()
                                can_edit
                                editing=editing.into()
                                on_toggle_edit=Callback::new(move |()| {
                                    editing.update(|value| *value = !*value)
                                })
                            />
                        </Show>

                        <div class="mt-4">
                            <Show
                                when=move || arrangement.get().is_some()
                                fallback=move || view! {
                                    <EmptyChart can_edit on_create=create_arrangement />
                                }
                            >
                                <Show
                                    when=move || editing.get() && can_edit
                                    fallback=move || view! {
                                        <ChartBody
                                            arrangement=Signal::derive(move || {
                                                arrangement.get().expect("an arrangement")
                                            })
                                            written
                                            target=Signal::derive(move || target.get().0)
                                            capo
                                            display=display.into()
                                            on_respelled=Callback::new(move |value: bool| {
                                                respelled.set(value)
                                            })
                                        />
                                    }
                                >
                                    <ArrangementSettings
                                        arrangement=Signal::derive(move || {
                                            arrangement.get().expect("an arrangement")
                                        })
                                        arrangements
                                        on_change=Callback::new(move |
                                            (id, changes): (String, serde_json::Value),
                                        | record("arrangements", id, changes))
                                    />

                                    <ChartEditor
                                        body=Signal::derive(move || {
                                            arrangement.get().map(|row| row.body).unwrap_or_default()
                                        })
                                        on_save=save
                                        preview
                                    />
                                </Show>
                            </Show>
                        </div>

                        <Show when=move || {
                            can_edit && !arrangements.get().is_empty() && !editing.get()
                        }>
                            <button
                                class="mt-6 text-sm underline"
                                data-testid="add-arrangement"
                                on:click=move |_| create_arrangement.run(())
                            >
                                "Add another arrangement"
                            </button>
                        </Show>

                        <Show when=move || drawer.get()>
                            <SongMetadataDrawer
                                song=song.get_untracked().unwrap_or_default()
                                on_close=Callback::new(move |()| drawer.set(false))
                            />
                        </Show>
                    </div>
            }
            .into_any(),
        }}
    }
}

#[component]
fn SongHeader(song: Signal<Song>, can_edit: bool, on_details: Callback<()>) -> impl IntoView {
    // Held in stored values so the buttons below stay `Fn`: a `<Show>` body can run more than
    // once, and a closure that moves its captures cannot.
    let library = StoredValue::new(use_workspace().library());
    let navigate = StoredValue::new(use_navigate());
    let id = Signal::derive(move || song.get().id);
    let archived = Signal::derive(move || song.get().archived == 1);

    view! {
        <div class="mb-3 mt-2 flex flex-wrap items-baseline gap-3">
            <h2 class="text-2xl font-semibold" data-testid="song-title">
                {move || song.get().title}
            </h2>
            {move || song.get().artist.map(|artist| view! {
                <span class="text-slate-500">{artist}</span>
            })}
            {move || song.get().tempo.map(|tempo| view! {
                <span class="text-sm text-slate-500">{format!("{tempo} bpm")}</span>
            })}
            {move || song.get().time_signature.map(|time| view! {
                <span class="text-sm text-slate-500">{time}</span>
            })}

            {move || list_of(song.get().tags.as_deref())
                .into_iter()
                .map(|tag| view! {
                    <span class="rounded bg-slate-100 px-2 text-xs text-slate-600 dark:bg-slate-800 dark:text-slate-300">
                        {tag}
                    </span>
                })
                .collect_view()}

            <Show when=move || can_edit>
                <span class="ml-auto flex gap-3 text-sm">
                    <button class="underline" on:click=move |_| on_details.run(())>"Details"</button>

                    <button
                        class="underline"
                        data-testid="duplicate-song"
                        on:click=move |_| {
                            let (Some(library), navigate, id) =
                                (library.get_value(), navigate.get_value(), id.get())
                            else {
                                return;
                            };

                            spawn_local(async move {
                                // A copy the reader did not ask to stay away from: opening it is
                                // the only sensible next screen.
                                if let Ok(Some(copy)) = library.duplicate_song(&id).await {
                                    navigate(&format!("/song/{copy}"), Default::default());
                                }
                            });
                        }
                    >
                        "Duplicate"
                    </button>

                    <button
                        class="underline"
                        data-testid="archive-song"
                        on:click=move |_| {
                            let (Some(library), id, archived) =
                                (library.get_value(), id.get(), archived.get())
                            else {
                                return;
                            };

                            spawn_local(async move {
                                let _ = library.set_archived(&id, !archived).await;
                            });
                        }
                    >
                        {move || if archived.get() { "Unarchive" } else { "Archive" }}
                    </button>

                    <button
                        class="underline text-red-700 dark:text-red-400"
                        data-testid="delete-song"
                        on:click=move |_| {
                            let (Some(library), navigate, id) =
                                (library.get_value(), navigate.get_value(), id.get())
                            else {
                                return;
                            };

                            spawn_local(async move {
                                let _ = library.delete_song(&id).await;

                                navigate("/library", Default::default());
                            });
                        }
                    >
                        "Delete"
                    </button>
                </span>
            </Show>
        </div>

        {move || song.get().subtitle.map(|subtitle| view! {
            <p class="mb-3 text-slate-500">{subtitle}</p>
        })}
    }
}

/// Name, written key and suggested capo — the parts of an arrangement that are not the chart.
#[component]
fn ArrangementSettings(
    arrangement: Signal<Arrangement>,
    arrangements: Signal<Vec<Arrangement>>,
    on_change: Callback<(String, serde_json::Value)>,
) -> impl IntoView {
    // Exactly one arrangement per song is the default. The old default is cleared first, in its
    // own operation, so the outbox replays the pair in an order that never shows two defaults.
    let make_default = move |_| {
        let id = arrangement.get_untracked().id;

        for row in arrangements.get_untracked() {
            if row.is_default == 1 && row.id != id {
                on_change.run((row.id, json!({ "is_default": 0 })));
            }
        }

        on_change.run((id, json!({ "is_default": 1 })));
    };

    view! {
        <div class="mb-3 flex flex-wrap items-center gap-2 text-sm">
            <input
                class="rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-900"
                data-testid="arrangement-name"
                placeholder="Arrangement name"
                prop:value=move || arrangement.get().name
                on:input=move |event| {
                    on_change.run((
                        arrangement.get_untracked().id,
                        json!({ "name": event_target_value(&event) }),
                    ));
                }
            />

            <label class="flex items-center gap-1">
                "Written in"
                <select
                    class="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
                    prop:value=move || arrangement.get().default_key.unwrap_or_default()
                    on:change=move |event| {
                        let value = event_target_value(&event);

                        on_change.run((
                            arrangement.get_untracked().id,
                            json!({
                                "default_key": if value.is_empty() { None } else { Some(value) },
                            }),
                        ));
                    }
                >
                    <option value="">"the song's key"</option>
                    {twelve()
                        .into_iter()
                        .map(|key| view! { <option value=key.to_string()>{key.to_string()}</option> })
                        .collect_view()}
                </select>
            </label>

            <label class="flex items-center gap-1">
                "Suggested capo"
                <select
                    class="rounded border border-slate-300 bg-transparent px-2 py-1 dark:border-slate-700"
                    prop:value=move || {
                        arrangement.get().capo_hint.map(|fret| fret.to_string()).unwrap_or_default()
                    }
                    on:change=move |event| {
                        let value = event_target_value(&event);

                        on_change.run((
                            arrangement.get_untracked().id,
                            json!({ "capo_hint": value.parse::<i64>().ok() }),
                        ));
                    }
                >
                    <option value="">"none"</option>
                    {(0..12)
                        .map(|fret| view! {
                            <option value=fret.to_string()>{fret.to_string()}</option>
                        })
                        .collect_view()}
                </select>
            </label>

            <Show
                when=move || arrangement.get().is_default == 1
                fallback=move || view! {
                    <button class="underline" on:click=make_default>"Make default"</button>
                }
            >
                <span class="text-slate-500">"Default arrangement"</span>
            </Show>
        </div>
    }
}

#[component]
fn ChartBody(
    arrangement: Signal<Arrangement>,
    written: Signal<Option<Key>>,
    target: Signal<Option<Key>>,
    capo: Signal<i64>,
    display: Signal<Display>,
    on_respelled: Callback<bool>,
) -> impl IntoView {
    let fallback = Key::parse("C").expect("C is a key");
    let source = Signal::derive(move || written.get().unwrap_or(fallback));

    view! {
        <Show
            when=move || !arrangement.get().body.trim().is_empty()
            fallback=|| view! {
                <p class="text-sm text-slate-500">"This arrangement has no chart yet."</p>
            }
        >
            <ChartView
                body=Signal::derive(move || arrangement.get().body)
                source
                target=Signal::derive(move || target.get().unwrap_or_else(|| source.get()))
                capo
                display
                on_respelled
            />
        </Show>
    }
}

#[component]
fn EmptyChart(can_edit: bool, on_create: Callback<()>) -> impl IntoView {
    view! {
        <div class="rounded border border-dashed border-slate-300 p-6 text-center dark:border-slate-700">
            <p class="text-sm text-slate-500">"No chart yet."</p>

            <Show when=move || can_edit>
                <button
                    class="mt-3 rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900"
                    data-testid="add-chart"
                    on:click=move |_| on_create.run(())
                >
                    "Add a chart"
                </button>
            </Show>
        </div>
    }
}

/// Transposition needs to know what the chart is written in. Until the song says, there is no
/// interval to move by — so the page asks once rather than guessing and re-lettering wrongly.
#[component]
fn NoKeyYet(can_edit: bool, on_pick: Callback<Key>) -> impl IntoView {
    view! {
        <Show
            when=move || can_edit
            fallback=|| view! {
                <p class="text-sm text-slate-500">
                    "This song has no key set, so it cannot be transposed."
                </p>
            }
        >
            <div
                class="flex flex-wrap items-center gap-2 border-b border-slate-200 pb-3 text-sm dark:border-slate-800"
                data-testid="no-key-yet"
            >
                <span class="text-slate-500">"What key is this chart written in?"</span>

                {twelve()
                    .into_iter()
                    .map(|key| view! {
                        <button
                            class="rounded border border-slate-200 px-2 py-1 dark:border-slate-700"
                            on:click=move |_| on_pick.run(key)
                        >
                            {key.to_string()}
                        </button>
                    })
                    .collect_view()}
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrangement(id: &str, is_default: i64) -> Arrangement {
        Arrangement {
            id: id.to_owned(),
            is_default,
            ..Arrangement::default()
        }
    }

    #[test]
    fn the_readers_own_choice_wins() {
        let rows = vec![arrangement("a", 1), arrangement("b", 0)];

        assert_eq!(
            chosen_arrangement(&rows, Some("b")).map(|row| row.id.as_str()),
            Some("b")
        );
    }

    #[test]
    fn a_choice_that_is_no_longer_there_falls_back_to_the_default() {
        let rows = vec![arrangement("a", 0), arrangement("b", 1)];

        assert_eq!(
            chosen_arrangement(&rows, Some("gone")).map(|row| row.id.as_str()),
            Some("b")
        );
    }

    #[test]
    fn two_defaults_still_open_the_same_one_every_time() {
        let rows = vec![arrangement("a", 1), arrangement("b", 1)];

        assert_eq!(
            chosen_arrangement(&rows, None).map(|row| row.id.as_str()),
            Some("a")
        );
    }

    #[test]
    fn no_default_opens_the_first() {
        let rows = vec![arrangement("a", 0), arrangement("b", 0)];

        assert_eq!(
            chosen_arrangement(&rows, None).map(|row| row.id.as_str()),
            Some("a")
        );
    }

    #[test]
    fn a_song_with_no_arrangements_opens_none() {
        assert!(chosen_arrangement(&[], Some("a")).is_none());
    }

    #[test]
    fn the_twelve_keys_start_at_c_and_do_not_repeat() {
        let keys: Vec<String> = twelve().iter().map(Key::to_string).collect();

        assert_eq!(keys.len(), 12);
        assert_eq!(keys[0], "C");
        assert_eq!(
            keys.iter().collect::<std::collections::HashSet<_>>().len(),
            12
        );
    }
}
