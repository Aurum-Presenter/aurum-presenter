//! How long the search index takes to build, in the browser, on the WebAssembly it will ship as.
//!
//! The client puts the index in a Web Worker because rebuilding it on every song write was
//! landing on the frame the musician was typing into. Whether that was still true after the port
//! was a question with a number for an answer, and the answer is yes: a thousand songs take
//! around 90 ms to index, which is six frames. Querying is another matter — under a millisecond
//! — so only the build needs to be off the main thread.
//!
//! This guards both halves of that. If indexing ever becomes cheap enough to do inline, the
//! failure here is the signal to delete a moving part.

#![cfg(target_arch = "wasm32")]

use aurum_core::library::search::{IndexedSong, SearchIndex};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn library(size: usize) -> Vec<IndexedSong> {
    (0..size)
        .map(|number| IndexedSong {
            id: format!("song-{number}"),
            title: format!("Song number {number} of the library"),
            alt_titles: vec![format!("Alternate {number}")],
            artist: Some("Some Artist".to_owned()),
            tags: vec!["hymn".to_owned(), "slow".to_owned()],
            lyrics: format!(
                "{}unique{number}",
                "amazing grace how sweet the sound that saved a wretch like me ".repeat(8)
            ),
        })
        .collect()
}

fn milliseconds(work: impl FnOnce()) -> f64 {
    let started = js_sys::Date::now();
    work();

    js_sys::Date::now() - started
}

/// A frame is 16 ms. Building is well over it and querying is well under, which is exactly why
/// the index is built in a worker and queried from wherever the answer is needed.
#[wasm_bindgen_test]
fn building_the_index_costs_more_than_a_frame_and_querying_costs_none() {
    let songs = library(1000);
    let mut index = None;

    let build = milliseconds(|| index = Some(SearchIndex::build(&songs)));
    let index = index.expect("an index");

    let query = milliseconds(|| {
        let hits = index.search("unique512", 50);
        assert_eq!(hits[0].id, "song-512");
    });

    web_sys::console::log_1(
        &format!("index of 1,000 songs: build {build} ms, query {query} ms").into(),
    );

    assert!(
        build > 16.0,
        "building took only {build} ms — if that holds, the worker is a moving part to delete"
    );
    assert!(
        query < 16.0,
        "querying took {query} ms, which is a frame: the query would have to move too"
    );
}
