//! Everything a screen needs to read and write the current workspace.
//!
//! The database and the sync engine are per workspace, and switching workspace means opening a
//! different database rather than filtering a shared one — the same shape the server has, where
//! a workspace *is* a file.

use leptos::prelude::*;

use crate::api::Api;
use crate::api::models::{Account, Workspace};
use crate::db::{Database, live};
use crate::sync::SyncEngine;

/// Provided once, at the top of the signed-in app, and read by every screen below it.
#[derive(Clone)]
pub struct WorkspaceContext {
    pub me: RwSignal<Account>,
    /// True when there is no account yet: everything works, nothing leaves the device.
    pub local: bool,
    pub workspace: Signal<Workspace>,
    pub db: Signal<Option<Database>>,
    pub engine: Signal<Option<SyncEngine>>,
    pub api: Api,
    pub online: RwSignal<bool>,
    pub pending: RwSignal<usize>,
    set_workspace_id: WriteSignal<String>,
}

impl WorkspaceContext {
    pub fn can_edit(&self) -> bool {
        self.local || self.workspace.get().can_edit()
    }

    pub fn can_manage(&self) -> bool {
        !self.local && self.workspace.get().can_manage()
    }

    pub fn set_workspace(&self, id: &str) {
        super::storage::write(super::storage::WORKSPACE, id);
        self.set_workspace_id.set(id.to_owned());
    }

    /// The workspace database, for a screen that has one. `None` only while it is opening.
    pub fn database(&self) -> Option<Database> {
        self.db.get()
    }
}

pub fn use_workspace() -> WorkspaceContext {
    use_context::<WorkspaceContext>().expect("a WorkspaceContext above this screen")
}

/// Builds the context and puts it in scope.
///
/// The chosen workspace is remembered on the device, because a musician who opened the band's
/// library last time is opening it again — but a remembered id that is no longer a membership
/// falls back to the first one rather than to an empty screen.
pub fn provide_workspace(me: Account, local: bool, api: Api) -> WorkspaceContext {
    let me = RwSignal::new(me);
    let remembered = super::storage::read(super::storage::WORKSPACE);

    let first = me.with_untracked(|me| {
        remembered
            .filter(|id| me.workspaces.iter().any(|space| &space.id == id))
            .or_else(|| me.workspaces.first().map(|space| space.id.clone()))
            .unwrap_or_default()
    });

    let (workspace_id, set_workspace_id) = signal(first);

    let workspace = Signal::derive(move || {
        let id = workspace_id.get();

        me.with(|me| {
            me.workspaces
                .iter()
                .find(|space| space.id == id)
                .or_else(|| me.workspaces.first())
                .cloned()
                .unwrap_or_else(|| Workspace {
                    id,
                    name: "My songs".to_owned(),
                    kind: "personal".to_owned(),
                    role: "owner".to_owned(),
                    created_at: String::new(),
                })
        })
    });

    // Opening a database is asynchronous, so the rest of the app reads it as "not yet".
    let opened = LocalResource::new(move || {
        let id = workspace.get().id;

        async move {
            let database = Database::open(&id).await.ok();

            if database.is_some() {
                live::listen(&id);
            }

            database
        }
    });

    let db = Signal::derive(move || opened.get().and_then(|mut held| held.take()));
    let engine_api = api.clone();
    let engine = Signal::derive(move || {
        let database = db.get()?;
        let id = database.workspace_id().to_owned();

        Some(SyncEngine::new(database, engine_api.clone(), id))
    });

    let context = WorkspaceContext {
        me,
        local,
        workspace,
        db,
        engine,
        api,
        online: RwSignal::new(crate::online()),
        pending: RwSignal::new(0),
        set_workspace_id,
    };

    provide_context(context.clone());

    context
}
