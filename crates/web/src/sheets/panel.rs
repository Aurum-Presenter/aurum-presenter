//! The sheets attached to a song: which one is being offered for the key on screen, why, and
//! everything needed to attach another.
//!
//! A sheet whose file this device has never downloaded says exactly that. It is a normal state —
//! the row synced and the file did not — and it is offered with the thing that fixes it.

use aurum_core::chart::notes::Key;
use aurum_core::sheets::selection::{PARTS, Part, SheetChoice, select_sheet};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use web_sys::{File, HtmlInputElement};

use super::repository::{SheetInput, Sheets, WARN_SHEET_BYTES, file_size, problem_with};
use crate::app::use_workspace;
use crate::blobs::BlobStore;
use crate::db::live::live_query;
use crate::db::records::{BlobRecord, Sheet, alive};

fn twelve() -> Vec<Key> {
    let c = Key::parse("C").expect("C is a key");

    (0..12).map(|semitones| c.transposed(semitones)).collect()
}

pub fn use_sheets() -> Option<Sheets> {
    let context = use_workspace();
    let db = context.db.get_untracked()?;
    let engine = context.engine.get_untracked()?;
    let store = BlobStore::new(db.clone(), db.workspace_id());

    Some(Sheets::new(db, engine, store))
}

/// The first file of an `<input type="file">`, which is the only one any of these accept.
fn chosen_file(event: &web_sys::Event) -> Option<File> {
    event
        .target()
        .and_then(|target| wasm_bindgen::JsCast::dyn_into::<HtmlInputElement>(target).ok())
        .and_then(|input| input.files())
        .and_then(|files| files.get(0))
}

#[component]
pub fn SheetsPanel(
    song_id: Signal<String>,
    song_key: Signal<Option<Key>>,
    part: Signal<Option<Part>>,
    on_part: Callback<Option<Part>>,
    pinned: Signal<bool>,
    on_pinned: Callback<bool>,
) -> impl IntoView {
    let context = use_workspace();
    let can_edit = context.can_edit();
    let db = context.db;
    let attaching = RwSignal::new(false);

    let sheets = live_query(&["sheets"], move || {
        let db = db.get();
        let song_id = song_id.get();

        async move {
            let Some(db) = db else {
                return Vec::new();
            };

            let mut rows: Vec<Sheet> = alive(
                db.by_index("sheets", "song_id", &song_id.as_str().into())
                    .await
                    .unwrap_or_default(),
            );

            rows.sort_by_key(|sheet| sheet.position);

            rows
        }
    });

    let cached = live_query(&["blobs"], move || {
        let db = db.get();

        async move {
            match db {
                Some(db) => db
                    .all::<BlobRecord>("blobs")
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|blob| blob.sheet_id)
                    .collect::<Vec<_>>(),
                None => Vec::new(),
            }
        }
    });

    let rows = Signal::derive(move || sheets.get().unwrap_or_default());

    // The choice and its explanation come from `aurum-core`, so the sentence a musician reads
    // here is the same rule the print pack and the slides obey.
    let chosen = Signal::derive(move || {
        let rows = rows.get();
        let choices: Vec<SheetChoice<'_>> = rows
            .iter()
            .map(|sheet| SheetChoice {
                id: &sheet.id,
                sheet_key: sheet.sheet_key.as_deref(),
                part: sheet.part.as_deref(),
                position: sheet.position,
                deleted: sheet.sync.deleted_at.is_some(),
            })
            .collect();

        select_sheet(&choices, song_key.get().as_ref(), part.get()).map(|selection| {
            (
                selection.sheet.id.to_owned(),
                selection.explain(song_key.get().as_ref(), part.get()),
            )
        })
    });

    view! {
        <section class="mt-8" data-testid="sheets">
            <div class="mb-2 flex flex-wrap items-center gap-3">
                <h3 class="font-semibold">"Sheets"</h3>

                <label class="flex items-center gap-1 text-sm text-ink-3">
                    "My part"
                    <select
                        class="rounded-md border border-line-strong bg-transparent px-2 py-1"
                        data-testid="my-part"
                        prop:value=move || part.get().map(Part::as_str).unwrap_or_default()
                        on:change=move |event| {
                            on_part.run(Part::parse(&event_target_value(&event)));
                        }
                    >
                        <option value="">"any"</option>
                        {PARTS
                            .into_iter()
                            .map(|value| view! {
                                <option value=value.as_str()>{value.as_str()}</option>
                            })
                            .collect_view()}
                    </select>
                </label>

                <label
                    class="flex items-center gap-1 text-sm text-ink-3"
                    title="Download this song's sheets and keep them"
                >
                    <input
                        type="checkbox"
                        data-testid="keep-sheets-offline"
                        prop:checked=move || pinned.get()
                        on:change=move |event| on_pinned.run(event_target_checked(&event))
                    />
                    "Keep offline"
                </label>

                <Show when=move || can_edit>
                    <button
                        class="ml-auto text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                        data-testid="attach-sheet"
                        on:click=move |_| attaching.set(true)
                    >
                        "Attach a sheet"
                    </button>
                </Show>
            </div>

            <Show
                when=move || !rows.get().is_empty()
                fallback=|| view! {
                    <p class="text-sm text-ink-3">
                        "No sheets for this song. The chart above is always available; a PDF is \
                         optional."
                    </p>
                }
            >
                {move || chosen.get().and_then(|(_, why)| why).map(|why| view! {
                    <p
                        class="mb-2 rounded-md border border-warn/50 bg-warn/10 px-3 py-2 text-sm text-warn"
                        data-testid="sheet-fallback"
                    >
                        {why}
                    </p>
                })}

                <ul class="divide-y divide-line ">
                    {move || {
                        let held = cached.get().unwrap_or_default();
                        let picked = chosen.get().map(|(id, _)| id);

                        rows.get()
                            .into_iter()
                            .map(|sheet| {
                                let on_device = held.contains(&sheet.id);
                                let showing = picked.as_deref() == Some(sheet.id.as_str());

                                view! {
                                    <SheetRow sheet song_id showing on_device can_edit />
                                }
                            })
                            .collect_view()
                    }}
                </ul>
            </Show>

            <Show when=move || attaching.get()>
                <AttachDialog
                    song_id
                    song_key
                    on_close=Callback::new(move |()| attaching.set(false))
                />
            </Show>
        </section>
    }
}

#[component]
fn SheetRow(
    sheet: Sheet,
    song_id: Signal<String>,
    showing: bool,
    on_device: bool,
    can_edit: bool,
) -> impl IntoView {
    let id = StoredValue::new(sheet.id.clone());
    let uploaded = sheet.sha256.is_some();
    let size = sheet.size;

    view! {
        <li class="flex flex-wrap items-baseline gap-2 py-2 text-sm">
            <A
                href=move || format!("/song/{}/sheet/{}", song_id.get(), id.get_value())
                attr:class=if showing { "font-medium underline" } else { "underline" }
            >
                {format!(
                    "{}{}",
                    sheet.part.clone().unwrap_or_else(|| "other".to_owned()),
                    sheet.sheet_key.clone().map(|key| format!(" · {key}")).unwrap_or_default(),
                )}
            </A>

            {sheet.label.clone().map(|label| view! {
                <span class="text-ink-3">{label}</span>
            })}

            {sheet.page_count.map(|pages| view! {
                <span class="text-ink-4">
                    {if pages == 1 { "1 page".to_owned() } else { format!("{pages} pages") }}
                </span>
            })}

            {showing.then(|| view! {
                <span class="rounded-full bg-accent/15 px-2 text-xs text-accent">
                    "shown for this key"
                </span>
            })}

            {(!uploaded).then(|| view! {
                <span class="text-xs text-warn">"waiting to upload"</span>
            })}

            {(uploaded && !on_device).then(|| view! {
                <span
                    class="text-xs text-ink-3"
                    title="The row synced, the file has not been downloaded"
                >
                    {format!(
                        "not downloaded{}",
                        size.map(|size| format!(" · {}", file_size(size))).unwrap_or_default(),
                    )}
                </span>
            })}

            <Show when=move || can_edit>
                <span class="ml-auto flex gap-3">
                    <label class="cursor-pointer text-ink-3 hover:text-ink underline-offset-2 hover:underline">
                        "Replace"
                        <input
                            type="file"
                            class="hidden"
                            accept=".pdf,image/png,image/jpeg"
                            on:change=move |event| {
                                let (Some(file), Some(repository)) =
                                    (chosen_file(&event), use_sheets())
                                else {
                                    return;
                                };

                                let id = id.get_value();

                                spawn_local(async move {
                                    let _ = repository.replace(&id, &file).await;
                                });
                            }
                        />
                    </label>

                    <button
                        class="text-live-ink underline-offset-2 hover:underline"
                        on:click=move |_| {
                            let Some(repository) = use_sheets() else {
                                return;
                            };

                            let id = id.get_value();

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

#[component]
fn AttachDialog(
    song_id: Signal<String>,
    song_key: Signal<Option<Key>>,
    on_close: Callback<()>,
) -> impl IntoView {
    let file = StoredValue::new_local(None::<File>);
    let size = RwSignal::new(0.0_f64);
    let key = RwSignal::new(
        song_key
            .get_untracked()
            .map(|key| key.to_string())
            .unwrap_or_default(),
    );
    let part = RwSignal::new(Part::Lead.as_str().to_owned());
    let label = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);

    let attach = move |_| {
        let Some(chosen) = file.get_value() else {
            problem.set(Some("Choose a file first.".to_owned()));
            return;
        };

        if let Some(found) = problem_with(&chosen) {
            problem.set(Some(found));
            return;
        }

        let (Some(repository), song_id) = (use_sheets(), song_id.get_untracked()) else {
            return;
        };

        let input = SheetInput {
            key: Some(key.get_untracked()).filter(|value| !value.is_empty()),
            part: part.get_untracked(),
            label: Some(label.get_untracked().trim().to_owned()).filter(|value| !value.is_empty()),
            arrangement_id: None,
        };

        spawn_local(async move {
            match repository.attach(&song_id, &chosen, &input).await {
                Ok(_) => on_close.run(()),
                Err(error) => problem.set(Some(error.to_string())),
            }
        });
    };

    view! {
        <div
            class="fixed inset-0 z-20 flex items-center justify-center bg-black/60 p-6"
            on:click=move |_| on_close.run(())
        >
            <div
                class="w-96 rounded-md bg-surface p-4 shadow-lg"
                data-testid="attach-sheet-dialog"
                on:click=|event| event.stop_propagation()
            >
                <h2 class="mb-3 font-semibold">"Attach a sheet"</h2>

                <input
                    type="file"
                    accept=".pdf,image/png,image/jpeg"
                    class="mb-3 block w-full text-sm"
                    data-testid="sheet-file"
                    on:change=move |event| {
                        let chosen = chosen_file(&event);

                        size.set(chosen.as_ref().map(|file| file.size()).unwrap_or(0.0));
                        file.set_value(chosen);
                        problem.set(None);
                    }
                />

                <Show when=move || { size.get() > WARN_SHEET_BYTES }>
                    <p class="mb-3 text-xs text-warn">
                        {move || format!(
                            "{} MB will be kept on every device that pins this song.",
                            (size.get() / 1024.0 / 1024.0).round(),
                        )}
                    </p>
                </Show>

                <div class="mb-3 flex gap-2 text-sm">
                    <label class="flex-1">
                        "Key"
                        <select
                            class="w-full rounded-md border border-line-strong bg-transparent px-2 py-1"
                            prop:value=move || key.get()
                            on:change=move |event| key.set(event_target_value(&event))
                        >
                            <option value="">"any key"</option>
                            {twelve()
                                .into_iter()
                                .map(|option| view! {
                                    <option value=option.to_string()>{option.to_string()}</option>
                                })
                                .collect_view()}
                        </select>
                    </label>

                    <label class="flex-1">
                        "Part"
                        <select
                            class="w-full rounded-md border border-line-strong bg-transparent px-2 py-1"
                            prop:value=move || part.get()
                            on:change=move |event| part.set(event_target_value(&event))
                        >
                            {PARTS
                                .into_iter()
                                .map(|option| view! {
                                    <option value=option.as_str()>{option.as_str()}</option>
                                })
                                .collect_view()}
                        </select>
                    </label>
                </div>

                <input
                    class="mb-3 w-full rounded-md border border-line-strong px-2 py-1 text-sm"
                    placeholder="Label — SATB, Kate's copy…"
                    prop:value=move || label.get()
                    on:input=move |event| label.set(event_target_value(&event))
                />

                <Show when=move || problem.get().is_some()>
                    <p class="mb-3 text-sm text-live-ink">{move || problem.get()}</p>
                </Show>

                <div class="flex gap-2">
                    <button
                        class="rounded-md bg-accent px-4 py-2 text-sm text-on-accent"
                        data-testid="confirm-attach"
                        on:click=attach
                    >
                        "Attach"
                    </button>
                    <button class="text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline" on:click=move |_| on_close.run(())>
                        "Cancel"
                    </button>
                </div>
            </div>
        </div>
    }
}
