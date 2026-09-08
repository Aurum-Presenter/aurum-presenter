//! The device's working source of truth: what stores exist and how they are indexed.
//!
//! One database per workspace, mirroring the server's one-SQLite-file-per-workspace layout.
//! Because the file *is* the scope on the server, these records carry no workspace id —
//! switching workspaces means opening a different database, not filtering a shared one.
//!
//! The names here are contract. The end-to-end suite reads them, and a device that already
//! holds a database from an earlier version has to find its stores where it left them.

/// A store, its key path, and the indexes the app actually queries by.
pub struct Store {
    pub name: &'static str,
    pub key_path: &'static str,
    /// `++` on the key path means the store generates its own keys.
    pub auto_increment: bool,
    pub indexes: &'static [Index],
}

pub struct Index {
    pub name: &'static str,
    /// More than one path is a compound index.
    pub key_path: &'static [&'static str],
}

const fn store(name: &'static str, key_path: &'static str, indexes: &'static [Index]) -> Store {
    Store {
        name,
        key_path,
        auto_increment: false,
        indexes,
    }
}

const fn index(name: &'static str) -> Index {
    Index {
        name,
        key_path: &[],
    }
}

/// The database version. Everything below is created on open if it is not already there, so a
/// device holding an older database gains what it is missing rather than being rebuilt.
pub const VERSION: u32 = 6;

pub fn database_name(workspace_id: &str) -> String {
    format!("aurum-{workspace_id}")
}

pub const STORES: &[Store] = &[
    store("folders", "id", &[index("parent_id"), index("change_seq")]),
    store(
        "songs",
        "id",
        &[index("folder_id"), index("title"), index("change_seq")],
    ),
    store(
        "song_placements",
        "id",
        &[index("song_id"), index("folder_id"), index("change_seq")],
    ),
    store(
        "arrangements",
        "id",
        &[index("song_id"), index("change_seq")],
    ),
    store("sheets", "id", &[index("song_id"), index("change_seq")]),
    store(
        "annotations",
        "id",
        &[index("sheet_id"), index("change_seq")],
    ),
    store("sets", "id", &[index("scheduled_for"), index("change_seq")]),
    store(
        "set_items",
        "id",
        &[
            index("set_id"),
            index("song_id"),
            index("rank"),
            index("change_seq"),
        ],
    ),
    store(
        "preferences",
        "id",
        &[
            Index {
                name: "scope",
                key_path: &["user_id", "scope_type", "scope_id", "name"],
            },
            index("change_seq"),
        ],
    ),
    store("presenter_themes", "id", &[index("change_seq")]),
    // The outbox generates its own keys, which is what gives it a monotonic local order — and
    // that order is what lets it be drained exactly as the musician made the changes.
    Store {
        name: "outbox",
        key_path: "seq",
        auto_increment: true,
        indexes: &[index("op_id"), index("status"), index("created_at")],
    },
    store("sync_state", "key", &[]),
    store(
        "conflicts",
        "id",
        &[index("table"), index("record_id"), index("reviewed_at")],
    ),
    store(
        "blobs",
        "sheet_id",
        &[index("cached_at"), index("pin_reason")],
    ),
    store("files", "sheet_id", &[]),
    store("uploads", "sheet_id", &[index("queued_at")]),
    // Local only, never synced: a session has no meaning after it ends, and none at all on
    // another device.
    store("live_sessions", "session_id", &[index("updated_at")]),
    store("session_log", "session_id", &[index("started_at")]),
];

/// The tables that sync, in the order a full pull walks them. Taken from `aurum-core` so the
/// client cannot come to believe in a table the server will not take.
pub fn synced_tables() -> Vec<&'static str> {
    aurum_core::sync::schema::tables()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_synced_table_has_a_store_to_land_in() {
        for table in synced_tables() {
            assert!(
                STORES.iter().any(|store| store.name == table),
                "{table} syncs but has nowhere on the device to go"
            );
        }
    }

    /// The name is contract: the end-to-end suite reads it, and so does a device upgrading.
    #[test]
    fn the_database_is_named_after_its_workspace() {
        assert_eq!(
            database_name("01890000-0000-7000-8000-0000000000ab"),
            "aurum-01890000-0000-7000-8000-0000000000ab"
        );
    }

    #[test]
    fn only_the_outbox_generates_its_own_keys() {
        let generated: Vec<&str> = STORES
            .iter()
            .filter(|store| store.auto_increment)
            .map(|store| store.name)
            .collect();

        assert_eq!(generated, ["outbox"]);
    }
}
