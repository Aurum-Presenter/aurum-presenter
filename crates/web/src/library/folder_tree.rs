//! The folder tree. Drag a folder to re-file it, drag a song onto a folder to move it there.
//!
//! The tree refuses a drop that would put a folder inside its own subtree — the caller checks the
//! path and says so out loud rather than silently doing nothing, because a drag that appears to
//! do nothing reads as a bug.

use std::collections::{HashMap, HashSet};

use leptos::ev::{DragEvent, KeyboardEvent};
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use crate::db::records::Folder;

pub const SONG_DRAG_TYPE: &str = "application/x-aurum-song";
const FOLDER_DRAG_TYPE: &str = "application/x-aurum-folder";

/// How a folder is counted: the songs filed directly in it, archived ones excluded.
pub type Counts = HashMap<Option<String>, usize>;

/// What a row can be told to do. One bundle rather than nine props, because every row needs all
/// of them and Leptos has no spread.
#[derive(Clone, Copy)]
pub struct FolderActions {
    pub create: Callback<Option<String>>,
    pub rename: Callback<(String, String)>,
    pub move_folder: Callback<(String, Option<String>)>,
    pub move_song: Callback<(String, Option<String>)>,
    pub delete: Callback<Folder>,
}

/// Depth-first, so a child is drawn under its parent rather than wherever the store returned it,
/// and with everything under a collapsed folder left out.
fn visible(folders: &[Folder], collapsed: &HashSet<String>) -> Vec<(Folder, usize)> {
    fn walk(
        folders: &[Folder],
        collapsed: &HashSet<String>,
        parent: Option<&str>,
        depth: usize,
        into: &mut Vec<(Folder, usize)>,
    ) {
        let mut children: Vec<&Folder> = folders
            .iter()
            .filter(|folder| folder.parent_id.as_deref() == parent)
            .collect();

        children.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then(left.name.cmp(&right.name))
        });

        for folder in children {
            let id = folder.id.clone();
            into.push((folder.clone(), depth));

            if !collapsed.contains(&id) {
                walk(folders, collapsed, Some(&id), depth + 1, into);
            }
        }
    }

    let mut rows = Vec::new();
    walk(folders, collapsed, None, 0, &mut rows);

    rows
}

fn has_children(folders: &[Folder], id: &str) -> bool {
    folders
        .iter()
        .any(|folder| folder.parent_id.as_deref() == Some(id))
}

#[component]
pub fn FolderTree(
    folders: Signal<Vec<Folder>>,
    selected: Signal<Option<String>>,
    can_edit: bool,
    counts: Signal<Counts>,
    actions: FolderActions,
) -> impl IntoView {
    let collapsed = RwSignal::new(HashSet::<String>::new());
    let rows = Signal::derive(move || visible(&folders.get(), &collapsed.get()));

    view! {
        <nav class="text-sm" data-testid="folder-tree">
            <Row
                folder=None
                depth=0
                label="All songs".to_owned()
                count=Signal::derive(move || counts.get().values().sum())
                open=None
                selected=selected
                can_edit=can_edit
                collapsed=collapsed
                actions=actions
            />

            <For each=move || rows.get() key=|(folder, _)| folder.id.clone() let:entry>
                {
                    let (folder, depth) = entry;
                    let id = folder.id.clone();
                    let for_count = id.clone();

                    view! {
                        <Row
                            folder=Some(folder)
                            depth=depth
                            label=String::new()
                            count=Signal::derive(move || {
                                counts.get().get(&Some(for_count.clone())).copied().unwrap_or(0)
                            })
                            open=Some(Signal::derive(move || {
                                has_children(&folders.get(), &id)
                            }))
                            selected=selected
                            can_edit=can_edit
                            collapsed=collapsed
                            actions=actions
                        />
                    }
                }
            </For>

            <Show when=move || can_edit>
                <button
                    class="mt-2 text-xs underline text-slate-500"
                    data-testid="new-folder"
                    on:click=move |_| actions.create.run(None)
                >
                    "New folder"
                </button>
            </Show>
        </nav>
    }
}

#[component]
fn Row(
    folder: Option<Folder>,
    depth: usize,
    label: String,
    count: Signal<usize>,
    /// `Some` for a real folder: whether it has children to expand. `None` for "All songs".
    open: Option<Signal<bool>>,
    selected: Signal<Option<String>>,
    can_edit: bool,
    collapsed: RwSignal<HashSet<String>>,
    actions: FolderActions,
) -> impl IntoView {
    let navigate = StoredValue::new_local(use_navigate());
    let row = StoredValue::new(folder.clone());
    let id = StoredValue::new(folder.as_ref().map(|folder| folder.id.clone()));
    let name = folder
        .as_ref()
        .map(|folder| folder.name.clone())
        .unwrap_or(label);

    let renaming = RwSignal::new(false);
    let over = RwSignal::new(false);
    let editable = can_edit && row.get_value().is_some();

    let is_selected = Signal::derive(move || selected.get() == id.get_value());
    let expanded = Signal::derive(move || {
        id.get_value()
            .is_none_or(|id| !collapsed.get().contains(&id))
    });

    let drop = move |event: DragEvent| {
        event.prevent_default();
        over.set(false);

        let Some(data) = event.data_transfer() else {
            return;
        };

        let song = data.get_data(SONG_DRAG_TYPE).unwrap_or_default();
        let dragged = data.get_data(FOLDER_DRAG_TYPE).unwrap_or_default();
        let here = id.get_value();

        if !song.is_empty() {
            actions.move_song.run((song, here));
        } else if !dragged.is_empty() && Some(&dragged) != here.as_ref() {
            actions.move_folder.run((dragged, here));
        }
    };

    let toggle = move |_| {
        let Some(id) = id.get_value() else {
            return;
        };

        collapsed.update(|held| {
            if !held.remove(&id) {
                held.insert(id);
            }
        });
    };

    let finish_rename = move |event: leptos::ev::FocusEvent| {
        let value = event
            .target()
            .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
            .map(|input| input.value())
            .unwrap_or_default();

        if let Some(id) = id.get_value()
            && !value.trim().is_empty()
        {
            actions.rename.run((id, value));
        }

        renaming.set(false);
    };

    let keys = move |event: KeyboardEvent| {
        match event.key().as_str() {
            // Enter commits by blurring, which is the one path that writes.
            "Enter" => {
                if let Some(input) = event
                    .target()
                    .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
                {
                    let _ = input.blur();
                }
            }
            "Escape" => renaming.set(false),
            _ => {}
        }
    };

    view! {
        <div
            class=move || {
                let mut classes = "flex items-center gap-1 rounded px-2 py-1".to_owned();

                if is_selected.get() {
                    classes.push_str(" bg-slate-200 dark:bg-slate-800");
                }

                if over.get() {
                    classes.push_str(" ring-2 ring-sky-400");
                }

                classes
            }
            style=format!("padding-left: {}px", depth * 12 + 8)
            on:dragover=move |event: DragEvent| {
                event.prevent_default();
                over.set(true);
            }
            on:dragleave=move |_| over.set(false)
            on:drop=drop
        >
            <button
                class=move || {
                    if open.is_some_and(|has| has.get()) {
                        "w-4 text-slate-400"
                    } else {
                        "w-4 text-slate-400 invisible"
                    }
                }
                aria-label=move || if expanded.get() { "Collapse" } else { "Expand" }
                on:click=toggle
            >
                {move || if expanded.get() { "▾" } else { "▸" }}
            </button>

            <Show
                when=move || renaming.get()
                fallback=move || {
                    let shown = name.clone();

                    view! {
                        <button
                            class="flex-1 truncate text-left"
                            draggable=if editable { "true" } else { "false" }
                            on:dragstart=move |event: DragEvent| {
                                if let (Some(data), Some(id)) = (
                                    event.data_transfer(),
                                    id.get_value(),
                                ) {
                                    let _ = data.set_data(FOLDER_DRAG_TYPE, &id);
                                }
                            }
                            on:click=move |_| {
                                navigate
                                    .get_value()(
                                    &match id.get_value() {
                                        Some(id) => format!("/library/folder/{id}"),
                                        None => "/library".to_owned(),
                                    },
                                    Default::default(),
                                );
                            }
                            on:dblclick=move |_| {
                                if editable {
                                    renaming.set(true);
                                }
                            }
                        >
                            {shown}
                        </button>
                    }
                }
            >
                <input
                    class="flex-1 rounded border border-slate-300 px-1 dark:border-slate-700 dark:bg-slate-900"
                    autofocus
                    value=row.get_value().map(|folder| folder.name).unwrap_or_default()
                    on:blur=finish_rename
                    on:keydown=keys
                />
            </Show>

            <span class="text-xs text-slate-400">{move || count.get()}</span>

            <Show when=move || editable>
                <span class="flex gap-1 text-xs text-slate-400">
                    <button title="New subfolder" on:click=move |_| actions.create.run(id.get_value())>
                        "＋"
                    </button>
                    <button
                        title="Delete folder"
                        on:click=move |_| {
                            if let Some(folder) = row.get_value() {
                                actions.delete.run(folder);
                            }
                        }
                    >
                        "🗑"
                    </button>
                </span>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, parent: Option<&str>, position: i64) -> Folder {
        Folder {
            id: id.to_owned(),
            name: id.to_owned(),
            parent_id: parent.map(str::to_owned),
            position,
            ..Folder::default()
        }
    }

    #[test]
    fn children_are_drawn_under_their_parent() {
        let folders = vec![
            folder("b", None, 1),
            folder("a", None, 0),
            folder("a1", Some("a"), 0),
        ];

        let rows = visible(&folders, &HashSet::new());
        let names: Vec<(&str, usize)> = rows
            .iter()
            .map(|(folder, depth)| (folder.id.as_str(), *depth))
            .collect();

        assert_eq!(names, vec![("a", 0), ("a1", 1), ("b", 0)]);
    }

    /// Two offline devices can leave a loop in the tree; the pane must still draw.
    #[test]
    fn a_tree_that_loops_still_terminates() {
        let folders = vec![folder("a", Some("b"), 0), folder("b", Some("a"), 0)];

        assert!(visible(&folders, &HashSet::new()).is_empty());
    }

    #[test]
    fn a_collapsed_folder_hides_its_whole_subtree() {
        let folders = vec![
            folder("a", None, 0),
            folder("a1", Some("a"), 0),
            folder("a1x", Some("a1"), 0),
        ];

        let collapsed = HashSet::from(["a".to_owned()]);
        let rows = visible(&folders, &collapsed);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.id, "a");
    }
}
