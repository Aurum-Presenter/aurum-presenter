//! What the browser cannot see about sync.
//!
//! The end-to-end suite proves a musician's edit arrives. These prove the things underneath it:
//! that the watermark has no gaps, that a retried batch does not apply twice, that a tombstone
//! beats a stale edit, and that one bad operation parks alone rather than failing the push.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{id, server, upsert};

#[tokio::test]
async fn a_push_inserts_and_a_pull_returns_the_row() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let song = id();

    let pushed = server
        .post(
            &format!("/api/v1/workspaces/{}/sync/push", ada.workspace),
            &ada.token,
            json!({ "ops": [upsert("songs", &song, json!({ "title": "Amazing Grace" }))] }),
        )
        .await;

    assert_eq!(pushed.status, StatusCode::OK, "{:?}", pushed.body);
    assert_eq!(pushed.body["results"][0]["status"], "applied");

    let pulled = server
        .get(
            &format!("/api/v1/workspaces/{}/sync/pull?since=0", ada.workspace),
            &ada.token,
        )
        .await;

    assert_eq!(pulled.body["tables"]["songs"][0]["title"], "Amazing Grace");
    assert_eq!(pulled.body["tables"]["songs"][0]["id"], song.as_str());
    // The server owns these, and stamps them.
    assert_eq!(
        pulled.body["tables"]["songs"][0]["updated_by"],
        ada.id.as_str()
    );
    assert!(
        pulled.body["tables"]["songs"][0]["change_seq"]
            .as_i64()
            .unwrap()
            > 0
    );
}

/// The watermark protocol only works if the counter has no gaps and never goes backwards.
#[tokio::test]
async fn change_sequence_is_monotonic_and_gap_free() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let mut seen: Vec<i64> = Vec::new();

    for batch in 0..5 {
        let ops: Vec<_> = (0..4)
            .map(|item| {
                upsert(
                    "songs",
                    &id(),
                    json!({ "title": format!("{batch}-{item}") }),
                )
            })
            .collect();

        let pushed = server
            .post(
                &format!("/api/v1/workspaces/{}/sync/push", ada.workspace),
                &ada.token,
                json!({ "ops": ops }),
            )
            .await;

        for result in pushed.body["results"].as_array().expect("results") {
            seen.push(result["change_seq"].as_i64().expect("a sequence"));
        }
    }

    let first = seen[0];

    assert_eq!(
        seen,
        (first..first + seen.len() as i64).collect::<Vec<_>>(),
        "every value in order, none skipped"
    );
}

#[tokio::test]
async fn pull_by_watermark_returns_only_newer_rows() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let path = format!("/api/v1/workspaces/{}/sync", ada.workspace);

    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [upsert("songs", &id(), json!({ "title": "First" }))] }),
        )
        .await;

    let watermark = server
        .get(&format!("{path}/pull?since=0"), &ada.token)
        .await
        .body["change_seq"]
        .as_i64()
        .expect("a watermark");

    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [upsert("songs", &id(), json!({ "title": "Second" }))] }),
        )
        .await;

    let delta = server
        .get(
            &format!("{path}/pull?since={watermark}&tables=songs"),
            &ada.token,
        )
        .await;

    assert_eq!(delta.body["tables"]["songs"].as_array().unwrap().len(), 1);
    assert_eq!(delta.body["tables"]["songs"][0]["title"], "Second");
}

/// A retried batch after a lost response must not apply twice.
#[tokio::test]
async fn a_replayed_operation_is_recognised_as_a_duplicate() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let path = format!("/api/v1/workspaces/{}/sync/push", ada.workspace);
    let op = upsert("songs", &id(), json!({ "title": "Amazing Grace" }));

    let first = server.post(&path, &ada.token, json!({ "ops": [op] })).await;
    let again = server.post(&path, &ada.token, json!({ "ops": [op] })).await;

    assert_eq!(first.body["results"][0]["status"], "applied");
    assert_eq!(again.body["results"][0]["status"], "duplicate");
}

/// Sync business rule 6. Resurrecting a deleted record is not something a stale offline edit
/// gets to do by accident.
#[tokio::test]
async fn a_tombstone_wins_over_a_later_edit() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let path = format!("/api/v1/workspaces/{}/sync", ada.workspace);
    let song = id();

    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [upsert("songs", &song, json!({ "title": "Amazing Grace" }))] }),
        )
        .await;

    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [{ "op_id": id(), "table": "songs", "record_id": song, "op": "delete" }] }),
        )
        .await;

    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [upsert("songs", &song, json!({ "title": "Edited offline" }))] }),
        )
        .await;

    let rows = server
        .get(&format!("{path}/pull?since=0&tables=songs"), &ada.token)
        .await;

    assert_eq!(rows.body["tables"]["songs"][0]["title"], "Amazing Grace");
    assert!(!rows.body["tables"]["songs"][0]["deleted_at"].is_null());
}

/// The incoming write wins, and the value it displaced is kept so the review panel can offer it
/// back. Nothing is silently destroyed.
#[tokio::test]
async fn a_concurrent_edit_records_the_displaced_value() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let path = format!("/api/v1/workspaces/{}/sync", ada.workspace);
    let song = id();

    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [upsert("songs", &song, json!({ "title": "Amazing Grace" }))] }),
        )
        .await;

    let base = server
        .get(&format!("{path}/pull?since=0&tables=songs"), &ada.token)
        .await
        .body["tables"]["songs"][0]["updated_at"]
        .as_str()
        .expect("a timestamp")
        .to_owned();

    // The first device writes, moving the row on.
    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [upsert("songs", &song, json!({ "title": "From the phone" }))] }),
        )
        .await;

    // The second arrives late, still editing from the base it last read.
    let late = server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [{
                "op_id": id(),
                "table": "songs",
                "record_id": song,
                "op": "upsert",
                "base_updated_at": base,
                "payload": { "title": "From the laptop" },
            }] }),
        )
        .await;

    assert_eq!(late.body["results"][0]["conflicts"], json!(["title"]));

    let conflicts = server.get(&format!("{path}/conflicts"), &ada.token).await;

    assert_eq!(conflicts.body["conflicts"][0]["field"], "title");
    assert_eq!(
        conflicts.body["conflicts"][0]["losing_value"],
        "From the phone"
    );

    let rows = server
        .get(&format!("{path}/pull?since=0&tables=songs"), &ada.token)
        .await;

    assert_eq!(rows.body["tables"]["songs"][0]["title"], "From the laptop");
}

/// A client that could write `change_seq` could move every other device's pull watermark.
#[tokio::test]
async fn server_owned_columns_cannot_be_set_by_a_push() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let path = format!("/api/v1/workspaces/{}/sync", ada.workspace);
    let song = id();

    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [upsert("songs", &song, json!({
                "title": "Amazing Grace",
                "change_seq": 9_999,
                "updated_by": "somebody-else",
                "deleted_at": "2020-01-01T00:00:00.000Z",
            }))] }),
        )
        .await;

    let row = &server
        .get(&format!("{path}/pull?since=0&tables=songs"), &ada.token)
        .await
        .body["tables"]["songs"][0];

    assert!(row["change_seq"].as_i64().unwrap() < 9_999);
    assert_eq!(row["updated_by"], ada.id.as_str());
    assert!(row["deleted_at"].is_null());
}

/// One bad operation parks alone. Twenty-eight good edits must not be lost because one was odd.
#[tokio::test]
async fn an_unknown_table_is_rejected_without_affecting_the_rest_of_the_batch() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let good = id();

    let pushed = server
        .post(
            &format!("/api/v1/workspaces/{}/sync/push", ada.workspace),
            &ada.token,
            json!({ "ops": [
                upsert("users", &id(), json!({ "email": "somebody@example.com" })),
                upsert("songs", &good, json!({ "title": "Still applied" })),
            ] }),
        )
        .await;

    assert_eq!(pushed.body["results"][0]["status"], "rejected");
    assert_eq!(pushed.body["results"][0]["code"], "unknown_table");
    assert_eq!(pushed.body["results"][1]["status"], "applied");
}

/// A row the schema refuses is one bad operation, not a broken batch.
#[tokio::test]
async fn a_row_the_schema_refuses_parks_without_failing_the_batch() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    let pushed = server
        .post(
            &format!("/api/v1/workspaces/{}/sync/push", ada.workspace),
            &ada.token,
            json!({ "ops": [
                // A capo outside 0–11, which the schema checks.
                upsert("set_items", &id(), json!({ "set_id": id(), "rank": "a", "capo_override": 99 })),
                upsert("songs", &id(), json!({ "title": "Still applied" })),
            ] }),
        )
        .await;

    assert_eq!(pushed.body["results"][0]["status"], "rejected");
    assert_eq!(pushed.body["results"][1]["status"], "applied");
}

#[tokio::test]
async fn a_push_may_not_carry_more_than_five_hundred_operations() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    let ops: Vec<_> = (0..501)
        .map(|_| upsert("songs", &id(), json!({ "title": "Too many" })))
        .collect();

    let pushed = server
        .post(
            &format!("/api/v1/workspaces/{}/sync/push", ada.workspace),
            &ada.token,
            json!({ "ops": ops }),
        )
        .await;

    assert_eq!(pushed.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(pushed.code(), "batch_too_large");
}

/// Exactly one default arrangement per song, and the demotion travels with the same change
/// sequence so every device pulls it as one change.
#[tokio::test]
async fn making_one_arrangement_the_default_demotes_the_other() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let path = format!("/api/v1/workspaces/{}/sync", ada.workspace);
    let (song, first, second) = (id(), id(), id());

    server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [
                upsert("songs", &song, json!({ "title": "Amazing Grace" })),
                upsert("arrangements", &first, json!({
                    "song_id": song, "name": "Original", "body": "[G]", "is_default": 1,
                })),
            ] }),
        )
        .await;

    let promoted = server
        .post(
            &format!("{path}/push"),
            &ada.token,
            json!({ "ops": [upsert("arrangements", &second, json!({
                "song_id": song, "name": "Acoustic", "body": "[D]", "is_default": 1,
            }))] }),
        )
        .await;

    let seq = promoted.body["results"][0]["change_seq"]
        .as_i64()
        .expect("a sequence");
    let rows = server
        .get(
            &format!("{path}/pull?since=0&tables=arrangements"),
            &ada.token,
        )
        .await;
    let arrangements = rows.body["tables"]["arrangements"]
        .as_array()
        .expect("rows");

    let defaults: Vec<&str> = arrangements
        .iter()
        .filter(|row| row["is_default"].as_i64() == Some(1))
        .map(|row| row["id"].as_str().unwrap_or_default())
        .collect();

    assert_eq!(
        defaults,
        [second.as_str()],
        "exactly one, and it is the new one"
    );

    let demoted = arrangements
        .iter()
        .find(|row| row["id"] == first.as_str())
        .expect("the old default");

    assert_eq!(
        demoted["change_seq"].as_i64(),
        Some(seq),
        "the demotion travels with it"
    );
}

/// Preferences are personal. Every member has their own row for the same song, and no member is
/// ever handed anyone else's.
#[tokio::test]
async fn each_member_pulls_only_their_own_preferences() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let grace = server.account("grace@example.com").await;
    let band = server.band(&ada, "The Band").await;

    // Grace joins as an editor.
    let invited = server
        .post(
            &format!("/api/v1/workspaces/{band}/invites"),
            &ada.token,
            json!({ "email": grace.email, "role": "editor" }),
        )
        .await;
    let link = invited.body["link"].as_str().expect("a link");
    let token = link.rsplit('/').next().expect("a token").to_owned();

    server
        .post(
            "/api/v1/invites/accept",
            &grace.token,
            json!({ "token": token }),
        )
        .await;

    for (account, value) in [(&ada, "G"), (&grace, "Bb")] {
        server
            .post(
                &format!("/api/v1/workspaces/{band}/sync/push"),
                &account.token,
                json!({ "ops": [upsert("preferences", &id(), json!({
                    "user_id": account.id,
                    "scope_type": "song",
                    "scope_id": "a-song",
                    "name": "chart",
                    "value": format!("{{\"preferred_key\":\"{value}\"}}"),
                }))] }),
            )
            .await;
    }

    for (account, expected) in [(&ada, "G"), (&grace, "Bb")] {
        let pulled = server
            .get(
                &format!("/api/v1/workspaces/{band}/sync/pull?since=0&tables=preferences"),
                &account.token,
            )
            .await;
        let rows = pulled.body["tables"]["preferences"]
            .as_array()
            .expect("rows");

        assert_eq!(rows.len(), 1, "only their own");
        assert!(rows[0]["value"].as_str().unwrap().contains(expected));
        assert_eq!(rows[0]["user_id"], account.id.as_str());
    }
}

/// Not a role question: an owner has no more business writing another member's preferred key
/// than a viewer does.
#[tokio::test]
async fn writing_another_members_preference_is_rejected() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;

    let pushed = server
        .post(
            &format!("/api/v1/workspaces/{}/sync/push", ada.workspace),
            &ada.token,
            json!({ "ops": [upsert("preferences", &id(), json!({
                "user_id": "01890000-0000-7000-8000-000000000009",
                "name": "chart",
                "value": "{}",
            }))] }),
        )
        .await;

    assert_eq!(pushed.body["results"][0]["status"], "rejected");
    assert_eq!(pushed.body["results"][0]["code"], "not_your_row");
}
