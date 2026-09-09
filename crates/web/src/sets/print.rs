//! The printable pack: every item of the set, each chart at the key this member will actually
//! play it in, and the sheet pages for the songs that have one.
//!
//! It is a print stylesheet rather than a generated file, which is what makes it work offline —
//! nothing is fetched and no service renders it. What it cannot do offline is print a sheet the
//! device never downloaded, so it says which ones those are before it starts rather than after.

use aurum_core::chart::notes::Key;
use aurum_core::sheets::selection::{SheetChoice, select_sheet};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use super::resolved::{ResolvedItem, use_resolved_set};
use crate::app::use_workspace;
use crate::blobs::BlobStore;
use crate::chart::view::ChartView;
use crate::db::live::live_query;
use crate::db::records::{BlobRecord, Sheet, alive};
use crate::prefs::display::{self, Display};
use crate::prefs::user;
use crate::sheets::renderer::{PdfiumRenderer, SheetRenderer, bytes_of};

/// Wide enough that a printer has real pixels to work with, small enough that a ten-page pack
/// does not exhaust a phone.
const PRINT_WIDTH: u32 = 1400;

#[component]
pub fn PrintPage() -> impl IntoView {
    let context = use_workspace();
    let params = use_params_map();
    let set_id = Signal::derive(move || params.read().get("set_id").unwrap_or_default());
    let resolved = use_resolved_set(set_id);
    let display = RwSignal::new(display::load());
    let db = context.db;
    let me = context.me;

    let use_sheets = RwSignal::new(true);

    let sheets = live_query(&["sheets", "blobs", "preferences"], move || {
        let (db, user_id) = (db.get(), me.get().id);

        async move {
            let Some(db) = db else {
                return (Vec::new(), Vec::new(), None);
            };

            let rows: Vec<Sheet> = alive(db.all("sheets").await.unwrap_or_default());
            let held: Vec<String> = db
                .all::<BlobRecord>("blobs")
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|blob| blob.sheet_id)
                .collect();

            (rows, held, user::read(&db, &user_id).await.part)
        }
    });

    let set = Signal::derive(move || resolved.get().set);
    let items = Signal::derive(move || resolved.get().items);

    // Which sheet each item would print, and which of those are not on this device.
    let chosen = Signal::derive(move || {
        let (rows, held, part) = sheets.get().unwrap_or_default();
        let mut picked: Vec<(String, Option<String>)> = Vec::new();
        let mut missing: Vec<String> = Vec::new();

        for resolved in items.get() {
            let Some(song) = resolved.song.as_ref() else {
                picked.push((resolved.item.id.clone(), None));
                continue;
            };

            let for_song: Vec<&Sheet> = rows
                .iter()
                .filter(|sheet| sheet.song_id == song.id)
                .collect();
            let choices: Vec<SheetChoice<'_>> = for_song
                .iter()
                .map(|sheet| SheetChoice {
                    id: &sheet.id,
                    sheet_key: sheet.sheet_key.as_deref(),
                    part: sheet.part.as_deref(),
                    position: sheet.position,
                    deleted: sheet.sync.deleted_at.is_some(),
                })
                .collect();

            let selection = select_sheet(&choices, resolved.key.as_ref(), part);
            let Some(id) = selection.map(|selection| selection.sheet.id.to_owned()) else {
                picked.push((resolved.item.id.clone(), None));
                continue;
            };

            if held.contains(&id) {
                picked.push((resolved.item.id.clone(), Some(id)));
            } else {
                // A row with a file on the server but not here. The chart prints instead.
                if for_song
                    .iter()
                    .any(|sheet| sheet.id == id && sheet.sha256.is_some())
                {
                    missing.push(resolved.title.clone());
                }

                picked.push((resolved.item.id.clone(), None));
            }
        }

        (picked, missing)
    });

    // A memo, not a derived signal: rendering a sheet reads its file, and anything that wakes
    // this list rebuilds every item and throws away the pages it had just drawn. The memo only
    // notifies when the plan actually changes.
    let plan = Memo::new(move |_| {
        let picked = chosen.get().0;
        let with_sheets = use_sheets.get();

        items
            .get()
            .into_iter()
            .map(|resolved| {
                let sheet = with_sheets
                    .then(|| {
                        picked
                            .iter()
                            .find(|(item, _)| item == &resolved.item.id)
                            .and_then(|(_, sheet)| sheet.clone())
                    })
                    .flatten();

                (resolved, sheet)
            })
            .collect::<Vec<_>>()
    });

    let subtitle = Signal::derive(move || {
        set.get()
            .map(|set| {
                [set.scheduled_for, set.venue]
                    .into_iter()
                    .flatten()
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .unwrap_or_default()
    });

    view! {
        <Show
            when=move || set.get().is_some()
            fallback=|| view! { <p class="p-6 text-sm text-slate-500">"Loading…"</p> }
        >
            <div class="mx-auto max-w-4xl p-6 print:max-w-none print:p-0">
                <style>
                    "@media print {
                       .no-print { display: none !important; }
                       .set-item { break-after: page; }
                       .set-item:last-child { break-after: auto; }
                       body { background: white; color: black; }
                     }"
                </style>

                <div class="no-print mb-4">
                    <div class="flex flex-wrap items-center gap-3">
                        <A
                            href=move || format!("/sets/{}", set_id.get())
                            attr:class="text-sm underline"
                        >
                            {move || format!(
                                "← {}",
                                set.get().map(|set| set.name).unwrap_or_default(),
                            )}
                        </A>

                        <label class="flex items-center gap-1 text-sm text-slate-500">
                            <input
                                type="checkbox"
                                prop:checked=move || use_sheets.get()
                                on:change=move |event| use_sheets.set(event_target_checked(&event))
                            />
                            "Use sheet PDFs where there is one"
                        </label>

                        <button
                            class="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900"
                            data-testid="print"
                            on:click=move |_| {
                                if let Some(window) = web_sys::window() {
                                    let _ = window.print();
                                }
                            }
                        >
                            "Print or save as PDF"
                        </button>
                    </div>

                    <Show when=move || { use_sheets.get() && !chosen.get().1.is_empty() }>
                        <p
                            class="mt-2 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900"
                            data-testid="sheets-not-here"
                        >
                            {move || {
                                let missing = chosen.get().1;

                                format!(
                                    "{} sheet{} ({}) {} not been downloaded to this device. \
                                     Printing now uses the chart for those songs instead.",
                                    missing.len(),
                                    if missing.len() == 1 { "" } else { "s" },
                                    missing.join(", "),
                                    if missing.len() == 1 { "has" } else { "have" },
                                )
                            }}
                        </p>
                    </Show>

                    <p class="mt-2 text-xs text-slate-500">
                        "Everything below is rendered here on the device. Nothing is fetched, so \
                         this works offline."
                    </p>
                </div>

                <h1 class="mb-1 text-2xl font-semibold">
                    {move || set.get().map(|set| set.name).unwrap_or_default()}
                </h1>
                <p class="mb-6 text-sm text-slate-500">{move || subtitle.get()}</p>

                {move || plan
                    .get()
                    .into_iter()
                    .enumerate()
                    .map(|(index, (resolved, sheet))| view! {
                        <PrintItem index resolved sheet display=display.into() />
                    })
                    .collect_view()}
            </div>
        </Show>
    }
}

#[component]
fn PrintItem(
    index: usize,
    resolved: ResolvedItem,
    /// The sheet to print instead of the chart, when this device has its file.
    sheet: Option<String>,
    display: Signal<Display>,
) -> impl IntoView {
    let context = use_workspace();
    let pages = RwSignal::new(Vec::<String>::new());
    let fallback = Key::parse("C").expect("C is a key");
    let written = resolved.written.unwrap_or(fallback);
    let target = resolved.key.unwrap_or(written);
    let capo = resolved.capo;
    let body = resolved
        .arrangement
        .as_ref()
        .map(|row| row.body.clone())
        .unwrap_or_default();

    // Rasterised here rather than embedded: a print stylesheet cannot page a PDF, and every
    // surface that shows a sheet goes through the same renderer.
    if let Some(sheet_id) = sheet.clone() {
        let db = context.db;

        Effect::new(move |_| {
            let (Some(db), sheet_id) = (db.get(), sheet_id.clone()) else {
                return;
            };

            spawn_local(async move {
                let store = BlobStore::new(db.clone(), db.workspace_id());

                let (Some(blob), Ok(renderer)) =
                    (store.get(&sheet_id).await, PdfiumRenderer::load().await)
                else {
                    return;
                };

                let Some(bytes) = bytes_of(&blob).await else {
                    return;
                };

                let Ok(count) = renderer.page_count(&bytes) else {
                    return;
                };

                let mut drawn = Vec::new();

                for number in 0..count {
                    let Ok(page) = renderer.render(&bytes, number, PRINT_WIDTH) else {
                        break;
                    };

                    if let Some(url) = page.to_data_url() {
                        drawn.push(url);
                    }
                }

                pages.set(drawn);
            });
        });
    }

    view! {
        <section class="set-item mb-8">
            <h2 class="mb-1 text-lg font-semibold">
                <span class="mr-2 text-slate-400">{index + 1}</span>
                {resolved.title.clone()}
                {resolved.key.map(|key| view! {
                    <span class="ml-3 text-base font-normal text-slate-500">{key.to_string()}</span>
                })}
                {(capo > 0).then(|| view! {
                    <span class="ml-2 text-base font-normal text-slate-500">
                        {format!("capo {capo}")}
                    </span>
                })}
            </h2>

            {resolved.item.note.clone().map(|note| view! {
                <p class="mb-2 text-sm italic">{note}</p>
            })}
            {resolved.item.content.clone().map(|content| view! {
                <p class="whitespace-pre-wrap">{content}</p>
            })}
            {resolved.missing.then(|| view! {
                <p class="text-sm">"This song is no longer in the library."</p>
            })}

            <Show
                when=move || !pages.get().is_empty()
                fallback={
                    let body = body.clone();

                    move || {
                        let body = body.clone();

                        (!body.trim().is_empty()).then(|| view! {
                            <ChartView
                                body=Signal::derive(move || body.clone())
                                source=Signal::derive(move || written)
                                target=Signal::derive(move || target)
                                capo=Signal::derive(move || capo)
                                display
                            />
                        })
                    }
                }
            >
                {move || pages
                    .get()
                    .into_iter()
                    .map(|source| view! { <img src=source alt="" class="mb-2 w-full" /> })
                    .collect_view()}
            </Show>
        </section>
    }
}
