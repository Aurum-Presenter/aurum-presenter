//! What happens when two devices wrote the same row.
//!
//! Every decision here is made from the incoming operation and the stored row alone — no clock,
//! no database. That matters because these are the rules a musician's work survives or does not:
//! the one thing sync is not allowed to do is lose an edit somebody made.

use serde_json::{Map, Value};

use super::schema;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Upsert,
    Delete,
}

/// One operation from a client's outbox, as far as the merge rules care about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Op<'a> {
    pub table: &'a str,
    pub kind: OpKind,
    pub payload: Map<String, Value>,
    /// The `updated_at` the client edited from. Absent on a row the client created.
    pub base_updated_at: Option<&'a str>,
}

/// The stored row, if there is one.
#[derive(Clone, Debug, PartialEq)]
pub struct Existing<'a> {
    pub updated_at: &'a str,
    pub deleted_at: Option<&'a str>,
    pub updated_by: Option<&'a str>,
    pub fields: &'a Map<String, Value>,
}

/// A value this write displaced, kept verbatim so the review panel can offer it back.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub field: String,
    pub losing_value: Value,
    pub losing_user: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Write a new row with this payload.
    Insert(Map<String, Value>),
    /// Overwrite these fields on the stored row.
    Update(Map<String, Value>),
    /// Tombstone the stored row.
    Delete,
    /// Do nothing, and say why — both of these are ordinary, neither is an error.
    Skip(Skipped),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skipped {
    /// An upsert onto a row that is already deleted. A tombstone wins over a concurrent edit
    /// (sync business rule 6): resurrecting a deleted record is not something a stale offline
    /// edit gets to do by accident.
    Tombstoned,
    /// A delete of something this server never had. The outcome the client wants — the record
    /// does not exist — already holds.
    AlreadyGone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub action: Action,
    /// Fields whose previous value must be recorded before the write lands.
    pub conflicts: Vec<Conflict>,
    /// Exactly one default arrangement per song: the song whose *other* arrangements this write
    /// demotes. Two people offline can each make a different arrangement the default, and both
    /// are legitimate edits — so the later one demotes the others rather than being refused.
    pub demote_defaults_of: Option<String>,
}

/// Decides what one operation does to one row.
pub fn plan(op: &Op<'_>, existing: Option<&Existing<'_>>) -> Plan {
    let nothing = |skipped: Skipped| Plan {
        action: Action::Skip(skipped),
        conflicts: Vec::new(),
        demote_defaults_of: None,
    };

    if op.kind == OpKind::Delete {
        return match existing {
            Some(_) => Plan {
                action: Action::Delete,
                conflicts: Vec::new(),
                demote_defaults_of: None,
            },
            None => nothing(Skipped::AlreadyGone),
        };
    }

    let payload = encode(&schema::filter(op.table, &op.payload));

    let Some(existing) = existing else {
        return Plan {
            demote_defaults_of: demoted_song(op.table, &payload, None),
            action: Action::Insert(payload),
            conflicts: Vec::new(),
        };
    };

    if existing.deleted_at.is_some() {
        return nothing(Skipped::Tombstoned);
    }

    Plan {
        conflicts: conflicts(&payload, op.base_updated_at, existing),
        demote_defaults_of: demoted_song(op.table, &payload, Some(existing)),
        action: Action::Update(payload),
    }
}

/// A field is in conflict when the stored row moved on after the base the client edited from,
/// and the stored value differs from what is arriving. The incoming write still wins — it is the
/// later one by the server's clock — but the value it displaces is kept. Nothing is silently
/// destroyed.
fn conflicts(
    payload: &Map<String, Value>,
    base_updated_at: Option<&str>,
    existing: &Existing<'_>,
) -> Vec<Conflict> {
    // No base means the client is not claiming to have read this row, and a row that has not
    // moved since the client read it cannot have lost anything.
    if base_updated_at.is_none_or(|base| existing.updated_at <= base) {
        return Vec::new();
    }

    payload
        .iter()
        .filter_map(|(field, incoming)| {
            let current = existing.fields.get(field).unwrap_or(&Value::Null);

            if as_text(current) == as_text(incoming) {
                return None;
            }

            Some(Conflict {
                field: field.clone(),
                losing_value: current.clone(),
                losing_user: existing.updated_by.map(str::to_owned),
            })
        })
        .collect()
}

/// From the payload when the row is being created, from the stored row when it is being updated:
/// a push that only flips the flag does not carry the song.
fn demoted_song(
    table: &str,
    payload: &Map<String, Value>,
    existing: Option<&Existing<'_>>,
) -> Option<String> {
    if table != "arrangements" || as_text(payload.get("is_default")?) != "1" {
        return None;
    }

    let song = payload
        .get("song_id")
        .or_else(|| existing?.fields.get("song_id"))?;

    match song {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        _ => None,
    }
}

/// A structured value is stored as its JSON text, because the column is TEXT and both halves
/// have to agree byte for byte on what went in.
fn encode(payload: &Map<String, Value>) -> Map<String, Value> {
    payload
        .iter()
        .map(|(field, value)| {
            let value = match value {
                Value::Array(_) | Value::Object(_) => Value::String(value.to_string()),
                other => other.clone(),
            };

            (field.clone(), value)
        })
        .collect()
}

/// How a value compares once it is in a TEXT column. `1` and `"1"` are the same stored value,
/// and a write that changes neither is not a conflict.
fn as_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(true) => "1".to_owned(),
        Value::Bool(false) => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(value: Value) -> Map<String, Value> {
        value.as_object().expect("an object").clone()
    }

    fn upsert<'a>(table: &'a str, payload: Value, base: Option<&'a str>) -> Op<'a> {
        Op {
            table,
            kind: OpKind::Upsert,
            payload: fields(payload),
            base_updated_at: base,
        }
    }

    fn existing<'a>(updated_at: &'a str, fields: &'a Map<String, Value>) -> Existing<'a> {
        Existing {
            updated_at,
            deleted_at: None,
            updated_by: Some("ada"),
            fields,
        }
    }

    #[test]
    fn creates_a_row_that_is_not_there_yet() {
        let plan = plan(
            &upsert("songs", json!({ "title": "Amazing Grace" }), None),
            None,
        );

        assert_eq!(
            plan.action,
            Action::Insert(fields(json!({ "title": "Amazing Grace" })))
        );
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn writes_only_the_columns_the_table_accepts() {
        let plan = plan(
            &upsert(
                "songs",
                json!({ "title": "X", "change_seq": 99, "nonsense": true }),
                None,
            ),
            None,
        );

        assert_eq!(plan.action, Action::Insert(fields(json!({ "title": "X" }))));
    }

    /// Sync business rule 6 — a tombstone wins over a concurrent edit.
    #[test]
    fn a_stale_edit_does_not_resurrect_a_deleted_record() {
        let stored = fields(json!({ "title": "Amazing Grace" }));
        let deleted = Existing {
            deleted_at: Some("2026-09-06T10:00:00.000Z"),
            ..existing("2026-09-06T10:00:00.000Z", &stored)
        };

        let plan = plan(
            &upsert(
                "songs",
                json!({ "title": "Edited offline" }),
                Some("2026-09-06T09:00:00.000Z"),
            ),
            Some(&deleted),
        );

        assert_eq!(plan.action, Action::Skip(Skipped::Tombstoned));
    }

    #[test]
    fn deleting_something_that_was_never_synced_is_not_an_error() {
        let op = Op {
            kind: OpKind::Delete,
            ..upsert("songs", json!({}), None)
        };

        assert_eq!(plan(&op, None).action, Action::Skip(Skipped::AlreadyGone));
    }

    #[test]
    fn deletes_a_row_that_is_there() {
        let stored = fields(json!({ "title": "Amazing Grace" }));
        let op = Op {
            kind: OpKind::Delete,
            ..upsert("songs", json!({}), None)
        };

        assert_eq!(
            plan(&op, Some(&existing("2026-09-06T10:00:00.000Z", &stored))).action,
            Action::Delete
        );
    }

    /// The incoming write wins, and the value it displaces is kept so it can be offered back.
    #[test]
    fn records_the_value_a_late_write_displaces() {
        let stored = fields(json!({ "title": "Amazing Grace", "artist": "Newton" }));
        let plan = plan(
            &upsert(
                "songs",
                json!({ "title": "Amazing Grace (Live)", "artist": "Newton" }),
                Some("2026-09-06T09:00:00.000Z"),
            ),
            Some(&existing("2026-09-06T10:00:00.000Z", &stored)),
        );

        assert_eq!(
            plan.action,
            Action::Update(fields(
                json!({ "title": "Amazing Grace (Live)", "artist": "Newton" })
            ))
        );
        assert_eq!(
            plan.conflicts,
            [Conflict {
                field: "title".to_owned(),
                losing_value: json!("Amazing Grace"),
                losing_user: Some("ada".to_owned()),
            }],
            "the unchanged field is not a conflict"
        );
    }

    #[test]
    fn a_row_that_has_not_moved_since_the_client_read_it_conflicts_with_nothing() {
        let stored = fields(json!({ "title": "Amazing Grace" }));

        assert!(
            plan(
                &upsert(
                    "songs",
                    json!({ "title": "New" }),
                    Some("2026-09-06T10:00:00.000Z")
                ),
                Some(&existing("2026-09-06T10:00:00.000Z", &stored)),
            )
            .conflicts
            .is_empty()
        );
    }

    #[test]
    fn a_client_that_claims_no_base_claims_no_conflict() {
        let stored = fields(json!({ "title": "Amazing Grace" }));

        assert!(
            plan(
                &upsert("songs", json!({ "title": "New" }), None),
                Some(&existing("2026-09-06T10:00:00.000Z", &stored))
            )
            .conflicts
            .is_empty()
        );
    }

    /// `1` from one client and `"1"` from another are the same stored value.
    #[test]
    fn does_not_call_a_type_difference_a_conflict() {
        let stored = fields(json!({ "position": "3" }));

        assert!(
            plan(
                &upsert(
                    "folders",
                    json!({ "position": 3 }),
                    Some("2026-09-06T09:00:00.000Z")
                ),
                Some(&existing("2026-09-06T10:00:00.000Z", &stored)),
            )
            .conflicts
            .is_empty()
        );
    }

    #[test]
    fn stores_a_structured_field_as_its_json_text() {
        let plan = plan(
            &upsert("songs", json!({ "tags": ["hymn", "slow"] }), None),
            None,
        );

        assert_eq!(
            plan.action,
            Action::Insert(fields(json!({ "tags": "[\"hymn\",\"slow\"]" })))
        );
    }

    /// Exactly one default arrangement per song, whichever way the flag arrives.
    #[test]
    fn the_later_default_arrangement_demotes_the_others() {
        let created = plan(
            &upsert(
                "arrangements",
                json!({ "song_id": "grace", "is_default": 1 }),
                None,
            ),
            None,
        );

        assert_eq!(created.demote_defaults_of.as_deref(), Some("grace"));

        // A push that only flips the flag does not carry the song; the stored row does.
        let stored = fields(json!({ "song_id": "grace", "is_default": 0 }));
        let flipped = plan(
            &upsert("arrangements", json!({ "is_default": 1 }), None),
            Some(&existing("2026-09-06T10:00:00.000Z", &stored)),
        );

        assert_eq!(flipped.demote_defaults_of.as_deref(), Some("grace"));
    }

    #[test]
    fn demotes_nothing_when_nothing_became_the_default() {
        let stored = fields(json!({ "song_id": "grace", "is_default": 1 }));

        assert_eq!(
            plan(
                &upsert("arrangements", json!({ "is_default": 0 }), None),
                Some(&existing("2026-09-06T10:00:00.000Z", &stored)),
            )
            .demote_defaults_of,
            None
        );
        assert_eq!(
            plan(&upsert("songs", json!({ "title": "X" }), None), None).demote_defaults_of,
            None
        );
    }
}
