//! The allowlist of synced tables and their client-writable columns.
//!
//! Nothing derived from a request is ever interpolated into SQL without passing through here
//! first. Column names cannot be bound as parameters, so an allowlist is the only safe way to
//! accept a client-supplied field set — and because both halves read this same list, the client
//! cannot come to believe in a column the server will not take.

use serde_json::{Map, Value};

/// Columns the server owns on every synced table. A client push may never set these.
pub const SYNC_COLUMNS: [&str; 4] = ["updated_at", "change_seq", "deleted_at", "updated_by"];

/// `sheets.sha256`, `size` and `uploaded_at` are deliberately absent: they are set by the
/// upload-completion endpoint once the server has verified the stored object's checksum. A sync
/// push that could set them would be a client claiming a content hash it never uploaded.
const TABLES: [(&str, &[&str]); 10] = [
    ("folders", &["parent_id", "name", "position"]),
    (
        "songs",
        &[
            "folder_id",
            "title",
            "subtitle",
            "authors",
            "ccli_number",
            "copyright",
            "notes",
            "original_key",
            "tempo",
            "time_signature",
            "tags",
            "artist",
            "alt_titles",
            "duration_sec",
            "archived",
        ],
    ),
    (
        "arrangements",
        &[
            "song_id",
            "name",
            "body",
            "default_key",
            "is_default",
            "position",
            "source_notation",
            "source_text",
            "capo_hint",
        ],
    ),
    ("song_placements", &["song_id", "folder_id"]),
    (
        "sheets",
        &[
            "song_id",
            "sheet_key",
            "part",
            "position",
            "page_count",
            "mime_type",
            "arrangement_id",
            "label",
            "filename",
            "pages_changed_at",
        ],
    ),
    (
        "annotations",
        &["sheet_id", "page", "strokes", "scope", "author_id"],
    ),
    (
        "sets",
        &[
            "name",
            "scheduled_for",
            "notes",
            "venue",
            "assigned_members",
            "pinned",
        ],
    ),
    (
        "set_items",
        &[
            "set_id",
            "rank",
            "song_id",
            "item_type",
            "content",
            "title_snapshot",
            "key_override",
            "capo_override",
            "arrangement_id",
            "sheet_part_override",
            "sections",
            "note",
        ],
    ),
    (
        "preferences",
        &["user_id", "scope_type", "scope_id", "name", "value"],
    ),
    (
        "presenter_themes",
        &[
            "name",
            "is_default",
            "font_family",
            "font_size_vh",
            "text_color",
            "background_kind",
            "background_value",
            "align",
            "safe_area_pct",
            "show_section_labels",
        ],
    ),
];

/// Tables a `viewer` may write, because the rows are their own and no other member ever sees
/// them. Everything else needs `workspace.write`.
const VIEWER_WRITABLE: [&str; 2] = ["preferences", "annotations"];

/// Every synced table, in the order a full pull walks them.
pub fn tables() -> Vec<&'static str> {
    TABLES.iter().map(|(name, _)| *name).collect()
}

pub fn is_synced_table(table: &str) -> bool {
    TABLES.iter().any(|(name, _)| *name == table)
}

/// The columns a client may write on this table, or `None` if there is no such synced table.
pub fn columns(table: &str) -> Option<&'static [&'static str]> {
    TABLES
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, columns)| *columns)
}

pub fn is_viewer_writable(table: &str) -> bool {
    VIEWER_WRITABLE.contains(&table)
}

/// Only the columns this table actually accepts. Anything else the client sent is dropped
/// silently — a push carrying a field we do not know is a client from the future, not an attack
/// to reject, and refusing it would cost the user the rest of their edit.
pub fn filter(table: &str, payload: &Map<String, Value>) -> Map<String, Value> {
    let Some(allowed) = columns(table) else {
        return Map::new();
    };

    payload
        .iter()
        .filter(|(field, _)| allowed.contains(&field.as_str()))
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload(value: Value) -> Map<String, Value> {
        value.as_object().expect("an object").clone()
    }

    #[test]
    fn knows_which_tables_sync() {
        assert_eq!(tables().len(), 10);
        assert!(is_synced_table("songs"));
        assert!(
            !is_synced_table("users"),
            "accounts are not workspace content"
        );
        assert!(
            !is_synced_table("sync_conflicts"),
            "the server's own bookkeeping"
        );
        assert_eq!(columns("nope"), None);
    }

    #[test]
    fn drops_a_field_the_table_does_not_have() {
        let filtered = filter(
            "songs",
            &payload(json!({ "title": "Amazing Grace", "made_up": 1 })),
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered["title"], json!("Amazing Grace"));
    }

    /// The rule that stops a client rewriting sync's own bookkeeping — and with it the pull
    /// watermark every other device depends on.
    #[test]
    fn no_table_lets_a_client_write_the_columns_the_server_owns() {
        for table in tables() {
            let filtered = filter(
                table,
                &payload(json!({
                    "updated_at": "2000-01-01T00:00:00.000Z",
                    "change_seq": 1,
                    "deleted_at": null,
                    "updated_by": "someone-else"
                })),
            );

            assert!(
                filtered.is_empty(),
                "{table} accepted a server-owned column"
            );

            for owned in SYNC_COLUMNS {
                assert!(
                    !columns(table).unwrap().contains(&owned),
                    "{table} lists {owned} as client-writable"
                );
            }
        }
    }

    /// A verified content hash is not something a client gets to assert.
    #[test]
    fn a_push_cannot_claim_a_sheet_it_never_uploaded() {
        let filtered = filter(
            "sheets",
            &payload(
                json!({ "sha256": "ab".repeat(32), "size": 17, "uploaded_at": "now", "part": "piano" }),
            ),
        );

        assert_eq!(filtered.keys().collect::<Vec<_>>(), ["part"]);
    }

    #[test]
    fn a_viewer_writes_only_their_own_rows() {
        assert!(is_viewer_writable("preferences"));
        assert!(is_viewer_writable("annotations"));
        assert!(!is_viewer_writable("songs"));
        assert!(!is_viewer_writable("sets"));
    }

    #[test]
    fn filtering_an_unknown_table_yields_nothing_rather_than_everything() {
        assert!(filter("users", &payload(json!({ "password_hash": "x" }))).is_empty());
    }
}
