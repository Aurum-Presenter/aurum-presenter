//! The sheet viewer.
//!
//! Rendering happens locally from the cached file — there is no streaming from the server and no
//! page-by-page fetch — because the whole point of pinning a set is that the PDF opens on a stage
//! with no signal. A file that is not here says so, plainly, with the size it would take.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde_json::json;
use wasm_bindgen::JsCast;
use web_sys::Url;

use super::annotations::{AnnotationLayer, Stroke};
use super::renderer::{PdfiumRenderer, SheetRenderer, bytes_of};
use super::repository::file_size;
use crate::app::use_workspace;
use crate::blobs::{BlobQueue, BlobStore};
use crate::db::Database;
use crate::db::live::live_query;
use crate::db::records::{Annotation, Sheet, alive};

/// How wide a page is rendered when nothing says otherwise. Beyond this the engine is doing work
/// no screen can show.
const MAX_WIDTH: f64 = 2400.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Fit {
    #[default]
    Width,
    Page,
    Free,
}

/// The file queue for the open workspace.
///
/// Takes the database rather than reading the context itself, so callers decide whether the
/// read is reactive. It has to be: on a cold load the database is not open yet, and a resource
/// that reads it untracked resolves to "not downloaded" and never asks again.
fn queue(db: Database) -> BlobQueue {
    let store = BlobStore::new(db.clone(), db.workspace_id());
    let api = use_workspace().api.clone();
    let workspace_id = db.workspace_id().to_owned();

    BlobQueue::new(db, store, api, &workspace_id)
}

#[component]
pub fn SheetViewerPage() -> impl IntoView {
    let context = use_workspace();
    let params = use_params_map();
    let song_id = Signal::derive(move || params.read().get("song_id").unwrap_or_default());
    let sheet_id = Signal::derive(move || params.read().get("sheet_id").unwrap_or_default());
    let can_edit = context.can_edit();
    let me = context.me;
    let db = context.db;

    let page = RwSignal::new(1_i64);
    let pages = RwSignal::new(1_i64);
    let zoom = RwSignal::new(1.0_f64);
    let fit = RwSignal::new(Fit::default());
    let rotation = RwSignal::new(0_i64);
    let scope = RwSignal::new("personal".to_owned());
    let drawing = RwSignal::new(false);
    let failure = RwSignal::new(None::<String>);

    let sheet = live_query(&["sheets"], move || {
        let (db, id) = (db.get(), sheet_id.get());

        async move {
            match db {
                Some(db) => db.get::<Sheet>("sheets", &id).await.unwrap_or_default(),
                None => None,
            }
        }
    });

    let marks = live_query(&["annotations"], move || {
        let (db, id, user_id) = (db.get(), sheet_id.get(), me.get().id);

        async move {
            let Some(db) = db else {
                return Vec::new();
            };

            alive(
                db.by_index::<Annotation>("annotations", "sheet_id", &id.as_str().into())
                    .await
                    .unwrap_or_default(),
            )
            .into_iter()
            .filter(|row| row.scope == "shared" || row.author_id == user_id)
            .collect()
        }
    });

    // The file, fetched once per sheet. `fetch_now` reads the local copy first, which is what
    // makes this work with the radio off.
    let file = LocalResource::new(move || {
        let id = sheet_id.get();
        let queue = db.get().map(queue);

        async move {
            match queue?.fetch_now(&id).await {
                Some(blob) => bytes_of(&blob).await,
                None => None,
            }
        }
    });

    let held = Signal::derive(move || sheet.get().flatten());
    let annotations = Signal::derive(move || marks.get().unwrap_or_default());

    // Marks made before the file's pages moved. Business rule 7: they are kept and flagged,
    // never deleted — a mark 20 mm out of place is still a mark its author can read, and nobody
    // gets to throw away somebody's notes on a score.
    let stale = Signal::derive(move || {
        let Some(moved) = held.get().and_then(|sheet| sheet.pages_changed_at) else {
            return 0;
        };

        annotations
            .get()
            .iter()
            .filter(|row| row.sync.updated_at.as_str() < moved.as_str())
            .count()
    });

    let visible = Signal::derive(move || {
        let wanted = page.get();
        let mut all = Vec::new();

        for row in annotations.get() {
            if row.page != wanted {
                continue;
            }

            all.extend(serde_json::from_str::<Vec<Stroke>>(&row.strokes).unwrap_or_default());
        }

        all
    });

    let mine = Signal::derive(move || {
        let (wanted, user_id, scope) = (page.get(), me.get().id, scope.get());

        annotations
            .get()
            .into_iter()
            .find(|row| row.page == wanted && row.author_id == user_id && row.scope == scope)
            .and_then(|row| serde_json::from_str::<Vec<Stroke>>(&row.strokes).ok())
            .unwrap_or_default()
    });

    let save = Callback::new(move |strokes: Vec<Stroke>| {
        let (Some(engine), Some(db)) = (context.engine.get_untracked(), db.get_untracked()) else {
            return;
        };

        let (sheet_id, user_id, scope, number) = (
            sheet_id.get_untracked(),
            me.get_untracked().id,
            scope.get_untracked(),
            page.get_untracked(),
        );

        spawn_local(async move {
            // One row per reader, page and scope: editing replaces that row rather than adding
            // a second one, so a mark undone stays undone.
            let existing = alive(
                db.by_index::<Annotation>("annotations", "sheet_id", &sheet_id.as_str().into())
                    .await
                    .unwrap_or_default(),
            )
            .into_iter()
            .find(|row| row.page == number && row.author_id == user_id && row.scope == scope);

            let id = existing.map(|row| row.id).unwrap_or_else(crate::new_id);

            let _ = engine
                .record(
                    "annotations",
                    &id,
                    "upsert",
                    json!({
                        "sheet_id": sheet_id,
                        "page": number,
                        "scope": scope,
                        "author_id": user_id,
                        "strokes": serde_json::to_string(&strokes).unwrap_or_else(|_| "[]".to_owned()),
                    })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                )
                .await;
        });
    });

    let turn = move |delta: i64| {
        page.update(|number| *number = (*number + delta).clamp(1, pages.get_untracked().max(1)));
    };

    view! {
        <div class="mx-auto max-w-5xl p-4">
            <div class="mb-3 flex flex-wrap items-center gap-3 text-sm">
                <A href=move || format!("/song/{}", song_id.get()) attr:class="text-ink-3 hover:text-ink underline-offset-2 hover:underline">
                    "← Song"
                </A>

                <span class="font-medium">
                    {move || {
                        let sheet = held.get();

                        format!(
                            "{}{}",
                            sheet
                                .as_ref()
                                .and_then(|sheet| sheet.part.clone())
                                .unwrap_or_else(|| "sheet".to_owned()),
                            sheet
                                .and_then(|sheet| sheet.sheet_key)
                                .map(|key| format!(" · {key}"))
                                .unwrap_or_default(),
                        )
                    }}
                </span>

                {move || held.get().and_then(|sheet| sheet.filename).map(|name| view! {
                    <span class="text-ink-3">{name}</span>
                })}

                <span class="ml-auto flex flex-wrap items-center gap-2">
                    <button
                        class="rounded-md border border-line-strong px-2"
                        data-testid="page-back"
                        on:click=move |_| turn(-1)
                    >
                        "←"
                    </button>
                    <span data-testid="page-position">
                        {move || format!("{} / {}", page.get(), pages.get())}
                    </span>
                    <button
                        class="rounded-md border border-line-strong px-2"
                        data-testid="page-forward"
                        on:click=move |_| turn(1)
                    >
                        "→"
                    </button>

                    <button
                        class="rounded-md border border-line-strong px-2"
                        on:click=move |_| {
                            fit.set(Fit::Free);
                            zoom.update(|value| *value = (*value - 0.25).max(0.25));
                        }
                    >
                        "−"
                    </button>
                    <button
                        class="rounded-md border border-line-strong px-2"
                        data-testid="zoom-in"
                        on:click=move |_| {
                            fit.set(Fit::Free);
                            zoom.update(|value| *value = (*value + 0.25).min(6.0));
                        }
                    >
                        "+"
                    </button>
                    <button
                        class=move || if fit.get() == Fit::Width {
                            "rounded-md border px-2 border-accent"
                        } else {
                            "rounded-md border px-2 border-line-strong"
                        }
                        on:click=move |_| fit.set(Fit::Width)
                    >
                        "fit width"
                    </button>
                    <button
                        class=move || if fit.get() == Fit::Page {
                            "rounded-md border px-2 border-accent"
                        } else {
                            "rounded-md border px-2 border-line-strong"
                        }
                        on:click=move |_| fit.set(Fit::Page)
                    >
                        "fit page"
                    </button>
                    <button
                        class="rounded-md border border-line-strong px-2"
                        data-testid="rotate"
                        on:click=move |_| rotation.update(|value| *value = (*value + 90) % 360)
                    >
                        "rotate"
                    </button>

                    <button
                        class=move || if drawing.get() {
                            "rounded-md border px-2 border-accent"
                        } else {
                            "rounded-md border px-2 border-line-strong"
                        }
                        data-testid="annotate"
                        on:click=move |_| drawing.update(|value| *value = !*value)
                    >
                        {move || if drawing.get() { "done" } else { "annotate" }}
                    </button>

                    <Show when=move || drawing.get()>
                        <select
                            class="rounded-md border border-line-strong bg-transparent px-1"
                            data-testid="annotation-scope"
                            prop:value=move || scope.get()
                            on:change=move |event| scope.set(event_target_value(&event))
                        >
                            <option value="personal">"just me"</option>
                            <Show when=move || can_edit>
                                <option value="shared">"the whole band"</option>
                            </Show>
                        </select>
                    </Show>
                </span>
            </div>

            <Show when=move || { stale.get() > 0 }>
                <p
                    class="mb-3 rounded-md border border-warn/50 bg-warn/10 px-3 py-2 text-sm text-warn"
                    data-testid="marks-may-not-line-up"
                >
                    {move || {
                        let count = stale.get();

                        format!(
                            "{} made before the file was replaced with a different number of \
                             pages, and may not line up. Nothing has been deleted.",
                            if count == 1 {
                                "A mark on this sheet was".to_owned()
                            } else {
                                format!("{count} marks on this sheet were")
                            },
                        )
                    }}
                </p>
            </Show>

            <div class="rounded-md border border-line p-2">
                {move || match (file.get().flatten(), failure.get()) {
                    (_, Some(why)) => view! { <NotDownloaded sheet=held reason=Some(why) /> }
                        .into_any(),

                    (None, None) if file.get().is_none() => view! {
                        <p class="py-16 text-center text-sm text-ink-3">"Opening…"</p>
                    }
                    .into_any(),

                    (None, None) => view! { <NotDownloaded sheet=held reason=None /> }.into_any(),

                    (Some(bytes), None) => {
                        let image = held
                            .get()
                            .map(|sheet| sheet.mime_type != "application/pdf")
                            .unwrap_or(false);

                        if image {
                            view! {
                                <ImagePage
                                    bytes
                                    rotation=rotation.into()
                                    strokes=visible
                                    mine
                                    drawing=drawing.into()
                                    on_change=save
                                />
                            }
                            .into_any()
                        } else {
                            view! {
                                <PdfPage
                                    bytes
                                    page=page.into()
                                    zoom=zoom.into()
                                    fit=fit.into()
                                    on_pages=Callback::new(move |count: i64| pages.set(count))
                                    on_failure=Callback::new(move |why: String| {
                                        failure.set(Some(why))
                                    })
                                    strokes=visible
                                    mine
                                    drawing=drawing.into()
                                    on_change=save
                                />
                            }
                            .into_any()
                        }
                    }
                }}
            </div>
        </div>
    }
}

#[component]
fn NotDownloaded(sheet: Signal<Option<Sheet>>, reason: Option<String>) -> impl IntoView {
    view! {
        <div class="py-16 text-center" data-testid="not-downloaded">
            <p class="text-sm text-ink-3">
                {move || format!(
                    "This sheet has not been downloaded to this device{}.",
                    sheet
                        .get()
                        .and_then(|sheet| sheet.size)
                        .map(|size| format!(" ({})", file_size(size)))
                        .unwrap_or_default(),
                )}
            </p>
            <p class="mt-2 text-sm text-ink-3">
                "Pin the song, or the set it is in, and it will be here the next time you have a \
                 connection."
            </p>
            {reason.map(|reason| view! { <p class="mt-2 text-xs text-ink-4">{reason}</p> })}
        </div>
    }
}

/// One page, rendered from the local file by the engine behind `SheetRenderer`.
#[component]
fn PdfPage(
    bytes: Vec<u8>,
    page: Signal<i64>,
    zoom: Signal<f64>,
    fit: Signal<Fit>,
    on_pages: Callback<i64>,
    on_failure: Callback<String>,
    strokes: Signal<Vec<Stroke>>,
    mine: Signal<Vec<Stroke>>,
    drawing: Signal<bool>,
    on_change: Callback<Vec<Stroke>>,
) -> impl IntoView {
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    // The space the page has, measured on the frame rather than on the canvas: a canvas with
    // nothing in it yet is zero wide, and fitting to that renders a postage stamp.
    let frame = NodeRef::<leptos::html::Div>::new();
    let size = RwSignal::new((0.0_f64, 0.0_f64));
    let held = StoredValue::new(bytes);

    Effect::new(move |_| {
        let (number, zoom, fit) = (page.get(), zoom.get(), fit.get());
        let (Some(element), Some(frame)) = (canvas.get(), frame.get()) else {
            return;
        };

        spawn_local(async move {
            let renderer = match PdfiumRenderer::load().await {
                Ok(renderer) => renderer,
                Err(why) => {
                    on_failure.run(why.to_string());
                    return;
                }
            };

            let bytes = held.get_value();

            let Ok(count) = renderer.page_count(&bytes) else {
                on_failure.run("This file could not be read as a PDF.".to_owned());
                return;
            };

            on_pages.run(i64::from(count));

            if number > i64::from(count) {
                return;
            }

            // Fit is decided against the space the page actually has, in device pixels, so a
            // score is sharp on a phone with a 3× display rather than merely large.
            let window = web_sys::window();
            let available = (frame.client_width() as f64).max(320.0);
            let ratio = window
                .as_ref()
                .map(|window| window.device_pixel_ratio())
                .unwrap_or(1.0);

            let target = match fit {
                Fit::Width => (available - 24.0) * ratio,

                // Fit-page needs the aspect ratio, which needs the page. One extra render at a
                // nominal width buys the shape; the second one is the one that is shown.
                Fit::Page => {
                    let tall = window
                        .and_then(|window| window.inner_height().ok())
                        .and_then(|height| height.as_f64())
                        .unwrap_or(800.0)
                        - 200.0;

                    match renderer.render(&bytes, (number - 1) as i32, 200) {
                        Ok(probe) if probe.height > 0 => {
                            let shape = probe.width as f64 / probe.height as f64;

                            ((available - 24.0).min(tall * shape)) * ratio
                        }
                        _ => (available - 24.0) * ratio,
                    }
                }

                Fit::Free => (available - 24.0) * ratio * zoom,
            }
            .clamp(320.0, MAX_WIDTH);

            match renderer.render(&bytes, (number - 1) as i32, target as u32) {
                Ok(rendered) => {
                    element.set_width(rendered.width);
                    element.set_height(rendered.height);
                    size.set((rendered.width as f64, rendered.height as f64));

                    let Some(context) =
                        element.get_context("2d").ok().flatten().and_then(|held| {
                            held.dyn_into::<web_sys::CanvasRenderingContext2d>().ok()
                        })
                    else {
                        return;
                    };

                    if let Ok(data) = rendered.to_image_data() {
                        let _ = context.put_image_data(&data, 0.0, 0.0);
                    }
                }

                Err(why) => on_failure.run(why.to_string()),
            }
        });
    });

    view! {
        <div node_ref=frame class="relative flex justify-center overflow-auto">
            <div class="relative">
                <canvas node_ref=canvas class="max-w-full" data-testid="sheet-canvas" />

                <AnnotationLayer
                    width=Signal::derive(move || size.get().0)
                    height=Signal::derive(move || size.get().1)
                    strokes
                    mine
                    drawing
                    on_change
                />
            </div>
        </div>
    }
}

/// A sheet that is a photograph rather than a PDF. Same overlay, no engine.
#[component]
fn ImagePage(
    bytes: Vec<u8>,
    rotation: Signal<i64>,
    strokes: Signal<Vec<Stroke>>,
    mine: Signal<Vec<Stroke>>,
    drawing: Signal<bool>,
    on_change: Callback<Vec<Stroke>>,
) -> impl IntoView {
    let size = RwSignal::new((0.0_f64, 0.0_f64));

    let source = {
        let array = js_sys::Uint8Array::from(bytes.as_slice());
        let parts = js_sys::Array::of1(&array);

        web_sys::Blob::new_with_u8_array_sequence(&parts)
            .ok()
            .and_then(|blob| Url::create_object_url_with_blob(&blob).ok())
            .unwrap_or_default()
    };

    // Revoked on the way out, or every sheet opened in this tab leaks its own copy.
    {
        let source = source.clone();

        on_cleanup(move || {
            let _ = Url::revoke_object_url(&source);
        });
    }

    view! {
        <div class="relative flex justify-center">
            <div class="relative">
                <img
                    src=source
                    alt="Sheet"
                    class="max-w-full"
                    style=move || format!("transform: rotate({}deg)", rotation.get())
                    on:load=move |event| {
                        if let Some(image) = event
                            .target()
                            .and_then(|target| {
                                target.dyn_into::<web_sys::HtmlImageElement>().ok()
                            })
                        {
                            size.set((image.client_width() as f64, image.client_height() as f64));
                        }
                    }
                />

                <AnnotationLayer
                    width=Signal::derive(move || size.get().0)
                    height=Signal::derive(move || size.get().1)
                    strokes
                    mine
                    drawing
                    on_change
                />
            </div>
        </div>
    }
}
