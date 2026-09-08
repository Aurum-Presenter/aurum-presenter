//! Draining the outbox and pulling the deltas.
//!
//! The order below is deliberate — drain, then pull. Pulling first would hand back the server's
//! older version of a record this device has already changed locally but not yet pushed, and the
//! apply step would overwrite the newer local edit.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use wasm_bindgen::JsValue;

use crate::api::{Api, ApiError};
use crate::app::locks::as_sole_worker;
use crate::db::records::{ConflictRecord, OutboxOp, SyncState};
use crate::db::{Database, DbError, schema};

const BATCH: usize = 200;

#[derive(Clone, Debug, Deserialize)]
pub struct PushResult {
    pub op_id: String,
    pub status: String,
    #[serde(default)]
    pub conflicts: Vec<String>,
    pub code: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PushResponse {
    results: Vec<PushResult>,
}

#[derive(Clone, Debug, Deserialize)]
struct PullResponse {
    change_seq: i64,
    tables: Map<String, Value>,
    has_more: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub pushed: usize,
    pub pulled: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    pub pending: usize,
    pub parked: usize,
    pub last_pull: Option<String>,
    pub last_push: Option<String>,
}

#[derive(Clone)]
pub struct SyncEngine {
    db: Database,
    api: Api,
    workspace_id: String,
}

impl SyncEngine {
    pub fn new(db: Database, api: Api, workspace_id: impl Into<String>) -> SyncEngine {
        SyncEngine {
            db,
            api,
            workspace_id: workspace_id.into(),
        }
    }

    /// Records a local change: the row and its outbox entry, together.
    ///
    /// If the two could diverge, a device could show a change it will never send, or send one it
    /// does not show.
    pub async fn record(
        &self,
        table: &str,
        record_id: &str,
        op: &str,
        payload: Map<String, Value>,
    ) -> Result<(), DbError> {
        let now = crate::now();
        let existing: Option<Map<String, Value>> = self.db.get(table, record_id).await?;
        let base_updated_at = existing
            .as_ref()
            .and_then(|row| row.get("updated_at"))
            .and_then(Value::as_str)
            .map(str::to_owned);

        let mut row = existing.clone().unwrap_or_else(|| blank_columns(table));

        if op == "delete" {
            // A delete is a tombstone locally too, so the row stays readable until the server
            // agrees and a later purge takes it.
            row.insert("id".to_owned(), json!(record_id));
            row.insert("deleted_at".to_owned(), json!(now));
            row.insert("updated_at".to_owned(), json!(now));
        } else {
            row.extend(payload.clone());
            row.insert("id".to_owned(), json!(record_id));
            row.insert("updated_at".to_owned(), json!(now));
            row.insert("deleted_at".to_owned(), Value::Null);
        }

        self.db.put(table, &row).await?;

        self.db
            .put(
                "outbox",
                &OutboxOp {
                    seq: None,
                    op_id: crate::new_id(),
                    table: table.to_owned(),
                    record_id: record_id.to_owned(),
                    op: op.to_owned(),
                    payload: Value::Object(payload),
                    base_updated_at,
                    attempts: 0,
                    last_error: None,
                    status: "pending".to_owned(),
                    created_at: now,
                },
            )
            .await?;

        Ok(())
    }

    async fn outbox(&self, status: &str) -> Result<Vec<OutboxOp>, DbError> {
        let mut ops: Vec<OutboxOp> = self
            .db
            .by_index("outbox", "status", &JsValue::from_str(status))
            .await?;

        ops.sort_by_key(|op| op.seq.unwrap_or_default());

        Ok(ops)
    }

    pub async fn pending_count(&self) -> usize {
        self.outbox("pending")
            .await
            .map(|ops| ops.len())
            .unwrap_or(0)
    }

    /// Operations the server refused. They wait for a person, not for a timer.
    pub async fn parked(&self) -> Result<Vec<OutboxOp>, DbError> {
        self.outbox("parked").await
    }

    /// Puts a parked operation back in the queue, after whatever blocked it has been fixed.
    pub async fn retry(&self, seq: i64) -> Result<(), DbError> {
        if let Some(mut op) = self.outbox_entry(seq).await? {
            op.status = "pending".to_owned();
            op.last_error = None;

            self.db.put("outbox", &op).await?;
        }

        Ok(())
    }

    /// Throws an operation away. The local record keeps whatever it has; only the push is lost.
    pub async fn discard(&self, seq: i64) -> Result<(), DbError> {
        self.db
            .delete("outbox", &JsValue::from_f64(seq as f64))
            .await
    }

    pub async fn status(&self) -> Status {
        let state = self.watermark().await;

        Status {
            pending: self
                .outbox("pending")
                .await
                .map(|ops| ops.len())
                .unwrap_or(0),
            parked: self
                .outbox("parked")
                .await
                .map(|ops| ops.len())
                .unwrap_or(0),
            last_pull: state.last_pull_at,
            last_push: state.last_push_at,
        }
    }

    /// One pass: drain, then pull. One window at a time across the whole device, not just one
    /// pass at a time in this one.
    pub async fn sync(&self) -> Result<Progress, ApiError> {
        if !crate::online() {
            return Ok(Progress::default());
        }

        let engine = self.clone();

        as_sole_worker(
            &format!("aurum-sync-{}", self.workspace_id),
            async move {
                let pushed = engine.drain().await?;
                let pulled = engine.pull().await?;

                Ok(Progress { pushed, pulled })
            },
            Ok(Progress::default()),
        )
        .await
    }

    async fn drain(&self) -> Result<usize, ApiError> {
        let pending = self.outbox("pending").await.unwrap_or_default();

        if pending.is_empty() {
            return Ok(0);
        }

        // Batched, but still in strict local order, so a create always reaches the server before
        // the update that depends on it.
        let batch: Vec<&OutboxOp> = pending.iter().take(BATCH).collect();
        let ops: Vec<Value> = batch
            .iter()
            .map(|op| {
                json!({
                    "op_id": op.op_id,
                    "table": op.table,
                    "record_id": op.record_id,
                    "op": op.op,
                    "payload": op.payload,
                    "base_updated_at": op.base_updated_at,
                })
            })
            .collect();

        let response: PushResponse = match self
            .api
            .post(
                &format!("/workspaces/{}/sync/push", self.workspace_id),
                json!({ "ops": ops }),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => {
                // Business rule 12: an expired session pauses the queue, it does not park it.
                // Parking asks a person to decide something, and "sign in again" is not a
                // decision about their work — the same batch goes out on the first pass after
                // they do.
                if !error.is_retryable() && error.status() != 401 {
                    // A permanent error parks the batch rather than retrying it forever. Nothing
                    // is discarded: a parked op waits for an explicit decision.
                    for op in &batch {
                        self.park(op, &error.to_string()).await;
                    }
                }

                return Err(error);
            }
        };

        let mut applied = 0;

        for op in &batch {
            let Some(result) = response
                .results
                .iter()
                .find(|result| result.op_id == op.op_id)
            else {
                continue;
            };

            if result.status == "rejected" {
                self.park(
                    op,
                    result
                        .error
                        .as_deref()
                        .or(result.code.as_deref())
                        .unwrap_or("rejected"),
                )
                .await;

                continue;
            }

            if !result.conflicts.is_empty() {
                let records: Vec<ConflictRecord> = result
                    .conflicts
                    .iter()
                    .map(|field| ConflictRecord {
                        id: crate::new_id(),
                        table: op.table.clone(),
                        record_id: op.record_id.clone(),
                        field: field.clone(),
                        losing_value: None,
                        at: crate::now(),
                        reviewed_at: None,
                    })
                    .collect();

                let _ = self.db.put_all("conflicts", &records).await;
            }

            if let Some(seq) = op.seq {
                let _ = self.discard(seq).await;
            }

            applied += 1;
        }

        if applied > 0 {
            let mut state = self.watermark().await;
            state.last_push_at = Some(crate::now());

            let _ = self.db.put("sync_state", &state).await;
        }

        Ok(applied)
    }

    async fn pull(&self) -> Result<usize, ApiError> {
        let mut applied = 0;

        loop {
            let state = self.watermark().await;
            let response: PullResponse = self
                .api
                .get(&format!(
                    "/workspaces/{}/sync/pull?since={}",
                    self.workspace_id, state.change_seq
                ))
                .await?;

            for table in schema::synced_tables() {
                let Some(rows) = response.tables.get(table).and_then(Value::as_array) else {
                    continue;
                };

                if rows.is_empty() {
                    continue;
                }

                let _ = self.db.put_all(table, rows).await;
                applied += rows.len();
            }

            let _ = self
                .db
                .put(
                    "sync_state",
                    &SyncState {
                        key: "watermark".to_owned(),
                        change_seq: response.change_seq,
                        last_pull_at: Some(crate::now()),
                        last_push_at: state.last_push_at,
                    },
                )
                .await;

            // A capped page means the server has more waiting; keep going rather than leaving
            // the device a page behind until the next tick.
            if !response.has_more {
                return Ok(applied);
            }
        }
    }

    async fn park(&self, op: &OutboxOp, error: &str) {
        let mut parked = op.clone();
        parked.status = "parked".to_owned();
        parked.last_error = Some(error.to_owned());
        parked.attempts += 1;

        let _ = self.db.put("outbox", &parked).await;
    }

    async fn outbox_entry(&self, seq: i64) -> Result<Option<OutboxOp>, DbError> {
        Ok(self
            .outbox("pending")
            .await?
            .into_iter()
            .chain(self.outbox("parked").await?)
            .find(|op| op.seq == Some(seq)))
    }

    pub async fn watermark(&self) -> SyncState {
        self.db
            .get("sync_state", "watermark")
            .await
            .ok()
            .flatten()
            .unwrap_or(SyncState {
                key: "watermark".to_owned(),
                change_seq: 0,
                last_pull_at: None,
                last_push_at: None,
            })
    }
}

/// Server-owned columns, present as null from the moment a row is created on a device.
///
/// They are filled in by the server and arrive on a later pull, so on the device that made the
/// row they would otherwise be *absent* rather than null — and "absent" reads as "no file" just
/// as convincingly. That is how the device holding the only copy of a file came to be told the
/// file had not been downloaded.
fn blank_columns(table: &str) -> Map<String, Value> {
    let mut blank = Map::new();

    blank.insert("change_seq".to_owned(), json!(0));
    blank.insert("updated_by".to_owned(), Value::Null);

    if table == "sheets" {
        for column in [
            "sha256",
            "size",
            "uploaded_at",
            "page_count",
            "pages_changed_at",
        ] {
            blank.insert(column.to_owned(), Value::Null);
        }
    }

    blank
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this exists to prevent: a sheet created on this device, whose file is right
    /// here, reported as "not downloaded" because the column was missing rather than null.
    #[test]
    fn a_new_sheet_starts_with_the_servers_columns_present_and_empty() {
        let blank = blank_columns("sheets");

        for column in [
            "sha256",
            "size",
            "uploaded_at",
            "page_count",
            "pages_changed_at",
        ] {
            assert_eq!(blank.get(column), Some(&Value::Null), "{column}");
        }

        assert_eq!(blank.get("change_seq"), Some(&json!(0)));
        // A table with no server-owned columns gets only the two every row has.
        assert_eq!(blank_columns("songs").len(), 2);
    }
}
