//! Using the app without an account.
//!
//! Everything works: songs, charts, sets, presentation. What is missing is other people. The
//! workspace id is generated on the device, so when the user does sign in, the same id is
//! claimed server-side and every record they already made keeps the identity it was born with —
//! nothing is re-created and nothing is re-downloaded.

use serde::{Deserialize, Serialize};

use crate::api::models::{Account, Totp, Workspace};
use crate::api::{Api, ApiError};
use crate::app::storage;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LocalMode {
    pub workspace_id: String,
    pub name: String,
    pub started_at: String,
}

pub fn local_mode() -> Option<LocalMode> {
    storage::read_json(storage::LOCAL)
}

pub fn start(name: &str) -> LocalMode {
    let mode = LocalMode {
        workspace_id: crate::new_id(),
        name: name.to_owned(),
        started_at: crate::now(),
    };

    storage::write_json(storage::LOCAL, &mode);

    mode
}

pub fn end() {
    storage::remove(storage::LOCAL);
}

/// The account the app runs on before there is an account. It is a real workspace with a real
/// id — the same one the server will be given when it is claimed.
pub fn local_account(mode: &LocalMode) -> Account {
    Account {
        id: "local-device".to_owned(),
        email: String::new(),
        display_name: "This device".to_owned(),
        totp: Totp::default(),
        workspaces: vec![Workspace {
            id: mode.workspace_id.clone(),
            name: mode.name.clone(),
            kind: "personal".to_owned(),
            role: "owner".to_owned(),
            created_at: mode.started_at.clone(),
        }],
    }
}

/// Hands the local workspace to the server under the id it already has.
///
/// Called once, straight after a first sign-in; from then on the outbox pushes everything that
/// was made offline.
pub async fn claim(api: &Api) -> Result<Option<Workspace>, ApiError> {
    let Some(mode) = local_mode() else {
        return Ok(None);
    };

    let claimed = match api
        .create_workspace(&mode.name, Some(&mode.workspace_id))
        .await
    {
        Ok(workspace) => workspace,
        // Already claimed by an earlier attempt that lost its answer: the id is taken, which is
        // the outcome this wanted. Anything else is a real failure and the local mode stays.
        Err(error) if error.code() == "workspace_exists" => {
            end();

            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    end();

    // Select it, too. Everything this person has made is in here, and the account they have
    // just created also has an empty personal workspace that would otherwise win by being first
    // in the list.
    storage::write(storage::WORKSPACE, &claimed.id);

    Ok(Some(claimed))
}
