//! What has to be true of the files themselves.
//!
//! The per-workspace split is the design's safety argument, and it only holds if the files
//! really are what they claim: the pragmas applied, no `workspace_id` anywhere to forget, every
//! allowlisted column present, and a counter that does not skip or repeat under load.

mod support;

use aurum_core::sync::schema;
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::json;
use support::{id, server, upsert};

#[tokio::test]
async fn a_created_workspace_file_is_sound() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    assert!(server.state.db.workspace_exists(&ada.workspace));

    let db = server
        .state
        .db
        .open_workspace(&ada.workspace)
        .expect("the file");

    for (pragma, expected) in [
        ("journal_mode", "wal"),
        ("foreign_keys", "1"),
        ("synchronous", "1"),
    ] {
        let value: String = db
            .query_row(&format!("PRAGMA {pragma}"), [], |row| {
                row.get_ref(0).map(|value| match value {
                    rusqlite::types::ValueRef::Text(bytes) => {
                        String::from_utf8_lossy(bytes).to_lowercase()
                    }
                    rusqlite::types::ValueRef::Integer(number) => number.to_string(),
                    _ => String::new(),
                })
            })
            .expect("a pragma");

        assert_eq!(value, expected, "{pragma}");
    }
}

/// The whole point of the split: a query cannot omit a predicate that does not exist.
#[tokio::test]
async fn no_content_table_carries_a_workspace_id() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let db = server
        .state
        .db
        .open_workspace(&ada.workspace)
        .expect("the file");

    for table in schema::tables() {
        assert!(
            !columns_of(&db, table)
                .iter()
                .any(|column| column == "workspace_id"),
            "{table} carries a workspace_id, which is a predicate somebody can forget"
        );
    }
}

/// A column the allowlist names but the schema does not have would be an insert that fails at
/// runtime, on somebody's edit.
#[tokio::test]
async fn every_allowlisted_column_exists_in_the_schema() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let db = server
        .state
        .db
        .open_workspace(&ada.workspace)
        .expect("the file");

    for table in schema::tables() {
        let present = columns_of(&db, table);

        for column in schema::columns(table).expect("a synced table") {
            assert!(
                present.iter().any(|name| name == column),
                "{table}.{column} is allowlisted but not in the schema"
            );
        }

        for owned in schema::SYNC_COLUMNS {
            assert!(
                present.iter().any(|name| name == owned),
                "{table} has no {owned}, which sync writes on every row"
            );
        }
    }
}

#[tokio::test]
async fn migrating_twice_changes_nothing() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    let again = server
        .state
        .db
        .migrate_workspace(&ada.workspace)
        .expect("a migration");

    assert!(again.is_empty(), "a second pass applied {again:?}");
}

/// One default theme, seeded by the migration, and not seeded again on reopening.
#[tokio::test]
async fn the_default_theme_is_seeded_once() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    server
        .state
        .db
        .migrate_workspace(&ada.workspace)
        .expect("a migration");

    let db = server
        .state
        .db
        .open_workspace(&ada.workspace)
        .expect("the file");
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM presenter_themes WHERE is_default = 1",
            [],
            |row| row.get(0),
        )
        .expect("a count");

    assert_eq!(count, 1);
}

#[tokio::test]
async fn deleting_a_workspace_leaves_no_file_behind() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    // Write enough to be sure WAL has sidecars to leave behind.
    server
        .post(
            &format!("/api/v1/workspaces/{}/sync/push", ada.workspace),
            &ada.token,
            json!({ "ops": [upsert("songs", &id(), json!({ "title": "Amazing Grace" }))] }),
        )
        .await;

    server.state.db.delete_workspace(&ada.workspace);

    for suffix in ["", "-wal", "-shm"] {
        let path = format!(
            "{}{suffix}",
            server.state.db.workspace_path(&ada.workspace).display()
        );

        assert!(
            !std::path::Path::new(&path).exists(),
            "{path} is still there, and its last transactions with it"
        );
    }
}

/// `BEGIN IMMEDIATE` is what makes this true. With a deferred transaction two writers read the
/// same counter and one fails on upgrade instead of waiting.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn twenty_concurrent_writers_all_commit_with_no_gaps() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let path = format!("/api/v1/workspaces/{}/sync/push", ada.workspace);

    let borrowed = &server;
    let pushes = (0..20).map(|number| {
        let (path, token) = (path.clone(), ada.token.clone());

        async move {
            borrowed
                .post(
                    &path,
                    &token,
                    json!({ "ops": [upsert("songs", &id(), json!({ "title": format!("Song {number}") }))] }),
                )
                .await
        }
    });

    let answers = futures_util::future::join_all(pushes).await;
    let mut sequences: Vec<i64> = answers
        .iter()
        .map(|answer| {
            assert_eq!(answer.status, StatusCode::OK, "{:?}", answer.body);
            assert_eq!(answer.body["results"][0]["status"], "applied");

            answer.body["results"][0]["change_seq"]
                .as_i64()
                .expect("a sequence")
        })
        .collect();

    sequences.sort_unstable();
    sequences.dedup();

    assert_eq!(
        sequences.len(),
        20,
        "two writers took the same sequence value"
    );

    let first = sequences[0];

    assert_eq!(
        sequences,
        (first..first + 20).collect::<Vec<_>>(),
        "the counter skipped a value"
    );
}

/// A sequence burns a value on rollback; this counter rolls back with the transaction, so a
/// refused batch leaves no hole for a client to wait forever on.
#[tokio::test]
async fn a_rolled_back_batch_does_not_consume_a_sequence_value() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let db = server
        .state
        .db
        .open_workspace(&ada.workspace)
        .expect("the file");

    let before: i64 = db
        .query_row("SELECT seq FROM sync_counter WHERE id = 1", [], |row| {
            row.get(0)
        })
        .expect("the counter");

    // A payload the schema refuses inside a batch that otherwise applies: the whole batch's
    // sequence block is reserved, but a batch that fails outright must give it back.
    let mut writer = server
        .state
        .db
        .open_workspace(&ada.workspace)
        .expect("the file");

    let result: Result<(), aurum_api::error::ApiError> =
        aurum_api::db::write_batch(&mut writer, 5, |_, _| {
            Err(aurum_api::error::ApiError::conflict(
                "deliberately abandoned",
            ))
        });

    assert!(result.is_err());

    let after: i64 = db
        .query_row("SELECT seq FROM sync_counter WHERE id = 1", [], |row| {
            row.get(0)
        })
        .expect("the counter");

    assert_eq!(after, before, "the abandoned batch kept its five values");
}

fn columns_of(db: &Connection, table: &str) -> Vec<String> {
    let mut statement = db
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .expect("a table");

    statement
        .query_map([], |row| row.get(0))
        .expect("columns")
        .collect::<Result<Vec<String>, _>>()
        .expect("columns")
}
