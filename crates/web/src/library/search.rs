//! Search, from the main thread's side of the worker.
//!
//! Debounced by 250 ms, as the feature document asks. If a worker cannot be created — an old
//! browser, a test runner — the same functions run here instead: a library that is a little less
//! smooth is better than a search box that does nothing.

use std::cell::RefCell;
use std::rc::Rc;

use aurum_core::library::search::{IndexedSong, SearchIndex};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, Worker};

pub const DEBOUNCE_MS: u32 = 250;
const LIMIT: usize = 50;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Hit {
    pub id: String,
    pub score: f64,
    /// Which field carried the match, so the list can say why a song is in the results.
    pub field: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum FromWorker {
    /// The worker says it finished indexing. It carries a count, which nothing on this side
    /// needs; serde drops it.
    Built,
    Results {
        id: u32,
        hits: Vec<Hit>,
    },
}

/// Holds the worker, or stands in for it.
#[derive(Clone)]
pub struct Search {
    worker: Option<Worker>,
    /// The index, for the case where there is no worker to hold it.
    inline: Rc<RefCell<SearchIndex>>,
    next_query: Rc<RefCell<u32>>,
}

impl Search {
    pub fn new() -> Search {
        // Trunk's loader shim is a classic script that calls `importScripts`, which a module
        // worker refuses — so this one is deliberately not a module.
        Search {
            worker: Worker::new("/search-worker_loader.js").ok(),
            inline: Rc::new(RefCell::new(SearchIndex::default())),
            next_query: Rc::new(RefCell::new(0)),
        }
    }

    pub fn has_worker(&self) -> bool {
        self.worker.is_some()
    }

    /// Rebuilds the index. Cheap to call: the caller debounces, and the work is off this thread.
    pub fn rebuild(&self, songs: &[IndexedSong]) {
        match &self.worker {
            Some(worker) => {
                if let Ok(message) = json!({ "kind": "build", "songs": songs })
                    .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
                {
                    let _ = worker.post_message(&message);
                }
            }
            None => *self.inline.borrow_mut() = SearchIndex::build(songs),
        }
    }

    /// Asks for results, and calls back with them.
    ///
    /// Answers to a query the person has already typed past are dropped, so a slow one cannot
    /// overwrite the list they are looking at now.
    pub fn query(&self, text: &str, answer: impl Fn(Vec<Hit>) + 'static) {
        let id = {
            let mut next = self.next_query.borrow_mut();
            *next += 1;
            *next
        };

        let Some(worker) = &self.worker else {
            let index = self.inline.borrow();

            answer(
                index
                    .search(text, LIMIT)
                    .into_iter()
                    .map(|hit| Hit {
                        id: hit.id,
                        score: hit.score,
                        field: format!("{:?}", hit.field).to_lowercase(),
                    })
                    .collect(),
            );

            return;
        };

        let latest = self.next_query.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Ok(FromWorker::Results { id: answered, hits }) =
                serde_wasm_bindgen::from_value::<FromWorker>(event.data())
            else {
                return;
            };

            if answered == *latest.borrow() {
                answer(hits);
            }
        });

        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        if let Ok(message) = json!({ "kind": "query", "id": id, "text": text, "limit": LIMIT })
            .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        {
            let _ = worker.post_message(&message);
        }
    }
}

impl Default for Search {
    fn default() -> Search {
        Search::new()
    }
}

/// A search box wired to the worker: hits, or `None` when the box is empty.
///
/// `None` is not "no results" — it is "not searching", which is what tells the library to show
/// the folder the person is standing in rather than an empty list.
pub fn use_search(
    songs: Signal<Vec<IndexedSong>>,
    query: Signal<String>,
) -> ReadSignal<Option<Vec<Hit>>> {
    let search = StoredValue::new_local(Search::new());
    let (hits, set_hits) = signal(None::<Vec<Hit>>);

    Effect::new(move |_| {
        let songs = songs.get();

        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(DEBOUNCE_MS).await;

            // The screen may be gone by the time the debounce elapses; there is nothing left
            // to index for.
            let _ = search.try_with_value(|search| search.rebuild(&songs));
        });
    });

    Effect::new(move |_| {
        let text = query.get();

        if text.trim().is_empty() {
            set_hits.set(None);

            return;
        }

        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(DEBOUNCE_MS).await;

            let _ = search.try_with_value(|search| {
                search.query(&text, move |hits| {
                    // Likewise: an answer that arrives after the screen closed has nowhere to go.
                    let _ = set_hits.try_set(Some(hits));
                });
            });
        });
    });

    hits
}

/// Turns the rows the device holds into what the index reads.
pub fn indexed(
    songs: &[crate::db::records::Song],
    arrangements: &[crate::db::records::Arrangement],
) -> Vec<IndexedSong> {
    songs
        .iter()
        .map(|song| IndexedSong {
            id: song.id.clone(),
            title: song.title.clone(),
            alt_titles: super::repository::list_of(song.alt_titles.as_deref()),
            artist: song.artist.clone(),
            tags: super::repository::list_of(song.tags.as_deref()),
            // Lyrics come from the chart, with the chords and directives taken out — which is
            // how a searcher thinks of them.
            lyrics: arrangements
                .iter()
                .filter(|arrangement| arrangement.song_id == song.id)
                .map(|arrangement| aurum_core::library::search::lyrics_of(&arrangement.body))
                .collect::<Vec<_>>()
                .join("\n"),
        })
        .collect()
}
