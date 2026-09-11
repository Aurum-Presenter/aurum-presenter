//! Import of ChordPro and plain text files.
//!
//! Files are processed in chunks so a folder of two hundred charts cannot freeze the tab, and a
//! file that fails is reported by name and reason while every other file still lands.

use aurum_core::chart::over_lyrics::Notation;
use aurum_core::library::importer::{ImportResult, ImportedSong, import_file};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::json;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, HtmlInputElement};

use crate::app::use_workspace;
use crate::library::SongInput;

/// How many files are read before the tab is given the frame back.
const CHUNK: usize = 20;

/// One imported song, written as the song and the arrangement that carries its chart.
pub async fn land(
    library: &crate::library::Library,
    engine: &crate::sync::SyncEngine,
    song: &ImportedSong,
    folder_id: Option<&str>,
) {
    let Ok(song_id) = library
        .create_song(&SongInput {
            title: song.title.clone(),
            folder_id: folder_id.map(str::to_owned),
            artist: song.artist.clone(),
            original_key: song.original_key.clone(),
            tempo: song.tempo,
            time_signature: song.time_signature.clone(),
            ..SongInput::default()
        })
        .await
    else {
        return;
    };

    let _ = engine
        .record(
            "arrangements",
            &crate::new_id(),
            "upsert",
            json!({
                "song_id": song_id,
                "name": "Default",
                "body": song.body,
                "default_key": song.original_key,
                "is_default": 1,
                "position": 0,
                "source_notation": match song.source_notation {
                    Notation::OverLyrics => "over_lyrics",
                    _ => "chordpro",
                },
                "source_text": song.source_text,
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        )
        .await;
}

/// The text of a file, or an empty string if it cannot be read — which the importer then
/// reports as an empty file rather than as a crash.
pub async fn text_of(file: &File) -> String {
    JsFuture::from(file.text())
        .await
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default()
}

fn chosen(event: &web_sys::Event) -> Vec<File> {
    let Some(files) = event
        .target()
        .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
        .and_then(|input| input.files())
    else {
        return Vec::new();
    };

    (0..files.length()).filter_map(|at| files.get(at)).collect()
}

#[component]
pub fn ImportDialog(folder_id: Signal<Option<String>>, on_close: Callback<()>) -> impl IntoView {
    let context = use_workspace();
    let results = RwSignal::new(None::<Vec<ImportResult>>);
    let busy = RwSignal::new(false);

    let run = move |files: Vec<File>| {
        let (Some(library), Some(engine)) = (context.library(), context.engine.get_untracked())
        else {
            return;
        };

        let folder = folder_id.get_untracked();

        busy.set(true);
        results.set(Some(Vec::new()));

        spawn_local(async move {
            let mut collected: Vec<ImportResult> = Vec::new();

            for chunk in files.chunks(CHUNK) {
                for file in chunk {
                    let result = import_file(&file.name(), &text_of(file).await);

                    if let Some(song) = &result.song {
                        land(&library, &engine, song, folder.as_deref()).await;
                    }

                    collected.push(result);
                }

                // Yield between chunks so the list of results paints as the import runs.
                results.set(Some(collected.clone()));
                gloo_timers::future::TimeoutFuture::new(0).await;
            }

            busy.set(false);
        });
    };

    let failures = Signal::derive(move || {
        results
            .get()
            .unwrap_or_default()
            .into_iter()
            .filter(|result| result.error.is_some())
            .collect::<Vec<_>>()
    });

    let imported =
        Signal::derive(move || results.get().unwrap_or_default().len() - failures.get().len());

    view! {
        <div class="fixed inset-0 z-20 flex items-center justify-center bg-slate-900/50 p-6">
            <div
                class="max-h-[80vh] w-[32rem] overflow-auto rounded bg-white p-4 shadow-lg dark:bg-slate-900"
                data-testid="import-dialog"
            >
                <h2 class="mb-2 font-semibold">"Import songs"</h2>
                <p class="mb-3 text-sm text-slate-500">
                    "ChordPro (" <code>".cho"</code> ", " <code>".chopro"</code> ", "
                    <code>".pro"</code> ") or plain chords-over-lyrics text. Files that cannot be \
                     read are listed; the rest still import."
                </p>

                <input
                    type="file"
                    multiple
                    accept=".cho,.chopro,.chordpro,.crd,.pro,.txt,text/plain"
                    class="mb-3 block w-full text-sm"
                    data-testid="import-files"
                    on:change=move |event| run(chosen(&event))
                />

                <Show when=move || results.get().is_some()>
                    <div class="mb-3 text-sm">
                        <p class="font-medium" data-testid="import-summary">
                            {move || {
                                let failed = failures.get().len();

                                format!(
                                    "{} imported{}{}",
                                    imported.get(),
                                    if failed > 0 {
                                        format!(", {failed} could not be read")
                                    } else {
                                        String::new()
                                    },
                                    if busy.get() { " …" } else { "" },
                                )
                            }}
                        </p>

                        <Show when=move || !failures.get().is_empty()>
                            <ul class="mt-2 space-y-1">
                                <For
                                    each=move || failures.get()
                                    key=|failure| failure.filename.clone()
                                    let:failure
                                >
                                    <li class="text-amber-700 dark:text-amber-400">
                                        <span class="font-mono text-xs">
                                            {failure.filename.clone()}
                                        </span>
                                        " — "
                                        {failure.error.clone()}
                                    </li>
                                </For>
                            </ul>
                        </Show>
                    </div>
                </Show>

                <button
                    class="rounded bg-slate-900 px-4 py-2 text-sm text-white dark:bg-slate-100 dark:text-slate-900"
                    on:click=move |_| on_close.run(())
                >
                    {move || if results.get().is_none() { "Cancel" } else { "Done" }}
                </button>
            </div>
        </div>
    }
}
