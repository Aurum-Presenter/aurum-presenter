//! Files that arrived from outside the app: opened from the file system with Aurum, or shared to
//! it from another app.
//!
//! Charts go through exactly the same parser as the import dialog — there is one importer, not a
//! second one for shared files. A PDF is a sheet, and a sheet needs to know which song it belongs
//! to, so it asks.

use std::cell::RefCell;

use aurum_core::library::importer::{ImportResult, import_file};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, HtmlSelectElement};

use crate::app::use_workspace;
use crate::db::live::live_query;
use crate::db::records::{Song, alive};
use crate::library::import::{land, text_of};
use crate::sheets::repository::{SheetInput, problem_with};

/// Where the service worker parks an Android share until a page is open to collect it.
const SHARE_CACHE: &str = "aurum-shared-files";

thread_local! {
    /// Files handed over by the OS when Aurum is used to open them. The launch queue is consumed
    /// once, at start-up, and parked here for this page to pick up.
    static HANDED: RefCell<Vec<File>> = const { RefCell::new(Vec::new()) };
}

fn take_handed() -> Vec<File> {
    HANDED.with(|held| held.borrow_mut().drain(..).collect())
}

/// Consumes the launch queue, which the browser fills when Aurum is the app that opened a file.
///
/// Called once at start-up, because the queue has one consumer and a later one gets nothing.
pub fn accept_launch_files() {
    let Some(window) = web_sys::window() else {
        return;
    };

    let Ok(queue) = js_sys::Reflect::get(&window, &JsValue::from_str("launchQueue")) else {
        return;
    };

    let Ok(set_consumer) = js_sys::Reflect::get(&queue, &JsValue::from_str("setConsumer"))
        .and_then(|held| held.dyn_into::<js_sys::Function>())
    else {
        return;
    };

    let consumer = Closure::<dyn Fn(JsValue)>::new(move |params: JsValue| {
        spawn_local(async move {
            let Ok(files) = js_sys::Reflect::get(&params, &JsValue::from_str("files")) else {
                return;
            };

            for handle in js_sys::Array::from(&files).iter() {
                let Some(file) = file_of(&handle).await else {
                    continue;
                };

                HANDED.with(|held| held.borrow_mut().push(file));
            }

            let empty = HANDED.with(|held| held.borrow().is_empty());
            let Some(window) = web_sys::window() else {
                return;
            };

            let path = window.location().pathname().unwrap_or_default();

            if !empty && !path.starts_with("/share") {
                let _ = window.location().assign("/share?from=file-handler");
            }
        });
    });

    let _ = set_consumer.call1(&queue, consumer.as_ref().unchecked_ref());
    consumer.forget();
}

/// `FileSystemFileHandle.getFile()`, which is not in `web-sys`.
async fn file_of(handle: &JsValue) -> Option<File> {
    let method = js_sys::Reflect::get(handle, &JsValue::from_str("getFile"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;

    let promise = method
        .call0(handle)
        .ok()?
        .dyn_into::<js_sys::Promise>()
        .ok()?;

    JsFuture::from(promise).await.ok()?.dyn_into::<File>().ok()
}

/// Everything the service worker parked from an Android share, emptying the cache as it goes.
async fn shared_files() -> Vec<File> {
    let mut collected = Vec::new();

    let Some(caches) = web_sys::window().and_then(|window| window.caches().ok()) else {
        return collected;
    };

    let Ok(cache) = JsFuture::from(caches.open(SHARE_CACHE))
        .await
        .and_then(|held| held.dyn_into::<web_sys::Cache>())
    else {
        return collected;
    };

    let Ok(keys) = JsFuture::from(cache.keys()).await else {
        return collected;
    };

    for request in js_sys::Array::from(&keys).iter() {
        let Ok(response) = JsFuture::from(cache.match_with_request(&request.clone().into())).await
        else {
            continue;
        };

        if let Ok(response) = response.dyn_into::<web_sys::Response>()
            && let Some(file) = file_from(&request, &response).await
        {
            collected.push(file);
        }

        let _ = JsFuture::from(cache.delete_with_request(&request.into())).await;
    }

    collected
}

/// The parked response, back as the file it was: the name is in the request path, prefixed with
/// the index the worker used to keep two files of the same name apart.
async fn file_from(request: &JsValue, response: &web_sys::Response) -> Option<File> {
    let url = js_sys::Reflect::get(request, &JsValue::from_str("url"))
        .ok()?
        .as_string()?;

    let tail = url.rsplit('/').next().unwrap_or("shared").to_owned();
    let decoded = js_sys::decode_uri_component(&tail)
        .map(String::from)
        .unwrap_or(tail);
    let name = match decoded.split_once('-') {
        Some((index, rest)) if index.chars().all(|digit| digit.is_ascii_digit()) => rest.to_owned(),
        _ => decoded,
    };

    let blob = JsFuture::from(response.blob().ok()?)
        .await
        .ok()?
        .dyn_into::<web_sys::Blob>()
        .ok()?;

    let parts = js_sys::Array::of1(blob.as_ref());
    let options = web_sys::FilePropertyBag::new();
    options.set_type(
        &response
            .headers()
            .get("content-type")
            .ok()
            .flatten()
            .unwrap_or_default(),
    );

    File::new_with_blob_sequence_and_options(parts.as_ref(), &name, &options).ok()
}

fn is_pdf(file: &File) -> bool {
    file.type_() == "application/pdf" || file.name().to_lowercase().ends_with(".pdf")
}

#[component]
pub fn SharePage() -> impl IntoView {
    let context = use_workspace();
    let db = context.db;
    let navigate = StoredValue::new_local(use_navigate());

    let collected = RwSignal::new(None::<usize>);
    let results = RwSignal::new(Vec::<ImportResult>::new());
    let pdfs = StoredValue::new_local(Vec::<File>::new());
    let waiting = RwSignal::new(0usize);
    let chosen = RwSignal::new(String::new());

    let songs = live_query(&["songs"], move || {
        let db = db.get();

        async move {
            let Some(db) = db else {
                return Vec::new();
            };

            let mut rows: Vec<Song> = alive(db.all::<Song>("songs").await.unwrap_or_default());
            rows.sort_by(|left, right| left.title.cmp(&right.title));

            rows
        }
    });

    let library = StoredValue::new(context.library());
    let engine = context.engine;

    // One pass over everything that arrived, whichever way it arrived.
    Effect::new(move |previous: Option<()>| {
        if previous.is_some() {
            return;
        }

        spawn_local(async move {
            let mut files = take_handed();
            files.extend(shared_files().await);

            collected.set(Some(files.len()));

            let mut imported = Vec::new();
            let mut documents = Vec::new();

            for file in files {
                if is_pdf(&file) {
                    documents.push(file);
                    continue;
                }

                let result = import_file(&file.name(), &text_of(&file).await);

                if let (Some(song), Some(library), Some(engine)) =
                    (result.song.as_ref(), library.get_value(), engine.get())
                {
                    land(&library, &engine, song, None).await;
                }

                imported.push(result);
            }

            waiting.set(documents.len());
            pdfs.set_value(documents);
            results.set(imported);
        });
    });

    let attach = move |_| {
        let song_id = chosen.get_untracked();

        if song_id.is_empty() {
            return;
        }

        spawn_local(async move {
            if let Some(sheets) = crate::sheets::panel::use_sheets() {
                for file in pdfs.get_value() {
                    if problem_with(&file).is_none() {
                        let _ = sheets
                            .attach(
                                &song_id,
                                &file,
                                &SheetInput {
                                    part: "lead".to_owned(),
                                    ..SheetInput::default()
                                },
                            )
                            .await;
                    }
                }
            }

            navigate.get_value()(&format!("/song/{song_id}"), Default::default());
        });
    };

    // The three states the React screen returned early for: still looking, nothing arrived, and
    // what to do with what did.
    move || {
        match collected.get() {
        None => view! {
            <p class="p-6 text-sm text-ink-3">"Looking at what was shared…"</p>
        }
        .into_any(),
        Some(0) => view! {
            <div class="p-6 text-sm">
                <p class="text-ink-3">"Nothing was shared with Aurum."</p>
                <A href="/library" attr:class="text-ink-3 hover:text-ink underline-offset-2 hover:underline">"Go to the library"</A>
            </div>
        }
        .into_any(),
        Some(_) => view! {
            <div class="mx-auto max-w-2xl p-4" data-testid="share-page">
                <h2 class="mb-3 text-2xl font-semibold">"Shared with Aurum"</h2>

                <Show when=move || !results.get().is_empty()>
                    <section class="mb-6">
                        <h3 class="mb-1 font-semibold" data-testid="share-imported">
                            {move || {
                                let landed = results
                                    .get()
                                    .iter()
                                    .filter(|result| result.error.is_none())
                                    .count();

                                format!("{landed} song(s) imported")
                            }}
                        </h3>
                        <ul class="space-y-1 text-sm">
                            <For
                                each=move || results.get()
                                key=|result| result.filename.clone()
                                let:result
                            >
                                <li class=if result.error.is_none() {
                                    String::new()
                                } else {
                                    "text-warn".to_owned()
                                }>
                                    <span class="font-mono text-xs">
                                        {result.filename.clone()}
                                    </span>
                                    {match (&result.error, &result.song) {
                                        (Some(problem), _) => format!(" — {problem}"),
                                        (None, Some(song)) => format!(" — {}", song.title),
                                        (None, None) => String::new(),
                                    }}
                                </li>
                            </For>
                        </ul>
                    </section>
                </Show>

                <Show when=move || { waiting.get() > 0 }>
                    <section>
                        <h3 class="mb-1 font-semibold">
                            {move || format!("{} PDF(s) to attach", waiting.get())}
                        </h3>
                        <p class="mb-2 text-sm text-ink-3">
                            "A sheet belongs to a song. Which one?"
                        </p>

                        <select
                            class="mb-3 w-full rounded-md border border-line-strong px-2 py-2"
                            data-testid="share-song"
                            prop:value=move || chosen.get()
                            on:change=move |event| {
                                let picked = event
                                    .target()
                                    .and_then(|target| {
                                        target.dyn_into::<HtmlSelectElement>().ok()
                                    })
                                    .map(|select| select.value())
                                    .unwrap_or_default();

                                chosen.set(picked);
                            }
                        >
                            <option value="">"Choose a song…"</option>
                            <For
                                each=move || songs.get().unwrap_or_default()
                                key=|song| song.id.clone()
                                let:song
                            >
                                <option value=song.id.clone()>{song.title.clone()}</option>
                            </For>
                        </select>

                        <button
                            class="rounded-md bg-accent px-4 py-2 text-sm text-on-accent disabled:opacity-40"
                            data-testid="share-attach"
                            disabled=move || chosen.get().is_empty()
                            on:click=attach
                        >
                            {move || format!("Attach {} sheet(s)", waiting.get())}
                        </button>
                    </section>
                </Show>

                <A href="/library" attr:class="mt-6 block text-sm text-ink-3 hover:text-ink underline-offset-2 hover:underline">"Done"</A>
            </div>
        }
        .into_any(),
    }
    }
}
