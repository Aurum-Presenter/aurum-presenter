//! The library: folder tree on the left, songs on the right, search over everything.
//!
//! Every list here is rendered from IndexedDB. There is no loading state for a song the device
//! already has, because there is no request to wait for.

use std::collections::HashMap;

use leptos::ev::DragEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use super::folder_tree::{Counts, FolderActions, FolderTree, SONG_DRAG_TYPE};
use super::import::ImportDialog;
use super::repository::{FolderSongs, list_of, songs_in_folder};
use super::search::{indexed, use_search};
use crate::app::use_workspace;
use crate::db::live::live_query;
use crate::db::records::{Arrangement, Folder, Song, alive};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Sort {
    #[default]
    Title,
    Recent,
    Tempo,
    Key,
}

impl Sort {
    fn parse(value: &str) -> Sort {
        match value {
            "recent" => Sort::Recent,
            "tempo" => Sort::Tempo,
            "key" => Sort::Key,
            _ => Sort::Title,
        }
    }
}

#[component]
pub fn LibraryPage() -> impl IntoView {
    let context = use_workspace();
    let params = use_params_map();
    let folder_id = Signal::derive(move || params.read().get("folder_id"));

    let query = RwSignal::new(String::new());
    let sort = RwSignal::new(Sort::default());
    let tag = RwSignal::new(String::new());
    let show_archived = RwSignal::new(false);
    let importing = RwSignal::new(false);
    let key = RwSignal::new(String::new());
    let deleting = RwSignal::new(None::<Folder>);
    let problem = RwSignal::new(None::<String>);

    let db = context.db;

    let folders = live_query(&["folders"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => alive(db.all::<Folder>("folders").await.unwrap_or_default()),
                None => Vec::new(),
            }
        }
    });

    let songs = live_query(&["songs", "song_placements"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => alive(db.all::<Song>("songs").await.unwrap_or_default()),
                None => Vec::new(),
            }
        }
    });

    let placements = live_query(&["song_placements"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => db.all("song_placements").await.unwrap_or_default(),
                None => Vec::new(),
            }
        }
    });

    let arrangements = live_query(&["arrangements"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => alive(
                    db.all::<Arrangement>("arrangements")
                        .await
                        .unwrap_or_default(),
                ),
                None => Vec::new(),
            }
        }
    });

    let all_songs = Signal::derive(move || songs.get().unwrap_or_default());
    let all_placements = Signal::derive(move || placements.get().unwrap_or_default());
    let all_folders = Signal::derive(move || folders.get().unwrap_or_default());

    // The index wants the words a person would type: titles, artist, tags and the lyrics out of
    // the chart — never the chord letters.
    let corpus =
        Signal::derive(move || indexed(&all_songs.get(), &arrangements.get().unwrap_or_default()));

    let hits = use_search(corpus, query.into());

    let tags = Signal::derive(move || {
        let mut names: Vec<String> = all_songs
            .get()
            .iter()
            .flat_map(|song| list_of(song.tags.as_deref()))
            .collect();

        names.sort();
        names.dedup();
        names
    });

    let keys = Signal::derive(move || {
        let mut found: Vec<String> = all_songs
            .get()
            .iter()
            .filter_map(|song| song.original_key.clone())
            .collect();

        found.sort();
        found.dedup();
        found
    });

    // What the folder tree shows beside each name: songs filed there, archived ones excluded.
    let counts = Signal::derive(move || {
        let mut tally: Counts = HashMap::new();

        for song in all_songs.get().iter().filter(|song| song.archived == 0) {
            *tally.entry(song.folder_id.clone()).or_default() += 1;
        }

        tally
    });

    // Two songs with the same title is usually an import run twice, and saying so on the row is
    // cheaper than finding out during a set.
    let duplicates = Signal::derive(move || {
        let mut seen: HashMap<String, usize> = HashMap::new();

        for song in all_songs.get() {
            *seen.entry(song.title.trim().to_lowercase()).or_default() += 1;
        }

        seen
    });

    let visible = Signal::derive(move || {
        let songs = all_songs.get();

        let mut rows = match hits.get() {
            // Search results arrive ranked; the folder is not what is being looked at.
            Some(hits) => hits
                .iter()
                .filter_map(|hit| songs.iter().find(|song| song.id == hit.id).cloned())
                .collect::<Vec<_>>(),
            None => songs_in_folder(&songs, &all_placements.get(), folder_id.get().as_deref()),
        };

        if !show_archived.get() {
            rows.retain(|song| song.archived == 0);
        }

        let wanted = tag.get();

        if !wanted.is_empty() {
            rows.retain(|song| list_of(song.tags.as_deref()).contains(&wanted));
        }

        let in_key = key.get();

        if !in_key.is_empty() {
            rows.retain(|song| song.original_key.as_deref() == Some(in_key.as_str()));
        }

        // Re-sorting search results by title would throw the ranking away.
        if hits.get().is_some() {
            return rows;
        }

        match sort.get() {
            Sort::Recent => rows.sort_by(|a, b| b.sync.updated_at.cmp(&a.sync.updated_at)),
            Sort::Tempo => rows.sort_by_key(|song| song.tempo.unwrap_or(999)),
            Sort::Key => rows.sort_by(|a, b| {
                a.original_key
                    .as_deref()
                    .unwrap_or("zz")
                    .cmp(b.original_key.as_deref().unwrap_or("zz"))
            }),
            Sort::Title => rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        }

        rows
    });

    let can_edit = context.can_edit();
    let library = StoredValue::new(context.library());
    let navigate = StoredValue::new_local(use_navigate());

    // Every folder edit is the same shape: ask the repository, and say out loud what it refused.
    let actions = FolderActions {
        create: Callback::new(move |parent: Option<String>| {
            let Some(name) = web_sys::window()
                .and_then(|window| window.prompt_with_message("Folder name").ok().flatten())
            else {
                return;
            };

            if name.trim().is_empty() {
                return;
            }

            spawn_local(async move {
                if let Some(library) = library.get_value() {
                    let _ = library.create_folder(&name, parent.as_deref()).await;
                }
            });
        }),
        rename: Callback::new(move |(id, name): (String, String)| {
            spawn_local(async move {
                if let Some(library) = library.get_value() {
                    let _ = library.rename_folder(&id, &name).await;
                }
            });
        }),
        move_folder: Callback::new(move |(id, parent): (String, Option<String>)| {
            spawn_local(async move {
                let Some(library) = library.get_value() else {
                    return;
                };

                match library.move_folder(&id, parent.as_deref()).await {
                    Ok(()) => problem.set(None),
                    Err(refused) => problem.set(Some(refused.to_string())),
                }
            });
        }),
        move_song: Callback::new(move |(id, folder): (String, Option<String>)| {
            spawn_local(async move {
                if let Some(library) = library.get_value() {
                    let _ = library.move_song(&id, folder.as_deref()).await;
                }
            });
        }),
        delete: Callback::new(move |folder: Folder| deleting.set(Some(folder))),
    };

    let confirm_delete = move |choice: FolderSongs| {
        let Some(folder) = deleting.get_untracked() else {
            return;
        };

        deleting.set(None);

        spawn_local(async move {
            if let Some(library) = library.get_value() {
                let _ = library.delete_folder(&folder.id, choice).await;
            }

            navigate.get_value()("/library", Default::default());
        });
    };

    view! {
        <div class="mx-auto flex max-w-6xl gap-6">
            <aside class="hidden w-56 shrink-0 md:block">
                <FolderTree
                    folders=all_folders
                    selected=folder_id
                    can_edit=can_edit
                    counts=counts
                    actions=actions
                />
                <A href="/library/trash" attr:class="mt-4 block text-xs text-ink-3 underline-offset-2 hover:underline">
                    "Trash"
                </A>
            </aside>

            <main class="min-w-0 flex-1">
                <h1 class="sr-only" data-testid="screen-title">"Library"</h1>

                <div class="mb-3 flex flex-wrap items-center gap-2">
                    <label class="flex h-10 min-w-48 flex-1 items-center gap-2.5 rounded-lg border border-line-strong bg-surface px-3">
                        <svg
                            class="size-4 shrink-0 text-ink-4"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.9"
                            stroke-linecap="round"
                            aria-hidden="true"
                        >
                            <circle cx="11" cy="11" r="6.5" />
                            <path d="M16 16 L21 21" />
                        </svg>
                        <input
                            class="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-ink-4"
                            data-testid="search"
                            placeholder="Search titles, lyrics, tags…"
                            prop:value=move || query.get()
                            on:input=move |event| query.set(event_target_value(&event))
                        />
                    </label>

                    <select
                        class="h-10 rounded-lg border border-line-strong bg-transparent px-2.5 text-sm text-ink-2"
                        on:change=move |event| sort.set(Sort::parse(&event_target_value(&event)))
                    >
                        <option value="title">"Title"</option>
                        <option value="recent">"Recently changed"</option>
                        <option value="tempo">"Tempo"</option>
                        <option value="key">"Key"</option>
                    </select>

                    <select
                        class="h-10 rounded-lg border border-line-strong bg-transparent px-2.5 text-sm text-ink-2"
                        on:change=move |event| tag.set(event_target_value(&event))
                    >
                        <option value="">"All tags"</option>
                        <For each=move || tags.get() key=|name| name.clone() let:name>
                            <option value=name.clone()>{name.clone()}</option>
                        </For>
                    </select>

                    <Show when=move || !keys.get().is_empty()>
                        <select
                            class="h-10 rounded-lg border border-line-strong bg-transparent px-2.5 text-sm text-ink-2"
                            on:change=move |event| key.set(event_target_value(&event))
                        >
                            <option value="">"Any key"</option>
                            <For each=move || keys.get() key=|name| name.clone() let:name>
                                <option value=name.clone()>{name.clone()}</option>
                            </For>
                        </select>
                    </Show>

                    <label class="flex items-center gap-1.5 text-xs text-ink-3">
                        <input
                            type="checkbox"
                            prop:checked=move || show_archived.get()
                            on:change=move |event| show_archived.set(event_target_checked(&event))
                        />
                        "Archived"
                    </label>
                </div>

                <Show when=move || can_edit>
                    <AddSong folder_id />

                    <button
                        type="button"
                        class="mb-3 rounded-md border border-line-strong px-4 py-2 text-sm"
                        data-testid="open-import"
                        on:click=move |_| importing.set(true)
                    >
                        "Import"
                    </button>
                </Show>

                <div class="cap grid grid-cols-[1fr_auto] gap-4 border-b border-line px-3 pb-2 sm:grid-cols-[1fr_11rem_3.5rem_3.5rem_9rem]">
                    <span>"Song"</span>
                    <span class="hidden sm:block">"Artist"</span>
                    <span class="hidden sm:block">"Key"</span>
                    <span class="hidden sm:block">"BPM"</span>
                    <span class="hidden sm:block">"Tags"</span>
                </div>

                <ul data-testid="song-list">
                    <For each=move || visible.get() key=|song| song.id.clone() let:song>
                        {
                            let dragged = song.id.clone();
                            let title = song.title.trim().to_lowercase();
                            let archived = song.archived == 1;

                            view! {
                                <li
                                    draggable=if can_edit { "true" } else { "false" }
                                    on:dragstart=move |event: DragEvent| {
                                        if let Some(data) = event.data_transfer() {
                                            let _ = data.set_data(SONG_DRAG_TYPE, &dragged);
                                        }
                                    }
                                >
                                    <A
                                        href=format!("/song/{}", song.id)
                                        attr:class="grid grid-cols-[1fr_auto] items-center gap-4 rounded-md border-l-2 border-transparent px-3 py-2.5 hover:border-accent hover:bg-surface sm:grid-cols-[1fr_11rem_3.5rem_3.5rem_9rem]"
                                    >
                                        <span class="flex flex-wrap items-baseline gap-2">
                                            <span class=if archived {
                                                "text-ink-3"
                                            } else {
                                                "font-medium"
                                            }>{song.title.clone()}</span>

                                            <Show when=move || archived>
                                                <span class="rounded-sm border border-line-strong px-1.5 text-xs text-ink-4">
                                                    "archived"
                                                </span>
                                            </Show>

                                            <Show when=move || {
                                                duplicates.get().get(&title).copied().unwrap_or(0) > 1
                                            }>
                                                <span
                                                    class="rounded-sm border border-warn/50 px-1.5 text-xs text-warn"
                                                    title="Another song has this title"
                                                >
                                                    "possible duplicate"
                                                </span>
                                            </Show>
                                        </span>

                                        <span class="hidden truncate text-sm text-ink-3 sm:block">
                                            {song.artist.clone()}
                                        </span>
                                        <span class="hidden font-mono text-sm text-ink-2 sm:block">
                                            {song.original_key.clone()}
                                        </span>
                                        <span class="hidden font-mono text-sm text-ink-3 sm:block">
                                            {song.tempo.map(|beats| beats.to_string())}
                                        </span>

                                        <span class="flex justify-end gap-1 sm:justify-start">
                                            <For
                                                each={
                                                    let tags = list_of(song.tags.as_deref());
                                                    move || tags.clone()
                                                }
                                                key=|name| name.clone()
                                                let:name
                                            >
                                                <span class="rounded-sm bg-raised px-2 py-0.5 text-xs text-ink-3">
                                                    {name.clone()}
                                                </span>
                                            </For>
                                        </span>
                                    </A>
                                </li>
                            }
                        }
                    </For>
                </ul>

                <Show when=move || visible.get().is_empty()>
                    <p class="py-8 text-center text-sm text-ink-3">
                        {move || if query.get().trim().is_empty() {
                            "Nothing here yet."
                        } else {
                            "No song matches that."
                        }}
                    </p>

                    <Show when=move || can_edit && query.get().trim().is_empty()>
                        <p class="pb-8 text-center">
                            <button
                                class="text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                                on:click=move |_| importing.set(true)
                            >
                                "Import ChordPro or text files"
                            </button>
                        </p>
                    </Show>
                </Show>

                <Show when=move || importing.get()>
                    <ImportDialog
                        folder_id=folder_id
                        on_close=Callback::new(move |()| importing.set(false))
                    />
                </Show>
            </main>

            <Show when=move || problem.get().is_some()>
                <p
                    class="fixed inset-x-4 bottom-4 rounded-md border border-live/50 bg-live/10 p-2 text-sm text-live-ink"
                    data-testid="folder-problem"
                    on:click=move |_| problem.set(None)
                >
                    {move || problem.get()}
                </p>
            </Show>

            <Show when=move || deleting.get().is_some()>
                <DeleteFolder
                    folder=Signal::derive(move || deleting.get())
                    songs=Signal::derive(move || {
                        deleting
                            .get()
                            .and_then(|folder| counts.get().get(&Some(folder.id)).copied())
                            .unwrap_or(0)
                    })
                    on_cancel=Callback::new(move |()| deleting.set(None))
                    on_confirm=Callback::new(confirm_delete)
                />
            </Show>
        </div>
    }
}

/// Business rule 2: deleting a folder is never allowed to quietly decide what happens to the
/// songs inside it.
#[component]
fn DeleteFolder(
    folder: Signal<Option<Folder>>,
    songs: Signal<usize>,
    on_cancel: Callback<()>,
    on_confirm: Callback<FolderSongs>,
) -> impl IntoView {
    view! {
        <div
            class="fixed inset-0 z-20 flex items-center justify-center bg-black/60 p-6"
            data-testid="delete-folder"
        >
            <div class="w-96 rounded-md bg-surface p-4 shadow-lg">
                <h2 class="mb-2 font-semibold">
                    {move || {
                        let name = folder.get().map(|folder| folder.name).unwrap_or_default();

                        format!("Delete “{name}”?")
                    }}
                </h2>
                <p class="mb-4 text-sm text-ink-3">
                    {move || match songs.get() {
                        0 => "The folder is empty. Subfolders move up to its parent.".to_owned(),
                        1 => "1 song is filed here. Nothing is deleted — choose where it goes."
                            .to_owned(),
                        many => format!(
                            "{many} songs are filed here. Nothing is deleted — choose where they go.",
                        ),
                    }}
                </p>
                <div class="flex flex-wrap gap-2">
                    <button
                        class="rounded-md bg-accent px-3 py-2 text-sm text-on-accent"
                        on:click=move |_| on_confirm.run(FolderSongs::MoveToParent)
                    >
                        "Move songs to the parent folder"
                    </button>
                    <button
                        class="rounded-md border border-line-strong px-3 py-2 text-sm"
                        on:click=move |_| on_confirm.run(FolderSongs::Archive)
                    >
                        "Archive the songs"
                    </button>
                    <button class="ml-auto text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline" on:click=move |_| on_cancel.run(())>
                        "Cancel"
                    </button>
                </div>
            </div>
        </div>
    }
}

/// Adding a song is a title and nothing else: the rest is filled in on the song's own page,
/// where there is room for it.
#[component]
fn AddSong(folder_id: Signal<Option<String>>) -> impl IntoView {
    let context = use_workspace();
    let title = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);
    let navigate = leptos_router::hooks::use_navigate();

    let submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();

        let wanted = title.get_untracked();

        if wanted.trim().is_empty() {
            problem.set(Some("A title is required.".to_owned()));

            return;
        }

        let Some(library) = context.library() else {
            // The database is still opening. Saying so beats a button that quietly does nothing.
            problem.set(Some(
                "Still opening this workspace — try again in a moment.".to_owned(),
            ));

            return;
        };
        let folder = folder_id.get_untracked();
        let navigate = navigate.clone();

        spawn_local(async move {
            let input = super::SongInput {
                title: wanted,
                folder_id: folder,
                ..super::SongInput::default()
            };

            match library.create_song(&input).await {
                Ok(id) => {
                    title.set(String::new());
                    navigate(&format!("/song/{id}"), Default::default());
                }
                Err(error) => problem.set(Some(error.to_string())),
            }
        });
    };

    view! {
        <form class="mb-3 flex gap-2" on:submit=submit>
            <input
                class="flex-1 rounded-md border border-line-strong px-3 py-2 text-sm"
                data-testid="new-song-title"
                placeholder="Add a song…"
                prop:value=move || title.get()
                on:input=move |event| title.set(event_target_value(&event))
            />
            <button class="rounded-md bg-accent px-3 py-2 text-sm text-on-accent">"Add"</button>
        </form>

        <Show when=move || problem.get().is_some()>
            <p class="mb-2 text-sm text-live-ink">{move || problem.get()}</p>
        </Show>
    }
}
