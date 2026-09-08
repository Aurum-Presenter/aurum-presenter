//! Refresh sessions, and the family that dies together when a cookie turns out to be stolen.

use rusqlite::{Connection, OptionalExtension, params};

use super::new_id;
use crate::db::now;
use crate::error::ApiResult;

#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub family_id: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub replaced_by: Option<String>,
}

fn session_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        family_id: row.get("family_id")?,
        expires_at: row.get("expires_at")?,
        revoked_at: row.get("revoked_at")?,
        replaced_by: row.get("replaced_by")?,
    })
}

pub fn open(
    db: &Connection,
    user_id: &str,
    refresh_token_hash: &str,
    expires_at: &str,
    user_agent: Option<&str>,
    family_id: Option<String>,
) -> ApiResult<(String, String)> {
    let id = new_id();
    let family = family_id.unwrap_or_else(new_id);

    db.execute(
        "INSERT INTO sessions
            (id, user_id, family_id, refresh_token_hash, user_agent, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            user_id,
            family,
            refresh_token_hash,
            user_agent,
            now(),
            expires_at
        ],
    )?;

    Ok((id, family))
}

pub fn find_by_refresh_hash(db: &Connection, hash: &str) -> ApiResult<Option<Session>> {
    Ok(db
        .query_row(
            "SELECT * FROM sessions WHERE refresh_token_hash = ?1",
            [hash],
            session_from,
        )
        .optional()?)
}

pub fn find_by_id(db: &Connection, id: &str) -> ApiResult<Option<Session>> {
    Ok(db
        .query_row("SELECT * FROM sessions WHERE id = ?1", [id], session_from)
        .optional()?)
}

pub fn mark_replaced(db: &Connection, session_id: &str, replacement_id: &str) -> ApiResult<()> {
    db.execute(
        "UPDATE sessions SET revoked_at = ?1, replaced_by = ?2 WHERE id = ?3",
        params![now(), replacement_id, session_id],
    )?;

    Ok(())
}

/// Reuse detection: a refresh token presented twice means somebody has a copy, so the whole
/// rotating chain is revoked rather than only the token that was replayed.
pub fn revoke_family(db: &Connection, family_id: &str) -> ApiResult<()> {
    db.execute(
        "UPDATE sessions SET revoked_at = COALESCE(revoked_at, ?1), replaced_by = NULL
          WHERE family_id = ?2",
        params![now(), family_id],
    )?;

    Ok(())
}

pub fn revoke(db: &Connection, session_id: &str) -> ApiResult<()> {
    db.execute(
        "UPDATE sessions SET revoked_at = ?1, replaced_by = NULL
          WHERE id = ?2 AND revoked_at IS NULL",
        params![now(), session_id],
    )?;

    Ok(())
}

pub fn revoke_all_for_user(db: &Connection, user_id: &str) -> ApiResult<()> {
    db.execute(
        "UPDATE sessions SET revoked_at = ?1, replaced_by = NULL
          WHERE user_id = ?2 AND revoked_at IS NULL",
        params![now(), user_id],
    )?;

    Ok(())
}

pub fn purge_expired(db: &Connection) -> ApiResult<usize> {
    Ok(db.execute("DELETE FROM sessions WHERE expires_at < ?1", [now()])?)
}

impl Session {
    /// Whether this session may still be *refreshed*.
    pub fn is_usable(&self) -> bool {
        self.revoked_at.is_none() && self.expires_at > now()
    }

    /// Whether an access token minted under this session is still honoured.
    ///
    /// A session that was replaced by rotation is revoked but not dead: the access token it
    /// issued has minutes left on it, and killing it the instant the client rotates would sign
    /// people out mid-request. A session revoked *without* a replacement — a logout, or reuse
    /// detection — carries nothing.
    pub fn carries_access_tokens(&self) -> bool {
        self.expires_at > now() && (self.revoked_at.is_none() || self.replaced_by.is_some())
    }
}
