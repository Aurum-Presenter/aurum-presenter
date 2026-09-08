//! Two tiers of SQLite, and the pragmas neither of them is correct without.
//!
//! The identity tier is one file: accounts, credentials, sessions, workspaces, memberships,
//! invites. The content tier is one file *per workspace* — which is the design's whole safety
//! argument, because a query cannot omit a `workspace_id` predicate that does not exist. A
//! handler that was handed the wrong connection is the only way to read the wrong workspace,
//! and handlers are never handed one they were not granted.

use std::path::{Path, PathBuf};

use include_dir::{Dir, include_dir};
use rusqlite::Connection;

use crate::config::Database;
use crate::error::{ApiError, ApiResult};

static CONTROL_MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../migrations/control");
static WORKSPACE_MIGRATIONS: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../../migrations/workspace");

#[derive(Clone, Debug)]
pub struct Databases {
    config: Database,
}

impl Databases {
    pub fn new(config: Database) -> Databases {
        Databases { config }
    }

    pub fn control_path(&self) -> &Path {
        &self.config.control_path
    }

    pub fn workspace_path(&self, workspace_id: &str) -> PathBuf {
        self.config
            .workspace_dir
            .join(format!("{workspace_id}.sqlite"))
    }

    pub fn workspace_exists(&self, workspace_id: &str) -> bool {
        self.workspace_path(workspace_id).is_file()
    }

    /// Every workspace id that has a database file on this host.
    pub fn all_workspaces(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.config.workspace_dir) else {
            return Vec::new();
        };

        let mut ids: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();

                (path.extension()? == "sqlite")
                    .then(|| path.file_stem()?.to_str().map(str::to_owned))?
            })
            .collect();
        ids.sort();
        ids
    }

    pub fn open_control(&self) -> ApiResult<Connection> {
        let connection = open(&self.config.control_path, self.config.busy_timeout_ms)?;

        if self.config.auto_migrate {
            migrate(&connection, &CONTROL_MIGRATIONS)?;
        }

        Ok(connection)
    }

    /// Opens the workspace file, creating and migrating it if it does not exist yet. A file
    /// behind the current migration set is brought up to date here, so a workspace restored from
    /// an old backup repairs itself rather than failing on its first query.
    pub fn open_workspace(&self, workspace_id: &str) -> ApiResult<Connection> {
        let connection = open(
            &self.workspace_path(workspace_id),
            self.config.busy_timeout_ms,
        )?;

        if self.config.auto_migrate {
            migrate(&connection, &WORKSPACE_MIGRATIONS)?;
        }

        Ok(connection)
    }

    pub fn migrate_control(&self) -> ApiResult<Vec<String>> {
        migrate(
            &open(&self.config.control_path, self.config.busy_timeout_ms)?,
            &CONTROL_MIGRATIONS,
        )
    }

    pub fn migrate_workspace(&self, workspace_id: &str) -> ApiResult<Vec<String>> {
        migrate(
            &open(
                &self.workspace_path(workspace_id),
                self.config.busy_timeout_ms,
            )?,
            &WORKSPACE_MIGRATIONS,
        )
    }

    pub fn delete_workspace(&self, workspace_id: &str) {
        // WAL leaves two sidecar files; a "deleted" workspace that leaves -wal behind is a
        // workspace whose last transactions are still recoverable on disk.
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!(
                "{}{suffix}",
                self.workspace_path(workspace_id).display()
            ));
        }
    }
}

/// Opens a SQLite file with the four pragmas this application depends on.
///
/// None of them is optional. WAL lets readers run while the single writer holds the write lock,
/// without which a sync pull blocks behind a sync push. `busy_timeout` makes concurrent writers
/// queue instead of failing outright. `foreign_keys` is off by default and per connection, so
/// every foreign key in the schema is decorative until it runs. And `synchronous = NORMAL` is
/// the correct pairing with WAL: durable across a process crash, at risk only in a power loss,
/// which is the trade the sync engine already tolerates.
fn open(path: &Path, busy_timeout_ms: u32) -> ApiResult<Connection> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| ApiError::internal("creating the database directory", error))?;
    }

    let connection = Connection::open(path)?;

    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "busy_timeout", busy_timeout_ms)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;

    Ok(connection)
}

/// Applies numbered plain-SQL migrations and records them in the file itself.
///
/// Keeping the applied set inside each database is what lets a workspace restored from an old
/// backup repair itself on open, with no central registry that could disagree with the file it
/// describes.
fn migrate(connection: &Connection, migrations: &Dir<'_>) -> ApiResult<Vec<String>> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version    TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
    )?;

    let applied: Vec<String> = connection
        .prepare("SELECT version FROM schema_version")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;

    let mut pending: Vec<(&str, &str)> = migrations
        .files()
        .filter_map(|file| {
            let version = file.path().file_stem()?.to_str()?;

            (!applied.iter().any(|done| done == version))
                .then(|| Some((version, file.contents_utf8()?)))?
        })
        .collect();
    pending.sort_by_key(|(version, _)| *version);

    let mut ran = Vec::new();

    for (version, sql) in pending {
        connection.execute_batch(&format!("BEGIN; {sql}"))?;
        connection.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?1, ?2)",
            (version, now()),
        )?;
        connection.execute_batch("COMMIT")?;

        ran.push(version.to_owned());
    }

    Ok(ran)
}

/// The wall clock, in the one format every timestamp in this system uses.
pub fn now() -> String {
    aurum_core::time::format(now_ms())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// Runs a closure inside `BEGIN IMMEDIATE`, handing it the workspace's next change sequence.
///
/// Not a plain `BEGIN`: in SQLite that is *deferred*, and the write lock is taken lazily at the
/// first write. Two deferred transactions that both read the counter before either writes
/// produce the same sequence value, and one then fails with SQLITE_BUSY on upgrade rather than
/// waiting. `BEGIN IMMEDIATE` takes the lock up front, so writers queue on `busy_timeout` and
/// the counter is strictly serialised.
///
/// This is the SQLite replacement for a per-workspace Postgres sequence, and it is stronger in
/// one respect: a sequence burns a value on rollback, whereas this counter rolls back with the
/// rest of the transaction, so `change_seq` has no gaps.
pub fn write_batch<T>(
    connection: &mut Connection,
    count: i64,
    work: impl FnOnce(&rusqlite::Transaction<'_>, i64) -> ApiResult<T>,
) -> ApiResult<T> {
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    let first = {
        transaction.execute(
            "UPDATE sync_counter SET seq = seq + ?1 WHERE id = 1",
            [count.max(1)],
        )?;

        transaction.query_row("SELECT seq FROM sync_counter WHERE id = 1", [], |row| {
            row.get::<_, i64>(0)
        })? - count.max(1)
            + 1
    };

    let result = work(&transaction, first)?;

    transaction.commit()?;

    Ok(result)
}

pub fn current_sequence(connection: &Connection) -> ApiResult<i64> {
    Ok(connection
        .query_row("SELECT seq FROM sync_counter WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap_or(0))
}

/// Runs blocking database work off the async runtime's worker threads.
pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| ApiError::internal("a database task panicked", error))?
}
