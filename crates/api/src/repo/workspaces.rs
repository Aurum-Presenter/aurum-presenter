//! Workspaces and who belongs to them — the one question a per-workspace file cannot answer
//! about itself.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::new_id;
use crate::db::now;
use crate::error::ApiResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Editor,
    Viewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Editor => "editor",
            Role::Viewer => "viewer",
        }
    }

    pub fn parse(text: &str) -> Option<Role> {
        match text {
            "owner" => Some(Role::Owner),
            "editor" => Some(Role::Editor),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    pub fn can_read(self) -> bool {
        true
    }

    /// A viewer reads. Everything that changes shared content needs at least an editor.
    pub fn can_write(self) -> bool {
        matches!(self, Role::Owner | Role::Editor)
    }

    /// Members, invites and roles are the owner's alone.
    pub fn can_manage(self) -> bool {
        self == Role::Owner
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub created_at: String,
}

fn workspace_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get("id")?,
        name: row.get("name")?,
        kind: row.get("kind")?,
        created_at: row.get("created_at")?,
    })
}

pub fn find(db: &Connection, id: &str) -> ApiResult<Option<Workspace>> {
    Ok(db
        .query_row(
            "SELECT * FROM workspaces WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            workspace_from,
        )
        .optional()?)
}

pub fn role_of(db: &Connection, user_id: &str, workspace_id: &str) -> ApiResult<Option<Role>> {
    let role: Option<String> = db
        .query_row(
            "SELECT m.role FROM memberships m
               JOIN workspaces w ON w.id = m.workspace_id
              WHERE m.user_id = ?1 AND m.workspace_id = ?2 AND w.deleted_at IS NULL",
            params![user_id, workspace_id],
            |row| row.get(0),
        )
        .optional()?;

    Ok(role.as_deref().and_then(Role::parse))
}

#[derive(Clone, Debug, Serialize)]
pub struct Membership {
    #[serde(flatten)]
    pub workspace: Workspace,
    pub role: Role,
}

pub fn for_user(db: &Connection, user_id: &str) -> ApiResult<Vec<Membership>> {
    let mut statement = db.prepare(
        "SELECT w.*, m.role FROM workspaces w
           JOIN memberships m ON m.workspace_id = w.id
          WHERE m.user_id = ?1 AND w.deleted_at IS NULL
          ORDER BY w.created_at",
    )?;

    let rows = statement
        .query_map([user_id], |row| {
            Ok(Membership {
                workspace: workspace_from(row)?,
                role: Role::parse(&row.get::<_, String>("role")?).unwrap_or(Role::Viewer),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

#[derive(Clone, Debug, Serialize)]
pub struct Member {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: Role,
    pub joined_at: String,
}

pub fn members(db: &Connection, workspace_id: &str) -> ApiResult<Vec<Member>> {
    let mut statement = db.prepare(
        "SELECT u.id, u.email, u.display_name, m.role, m.created_at FROM memberships m
           JOIN users u ON u.id = m.user_id
          WHERE m.workspace_id = ?1
          ORDER BY m.created_at",
    )?;

    let rows = statement
        .query_map([workspace_id], |row| {
            Ok(Member {
                user_id: row.get("id")?,
                email: row.get("email")?,
                display_name: row.get("display_name")?,
                role: Role::parse(&row.get::<_, String>("role")?).unwrap_or(Role::Viewer),
                joined_at: row.get("created_at")?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

pub fn create(db: &mut Connection, name: &str, kind: &str, owner_id: &str) -> ApiResult<Workspace> {
    create_with_id(db, None, name, kind, owner_id)
}

/// The client may name the id: it mints ids offline, and a workspace it created before it had
/// signal has to keep the id its rows already point at.
pub fn create_with_id(
    db: &mut Connection,
    id: Option<String>,
    name: &str,
    kind: &str,
    owner_id: &str,
) -> ApiResult<Workspace> {
    let id = id.unwrap_or_else(new_id);
    let now = now();
    let transaction = db.transaction()?;

    transaction.execute(
        "INSERT INTO workspaces (id, name, kind, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
        params![id, name, kind, now],
    )?;
    transaction.execute(
        "INSERT INTO memberships (id, user_id, workspace_id, role, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'owner', ?4, ?4)",
        params![new_id(), owner_id, id, now],
    )?;
    transaction.commit()?;

    Ok(Workspace {
        id,
        name: name.to_owned(),
        kind: kind.to_owned(),
        created_at: now,
    })
}

pub fn add_member(db: &Connection, workspace_id: &str, user_id: &str, role: Role) -> ApiResult<()> {
    db.execute(
        "INSERT INTO memberships (id, user_id, workspace_id, role, created_at, updated_at)
              VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT (user_id, workspace_id)
              DO UPDATE SET role = excluded.role, updated_at = excluded.updated_at",
        params![new_id(), user_id, workspace_id, role.as_str(), now()],
    )?;

    Ok(())
}

pub fn update_member_role(
    db: &Connection,
    workspace_id: &str,
    user_id: &str,
    role: Role,
) -> ApiResult<bool> {
    let affected = db.execute(
        "UPDATE memberships SET role = ?1, updated_at = ?2 WHERE workspace_id = ?3 AND user_id = ?4",
        params![role.as_str(), now(), workspace_id, user_id],
    )?;

    Ok(affected == 1)
}

pub fn remove_member(db: &Connection, workspace_id: &str, user_id: &str) -> ApiResult<bool> {
    let affected = db.execute(
        "DELETE FROM memberships WHERE workspace_id = ?1 AND user_id = ?2",
        params![workspace_id, user_id],
    )?;

    Ok(affected == 1)
}

/// How many owners a workspace has. A workspace with none is unadministrable, so the last one
/// cannot be demoted or removed.
pub fn owner_count(db: &Connection, workspace_id: &str) -> ApiResult<i64> {
    Ok(db.query_row(
        "SELECT COUNT(*) FROM memberships WHERE workspace_id = ?1 AND role = 'owner'",
        [workspace_id],
        |row| row.get(0),
    )?)
}
