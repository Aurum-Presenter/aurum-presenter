//! The local database, in a real browser.
//!
//! IndexedDB cannot be tested against a stub without testing the stub, so these run in headless
//! Chromium: `wasm-pack test --headless --chrome -p aurum-web`, or
//! `CHROMEDRIVER=$(which chromedriver) cargo test -p aurum-web --target wasm32-unknown-unknown`.

#![cfg(target_arch = "wasm32")]

use aurum_web::db::{Database, live, schema};
use serde::{Deserialize, Serialize};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Song {
    id: String,
    title: String,
    folder_id: Option<String>,
    change_seq: i64,
}

fn song(id: &str, title: &str, folder: Option<&str>) -> Song {
    Song {
        id: id.to_owned(),
        title: title.to_owned(),
        folder_id: folder.map(str::to_owned),
        change_seq: 1,
    }
}

/// A fresh database per test, so one cannot see another's rows.
async fn database() -> Database {
    let name = format!(
        "test-{}",
        js_sys::Math::floor(js_sys::Math::random() * 1e12) as u64
    );

    Database::open(&name).await.expect("a database")
}

#[wasm_bindgen_test]
async fn writes_a_record_and_reads_it_back() {
    let db = database().await;

    db.put("songs", &song("s1", "Amazing Grace", None))
        .await
        .expect("a write");

    let read: Option<Song> = db.get("songs", "s1").await.expect("a read");

    assert_eq!(read, Some(song("s1", "Amazing Grace", None)));
}

#[wasm_bindgen_test]
async fn a_key_that_is_not_there_is_none_rather_than_an_error() {
    let db = database().await;

    let read: Option<Song> = db.get("songs", "nobody").await.expect("a read");

    assert_eq!(read, None);
}

#[wasm_bindgen_test]
async fn writing_the_same_id_replaces_rather_than_duplicates() {
    let db = database().await;

    db.put("songs", &song("s1", "First", None))
        .await
        .expect("a write");
    db.put("songs", &song("s1", "Second", None))
        .await
        .expect("a write");

    let all: Vec<Song> = db.all("songs").await.expect("a read");

    assert_eq!(all.len(), 1);
    assert_eq!(all[0].title, "Second");
}

/// The shape of nearly every read the app makes: the songs of a folder, the items of a set.
#[wasm_bindgen_test]
async fn reads_by_index() {
    let db = database().await;

    for (id, folder) in [("s1", Some("f1")), ("s2", Some("f1")), ("s3", Some("f2"))] {
        db.put("songs", &song(id, "A song", folder))
            .await
            .expect("a write");
    }

    let mut in_folder: Vec<Song> = db
        .by_index("songs", "folder_id", &wasm_bindgen::JsValue::from_str("f1"))
        .await
        .expect("a read");
    in_folder.sort_by(|a, b| a.id.cmp(&b.id));

    assert_eq!(
        in_folder
            .iter()
            .map(|song| song.id.as_str())
            .collect::<Vec<_>>(),
        ["s1", "s2"]
    );
}

/// A pull brings hundreds of rows; writing them one transaction at a time would be slow and
/// would wake every list hundreds of times.
#[wasm_bindgen_test]
async fn writes_many_records_in_one_go() {
    let db = database().await;
    let songs: Vec<Song> = (0..200)
        .map(|number| song(&format!("s{number}"), "A song", None))
        .collect();

    db.put_all("songs", &songs).await.expect("a batch");

    assert_eq!(db.count("songs").await.expect("a count"), 200);

    // An empty batch is a no-op, not an error: a pull that brought nothing is ordinary.
    db.put_all::<Song>("songs", &[])
        .await
        .expect("an empty batch");
}

#[wasm_bindgen_test]
async fn deletes_and_clears() {
    let db = database().await;

    db.put("songs", &song("s1", "Amazing Grace", None))
        .await
        .expect("a write");
    db.put("songs", &song("s2", "The First Noel", None))
        .await
        .expect("a write");

    db.delete("songs", &wasm_bindgen::JsValue::from_str("s1"))
        .await
        .expect("a delete");

    assert_eq!(db.count("songs").await.expect("a count"), 1);

    db.clear("songs").await.expect("a clear");

    assert_eq!(db.count("songs").await.expect("a count"), 0);
}

/// Every store the schema declares has to actually be there, or a screen fails on the one read
/// nobody exercised before shipping.
#[wasm_bindgen_test]
async fn every_declared_store_exists() {
    let db = database().await;

    for store in schema::STORES {
        assert!(
            db.count(store.name).await.is_ok(),
            "{} is declared but not in the database",
            store.name
        );
    }
}

/// The outbox is the one store that generates its own keys, and the order it generates them in
/// is what lets it be drained exactly as the musician made the changes.
#[wasm_bindgen_test]
async fn the_outbox_keeps_the_order_things_were_written_in() {
    #[derive(Deserialize, Serialize)]
    struct Op {
        op_id: String,
        status: String,
        created_at: String,
    }

    #[derive(Deserialize)]
    struct Stored {
        seq: i64,
        op_id: String,
    }

    let db = database().await;

    for number in 0..5 {
        db.put(
            "outbox",
            &Op {
                op_id: format!("op-{number}"),
                status: "pending".to_owned(),
                created_at: "2026-09-08T00:00:00.000Z".to_owned(),
            },
        )
        .await
        .expect("a write");
    }

    let mut stored: Vec<Stored> = db.all("outbox").await.expect("a read");
    stored.sort_by_key(|op| op.seq);

    assert_eq!(
        stored
            .iter()
            .map(|op| op.op_id.as_str())
            .collect::<Vec<_>>(),
        ["op-0", "op-1", "op-2", "op-3", "op-4"]
    );
}

/// The whole point of the live layer: a write has to move something a screen is watching.
#[wasm_bindgen_test]
async fn a_write_wakes_a_query_over_that_store() {
    use leptos::prelude::*;

    // Effects run on the executor `mount_to_body` would normally have started.
    let _ = any_spawner::Executor::init_wasm_bindgen();

    let owner = Owner::new();
    owner.set();

    let runs = RwSignal::new(0);
    let effect = Effect::new(move |_| {
        live::watching(&["songs"]);
        runs.update(|count| *count += 1);
    });

    // Effects run on a microtask, so the first pass has to be allowed to happen.
    gloo_timers::future::TimeoutFuture::new(10).await;

    let before = runs.get_untracked();

    live::changed("test-workspace", "songs");
    gloo_timers::future::TimeoutFuture::new(10).await;

    assert!(
        runs.get_untracked() > before,
        "the query did not re-run after a write to songs"
    );

    // A write to a store this query does not read must not wake it.
    let after = runs.get_untracked();
    live::changed("test-workspace", "sets");
    gloo_timers::future::TimeoutFuture::new(10).await;

    assert_eq!(runs.get_untracked(), after, "an unrelated store woke it");

    // Kept alive to here: dropping the handle earlier would stop the effect being run.
    let _ = effect;
}

/// The bug this exists to stop coming back: a row that arrives as a map, not a struct.
///
/// `serde_wasm_bindgen` turns a map into an ES `Map` by default, and IndexedDB cannot read a key
/// path out of one — so the write fails with a bare DataError and nothing is stored. Struct
/// records serialise as plain objects either way, which is why every test above passed while
/// every write the sync engine made was failing.
#[wasm_bindgen_test]
async fn writes_a_record_that_arrived_as_a_map() {
    let db = database().await;
    let mut row = serde_json::Map::new();

    row.insert("id".to_owned(), serde_json::json!("s1"));
    row.insert("title".to_owned(), serde_json::json!("Amazing Grace"));
    row.insert("change_seq".to_owned(), serde_json::json!(0));

    db.put("songs", &row).await.expect("a write");

    let read: Option<Song> = db.get("songs", "s1").await.expect("a read");

    assert_eq!(
        read.map(|song| song.title),
        Some("Amazing Grace".to_owned())
    );
}
