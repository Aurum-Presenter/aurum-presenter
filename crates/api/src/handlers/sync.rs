//! Push and pull: the endpoints the whole offline story rests on.
//!
//! Every decision about *what* a push does — which columns are writable, whether a tombstone
//! beats a concurrent edit, what counts as a conflict — comes from `aurum_core::sync`, which the
//! client compiles too. This module supplies the transaction, the SQL and the HTTP.

use aurum_core::sync::{merge, schema};
use axum::Json;
use axum::extract::{Query, State};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::db::{blocking, current_sequence, now, write_batch};
use crate::error::{ApiError, ApiResult};
use crate::extract::{Body, Read, Workspace};
use crate::repo::new_id;
use crate::repo::workspaces::Role;
use crate::state::AppState;

const MAX_OPS: usize = 500;
const CONFLICT_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub struct PullQuery {
    #[serde(default)]
    since: i64,
    tables: Option<String>,
    limit: Option<i64>,
}

pub async fn pull(
    State(state): State<AppState>,
    workspace: Workspace<Read>,
    Query(query): Query<PullQuery>,
) -> ApiResult<Json<Value>> {
    let limit = query.limit.unwrap_or(1000).clamp(1, 5000);

    blocking(move || {
        let db = workspace.open(&state)?;
        let user_id = &workspace.caller.user.id;

        let requested: Vec<&str> = match query.tables.as_deref().filter(|t| !t.is_empty()) {
            None => schema::tables(),
            Some(list) => list
                .split(',')
                .map(str::trim)
                .filter(|table| schema::is_synced_table(table))
                .filter_map(|table| schema::tables().into_iter().find(|known| *known == table))
                .collect(),
        };

        let mut tables = Map::new();
        let mut has_more = false;

        for table in requested {
            // Preferences are personal — a preferred key, a capo, a font size. Every member has
            // their own row for the same song, and no member is ever handed anyone else's.
            let mine = if table == "preferences" {
                " AND user_id = ?3"
            } else {
                ""
            };

            let sql = format!(
                "SELECT * FROM {table} WHERE change_seq > ?1{mine} ORDER BY change_seq LIMIT ?2"
            );
            let mut statement = db.prepare(&sql)?;
            let mut rows = if mine.is_empty() {
                read_rows(statement.query(rusqlite::params![query.since, limit + 1])?)?
            } else {
                read_rows(statement.query(rusqlite::params![query.since, limit + 1, user_id])?)?
            };

            if rows.len() as i64 > limit {
                has_more = true;
                rows.pop();
            }

            tables.insert(table.to_owned(), Value::Array(rows));
        }

        Ok(Json(json!({
            "change_seq": current_sequence(&db)?,
            "tables": tables,
            "has_more": has_more,
        })))
    })
    .await
}

/// A row as JSON, with SQLite's five storage classes mapped the way the PHP's PDO did.
fn read_rows(mut rows: rusqlite::Rows<'_>) -> ApiResult<Vec<Value>> {
    let names: Vec<String> = rows
        .as_ref()
        .map(|statement| {
            statement
                .column_names()
                .iter()
                .map(|n| (*n).to_owned())
                .collect()
        })
        .unwrap_or_default();

    let mut out = Vec::new();

    while let Some(row) = rows.next()? {
        let mut object = Map::new();

        for (index, name) in names.iter().enumerate() {
            object.insert(name.clone(), sql_to_json(row.get_ref(index)?));
        }

        out.push(Value::Object(object));
    }

    Ok(out)
}

fn sql_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => json!(number),
        ValueRef::Text(bytes) => json!(String::from_utf8_lossy(bytes)),
        ValueRef::Blob(bytes) => json!(String::from_utf8_lossy(bytes)),
    }
}

pub async fn conflicts(
    State(state): State<AppState>,
    workspace: Workspace<Read>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let since = query.get("since").cloned().unwrap_or_default();

    blocking(move || {
        let db = workspace.open(&state)?;
        let mut statement = db.prepare(
            "SELECT id, table_name, record_id, field, losing_value, losing_user, at
               FROM sync_conflicts WHERE at > ?1 ORDER BY at DESC LIMIT ?2",
        )?;

        let rows = read_rows(statement.query(rusqlite::params![since, CONFLICT_LIMIT])?)?;

        Ok(Json(json!({ "conflicts": rows })))
    })
    .await
}

pub async fn push(
    State(state): State<AppState>,
    workspace: Workspace<Read>,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let ops = body
        .get("ops")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| ApiError::validation("ops", "\"ops\" must be an array."))?;

    if ops.len() > MAX_OPS {
        return Err(ApiError::unprocessable(format!(
            "A push may carry at most {MAX_OPS} operations."
        ))
        .with_code("batch_too_large"));
    }

    blocking(move || {
        let mut db = workspace.open(&state)?;
        let user_id = workspace.caller.user.id.clone();
        let role = workspace.role;

        if ops.is_empty() {
            return Ok(Json(json!({
                "results": [],
                "change_seq": current_sequence(&db)?,
            })));
        }

        // One BEGIN IMMEDIATE for the whole batch: the sequence values are contiguous, and a
        // batch that fails part-way leaves the workspace exactly as it was.
        let results = write_batch(&mut db, ops.len() as i64, |transaction, first_seq| {
            let now = now();

            Ok(ops
                .iter()
                .enumerate()
                .map(|(index, op)| {
                    let seq = first_seq + index as i64;

                    match apply_one(transaction, op, &user_id, role, seq, &now) {
                        Ok(result) => result,
                        // A permanent failure parks that one operation; the rest of the batch
                        // still applies. The client's outbox needs the op id back to know what
                        // to park.
                        Err(error) => json!({
                            "op_id": op.get("op_id").and_then(Value::as_str)
                                .map(str::to_owned)
                                .unwrap_or_else(|| format!("index-{index}")),
                            "status": "rejected",
                            "code": error.code,
                            "error": error.message,
                        }),
                    }
                })
                .collect::<Vec<Value>>())
        })?;

        Ok(Json(json!({
            "results": results,
            "change_seq": current_sequence(&db)?,
        })))
    })
    .await
}

fn apply_one(
    db: &Transaction<'_>,
    op: &Value,
    user_id: &str,
    role: Role,
    seq: i64,
    now: &str,
) -> ApiResult<Value> {
    let op_id = require_uuid(op, "op_id")?;
    let record_id = require_uuid(op, "record_id")?;
    let table = op.get("table").and_then(Value::as_str).unwrap_or_default();
    let payload = op
        .get("payload")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if !schema::is_synced_table(table) {
        return Err(
            ApiError::unprocessable(format!("\"{table}\" is not a synced table."))
                .with_code("unknown_table"),
        );
    }

    assert_may_write(table, role, &payload, user_id)?;

    // Idempotency: a retried batch after a lost response must not apply twice.
    let seen: Option<i64> = db
        .query_row(
            "SELECT 1 FROM applied_ops WHERE op_id = ?1",
            [&op_id],
            |row| row.get(0),
        )
        .optional()?;

    if seen.is_some() {
        return Ok(json!({ "op_id": op_id, "status": "duplicate" }));
    }

    let existing_fields = read_row(db, table, &record_id)?;
    let existing = existing_fields.as_ref().map(|fields| merge::Existing {
        updated_at: fields
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        deleted_at: fields.get("deleted_at").and_then(Value::as_str),
        updated_by: fields.get("updated_by").and_then(Value::as_str),
        fields,
    });

    let plan = merge::plan(
        &merge::Op {
            table,
            kind: match op.get("op").and_then(Value::as_str) {
                Some("delete") => merge::OpKind::Delete,
                _ => merge::OpKind::Upsert,
            },
            payload,
            base_updated_at: op.get("base_updated_at").and_then(Value::as_str),
        },
        existing.as_ref(),
    );

    let mut conflicted: Vec<String> = Vec::new();

    for conflict in &plan.conflicts {
        db.execute(
            "INSERT INTO sync_conflicts
                (id, table_name, record_id, field, losing_value, losing_user, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                new_id(),
                table,
                record_id,
                conflict.field,
                json_to_sql(&conflict.losing_value),
                conflict.losing_user,
                now
            ],
        )?;

        conflicted.push(conflict.field.clone());
    }

    // Exactly one default arrangement per song. The demotion carries this operation's sequence
    // number, so every device pulls it as part of the same change.
    if let Some(song_id) = &plan.demote_defaults_of {
        db.execute(
            "UPDATE arrangements SET is_default = 0, updated_at = ?1, change_seq = ?2, updated_by = ?3
              WHERE song_id = ?4 AND id <> ?5 AND is_default = 1",
            rusqlite::params![now, seq, user_id, song_id, record_id],
        )?;
    }

    match &plan.action {
        merge::Action::Insert(payload) => {
            insert(db, table, &record_id, payload, seq, now, user_id)?
        }
        merge::Action::Update(payload) => {
            update(db, table, &record_id, payload, seq, now, user_id)?
        }
        merge::Action::Delete => {
            db.execute(
                &format!(
                    "UPDATE {table} SET deleted_at = ?1, updated_at = ?1, change_seq = ?2,
                                        updated_by = ?3 WHERE id = ?4"
                ),
                rusqlite::params![now, seq, user_id, record_id],
            )?;
        }
        merge::Action::Skip(_) => {}
    }

    db.execute(
        "INSERT INTO applied_ops (op_id, user_id, applied_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![op_id, user_id, now],
    )?;

    let mut result = json!({ "op_id": op_id, "status": "applied", "change_seq": seq });

    if !conflicted.is_empty() {
        result["conflicts"] = json!(conflicted);
    }

    Ok(result)
}

fn read_row(db: &Transaction<'_>, table: &str, id: &str) -> ApiResult<Option<Map<String, Value>>> {
    let mut statement = db.prepare(&format!("SELECT * FROM {table} WHERE id = ?1"))?;
    let rows = read_rows(statement.query([id])?)?;

    Ok(rows.into_iter().next().and_then(|row| match row {
        Value::Object(fields) => Some(fields),
        _ => None,
    }))
}

fn insert(
    db: &Transaction<'_>,
    table: &str,
    id: &str,
    payload: &Map<String, Value>,
    seq: i64,
    now: &str,
    user_id: &str,
) -> ApiResult<()> {
    let mut columns = vec!["id".to_owned()];
    let mut values: Vec<SqlValue> = vec![SqlValue::Text(id.to_owned())];

    for (field, value) in payload {
        columns.push(field.clone());
        values.push(json_to_sql(value));
    }

    for (field, value) in [
        ("updated_at", SqlValue::Text(now.to_owned())),
        ("change_seq", SqlValue::Integer(seq)),
        ("updated_by", SqlValue::Text(user_id.to_owned())),
        ("deleted_at", SqlValue::Null),
    ] {
        columns.push(field.to_owned());
        values.push(value);
    }

    let placeholders: Vec<String> = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect();

    db.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            columns.join(", "),
            placeholders.join(", ")
        ),
        rusqlite::params_from_iter(values),
    )?;

    Ok(())
}

fn update(
    db: &Transaction<'_>,
    table: &str,
    id: &str,
    payload: &Map<String, Value>,
    seq: i64,
    now: &str,
    user_id: &str,
) -> ApiResult<()> {
    let mut assignments: Vec<String> = Vec::new();
    let mut values: Vec<SqlValue> = Vec::new();

    for (field, value) in payload {
        assignments.push(format!("{field} = ?{}", assignments.len() + 1));
        values.push(json_to_sql(value));
    }

    for (field, value) in [
        ("updated_at", SqlValue::Text(now.to_owned())),
        ("change_seq", SqlValue::Integer(seq)),
        ("updated_by", SqlValue::Text(user_id.to_owned())),
        ("deleted_at", SqlValue::Null),
    ] {
        assignments.push(format!("{field} = ?{}", assignments.len() + 1));
        values.push(value);
    }

    values.push(SqlValue::Text(id.to_owned()));

    db.execute(
        &format!(
            "UPDATE {table} SET {} WHERE id = ?{}",
            assignments.join(", "),
            values.len()
        ),
        rusqlite::params_from_iter(values),
    )?;

    Ok(())
}

fn json_to_sql(value: &Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Bool(flag) => SqlValue::Integer(i64::from(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(number) => SqlValue::Integer(number),
            None => SqlValue::Real(number.as_f64().unwrap_or_default()),
        },
        Value::String(text) => SqlValue::Text(text.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

/// Who may write what, beyond the role check the extractor already made.
fn assert_may_write(
    table: &str,
    role: Role,
    payload: &Map<String, Value>,
    user_id: &str,
) -> ApiResult<()> {
    let owns = |field: &str| {
        payload
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(|value| value == user_id)
    };

    // Not a role question: an owner has no more business writing another member's preferred key
    // than a viewer does.
    if table == "preferences" && !owns("user_id") {
        return Err(
            ApiError::forbidden("Preferences can only be written for yourself.")
                .with_code("not_your_row"),
        );
    }

    if role != Role::Viewer {
        return Ok(());
    }

    if !schema::is_viewer_writable(table) {
        return Err(
            ApiError::forbidden(format!("Your role (viewer) cannot change {table}."))
                .with_code("insufficient_role"),
        );
    }

    if table == "annotations" {
        let scope = payload
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("personal");

        if scope != "personal" {
            return Err(
                ApiError::forbidden("Shared annotations need editor access.")
                    .with_code("insufficient_role"),
            );
        }

        if !owns("author_id") {
            return Err(
                ApiError::forbidden("Annotations can only be written for yourself.")
                    .with_code("not_your_row"),
            );
        }
    }

    Ok(())
}

fn require_uuid(op: &Value, key: &str) -> ApiResult<String> {
    op.get(key)
        .and_then(Value::as_str)
        .filter(|value| aurum_core::ids::is_uuid(value))
        .map(str::to_owned)
        .ok_or_else(|| ApiError::validation(key, format!("\"{key}\" must be a UUID.")))
}

/// Only used by the CLI, which has no `Workspace` value to open one with.
pub fn open_for_maintenance(db: &Connection) -> ApiResult<i64> {
    current_sequence(db)
}
