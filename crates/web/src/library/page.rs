//! The library: folder tree on the left, songs on the right, search over everything.
//!
//! Every list here is rendered from IndexedDB. There is no loading state for a song the device
//! already has, because there is no request to wait for.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use super::repository::{list_of, songs_in_folder};
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

    view! {
        <div class="mx-auto flex max-w-6xl gap-6">
            <aside class="hidden w-56 shrink-0 md:block">
                <FolderList folders=all_folders selected=folder_id />
                <A href="/library/trash" attr:class="mt-4 block text-xs underline text-slate-500">
                    "Trash"
                </A>
            </aside>

            <main class="min-w-0 flex-1">
                <h1 class="sr-only" data-testid="screen-title">"Library"</h1>

                <div class="mb-3 flex flex-wrap items-center gap-2">
                    <input
                        class="min-w-48 flex-1 rounded border border-slate-300 px-3 py-2 text-sm dark:border-slate-700"
                        data-testid="search"
                        placeholder="Search titles, lyrics, tags…"
                        prop:value=move || query.get()
                        on:input=move |event| query.set(event_target_value(&event))
                    />

                    <select
                        class="rounded border border-slate-300 bg-transparent px-2 py-2 text-sm dark:border-slate-700"
                        on:change=move |event| sort.set(Sort::parse(&event_target_value(&event)))
                    >
                        <option value="title">"Title"</option>
                        <option value="recent">"Recently changed"</option>
                        <option value="tempo">"Tempo"</option>
                        <option value="key">"Key"</option>
                    </select>

                    <select
                        class="rounded border border-slate-300 bg-transparent px-2 py-2 text-sm dark:border-slate-700"
                        on:change=move |event| tag.set(event_target_value(&event))
                    >
                        <option value="">"All tags"</option>
                        <For each=move || tags.get() key=|name| name.clone() let:name>
                            <option value=name.clone()>{name.clone()}</option>
                        </For>
                    </select>

                    <label class="flex items-center gap-1 text-xs text-slate-500">
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
                </Show>

                <ul class="divide-y divide-slate-200 dark:divide-slate-800" data-testid="song-list">
                    <For each=move || visible.get() key=|song| song.id.clone() let:song>
                        <li class="py-2">
                            <A href=format!("/song/{}", song.id) attr:class="block">
                                <span class="font-medium">{song.title.clone()}</span>
                                <Show when={
                                    let artist = song.artist.clone();
                                    move || artist.is_some()
                                }>
                                    <span class="ml-2 text-sm text-slate-500">
                                        {song.artist.clone()}
                                    </span>
                                </Show>
                                <Show when={
                                    let key = song.original_key.clone();
                                    move || key.is_some()
                                }>
                                    <span class="ml-2 text-xs text-slate-400">
                                        {song.original_key.clone()}
                                    </span>
                                </Show>
                            </A>
                        </li>
                    </For>
                </ul>

                <Show when=move || visible.get().is_empty()>
                    <p class="py-8 text-center text-sm text-slate-500">
                        {move || if query.get().trim().is_empty() {
                            "Nothing here yet."
                        } else {
                            "No song matches that."
                        }}
                    </p>
                </Show>
            </main>
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
                class="flex-1 rounded border border-slate-300 px-3 py-2 text-sm dark:border-slate-700"
                data-testid="new-song-title"
                placeholder="Add a song…"
                prop:value=move || title.get()
                on:input=move |event| title.set(event_target_value(&event))
            />
            <button class="rounded bg-slate-900 px-3 py-2 text-sm text-white">"Add"</button>
        </form>

        <Show when=move || problem.get().is_some()>
            <p class="mb-2 text-sm text-red-600">{move || problem.get()}</p>
        </Show>
    }
}

/// The folder tree, as a flat list with indentation — which is what a tree is once it is drawn.
#[component]
fn FolderList(folders: Signal<Vec<Folder>>, selected: Signal<Option<String>>) -> impl IntoView {
    let ordered = Signal::derive(move || flatten(&folders.get(), None, 0));

    view! {
        <nav class="space-y-1 text-sm" data-testid="folder-tree">
            <A
                href="/library"
                attr:class=move || if selected.get().is_none() {
                    "block font-medium"
                } else {
                    "block text-slate-500"
                }
            >
                "All songs"
            </A>

            <For each=move || ordered.get() key=|(folder, _)| folder.id.clone() let:entry>
                {
                    let (folder, depth) = entry;
                    let id = folder.id.clone();

                    view! {
                        <A
                            href=format!("/library/folder/{id}")
                            attr:class=move || if selected.get().as_deref() == Some(id.as_str()) {
                                "block font-medium"
                            } else {
                                "block text-slate-500"
                            }
                            attr:style=format!("padding-left: {}rem", depth as f64 * 0.75)
                        >
                            {folder.name.clone()}
                        </A>
                    }
                }
            </For>
        </nav>
    }
}

/// Depth-first, so a child is drawn under its parent rather than wherever the store returned it.
fn flatten(folders: &[Folder], parent: Option<&str>, depth: usize) -> Vec<(Folder, usize)> {
    let mut children: Vec<&Folder> = folders
        .iter()
        .filter(|folder| folder.parent_id.as_deref() == parent)
        .collect();

    children.sort_by(|a, b| a.position.cmp(&b.position).then(a.name.cmp(&b.name)));

    children
        .into_iter()
        .flat_map(|folder| {
            let mut branch = vec![(folder.clone(), depth)];
            branch.extend(flatten(folders, Some(&folder.id), depth + 1));

            branch
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, parent: Option<&str>, position: i64) -> Folder {
        Folder {
            id: id.to_owned(),
            parent_id: parent.map(str::to_owned),
            name: id.to_owned(),
            position,
            ..Folder::default()
        }
    }

    #[test]
    fn draws_a_child_under_its_parent() {
        let folders = [
            folder("hymns", None, 0),
            folder("advent", Some("hymns"), 0),
            folder("modern", None, 1),
        ];

        let drawn = flatten(&folders, None, 0);

        assert_eq!(
            drawn
                .iter()
                .map(|(folder, depth)| (folder.id.as_str(), *depth))
                .collect::<Vec<_>>(),
            [("hymns", 0), ("advent", 1), ("modern", 0)]
        );
    }

    /// Two offline devices can leave a loop in the tree; the pane must still draw.
    #[test]
    fn terminates_on_a_tree_that_loops() {
        let folders = [folder("a", Some("b"), 0), folder("b", Some("a"), 0)];

        assert!(flatten(&folders, None, 0).is_empty());
    }
}
