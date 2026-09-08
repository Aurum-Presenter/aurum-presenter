//! The search index lives in a worker.
//!
//! Not by habit: measured on the WebAssembly this ships as, indexing a thousand songs takes
//! around ninety milliseconds — six frames — and it happens on every song write. The main thread
//! only ever sends songs and receives ranked ids. `crates/web/tests/search_cost.rs` is where
//! that number is checked, and where it will say so if it ever stops being true.

use aurum_core::library::search::{IndexedSong, SearchIndex};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ToWorker {
    Build { songs: Vec<IndexedSong> },
    Query { id: u32, text: String, limit: usize },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum FromWorker {
    Built { songs: usize },
    Results { id: u32, hits: Vec<Hit> },
}

/// The ids and why they matched, which is all the main thread needs; the rows are already there.
#[derive(Debug, Serialize)]
struct Hit {
    id: String,
    score: f64,
    field: String,
}

fn main() {
    console_error_panic_hook::set_once();

    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let mut index = SearchIndex::default();

    let on_message = {
        let scope = scope.clone();

        Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Ok(message) = serde_wasm_bindgen::from_value::<ToWorker>(event.data()) else {
                return;
            };

            let answer = match message {
                ToWorker::Build { songs } => {
                    index = SearchIndex::build(&songs);

                    FromWorker::Built { songs: songs.len() }
                }
                ToWorker::Query { id, text, limit } => FromWorker::Results {
                    id,
                    hits: index
                        .search(&text, limit)
                        .into_iter()
                        .map(|hit| Hit {
                            id: hit.id,
                            score: hit.score,
                            field: format!("{:?}", hit.field).to_lowercase(),
                        })
                        .collect(),
                },
            };

            if let Ok(value) = serde_wasm_bindgen::to_value(&answer) {
                let _ = scope.post_message(&value);
            }
        })
    };

    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
}
