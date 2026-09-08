//! Invitations to a workspace, and the queue the emails carrying them wait in.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use rand::RngCore;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::Sha256;

use super::{in_seconds, new_id};
use crate::db::now;
use crate::error::ApiResult;
use crate::repo::workspaces::Role;

pub const TTL_DAYS: i64 = 14;
const MAX_MAIL_ATTEMPTS: i64 = 5;

#[derive(Clone, Debug, Serialize)]
pub struct PendingInvite {
    pub id: String,
    pub email: String,
    pub role: Role,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct Invite {
    pub id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub email: String,
    pub role: Role,
    pub expires_at: String,
    pub accepted_at: Option<String>,
}

/// Stored as a keyed hash, never in the clear: the emailed link is the only copy of the token.
fn hash(signing_key: &str, token: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(signing_key.as_bytes())
        .expect("HMAC takes a key of any length");
    mac.update(token.as_bytes());

    hex::encode(mac.finalize().into_bytes())
}

pub fn create(
    db: &Connection,
    signing_key: &str,
    workspace_id: &str,
    email: &str,
    role: Role,
    invited_by: &str,
) -> ApiResult<(String, String)> {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);

    let token = URL_SAFE_NO_PAD.encode(bytes);
    let id = new_id();

    // A second invite to the same address replaces the first rather than colliding with the
    // partial unique index — resending is the common case, not an error.
    db.execute(
        "DELETE FROM invites WHERE workspace_id = ?1 AND email = ?2 AND accepted_at IS NULL",
        params![workspace_id, email],
    )?;
    db.execute(
        "INSERT INTO invites
            (id, workspace_id, email, role, token_hash, invited_by, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            workspace_id,
            email,
            role.as_str(),
            hash(signing_key, &token),
            invited_by,
            in_seconds(TTL_DAYS * 86_400),
            now()
        ],
    )?;

    Ok((id, token))
}

pub fn pending(db: &Connection, workspace_id: &str) -> ApiResult<Vec<PendingInvite>> {
    let mut statement = db.prepare(
        "SELECT id, email, role, expires_at, created_at FROM invites
          WHERE workspace_id = ?1 AND accepted_at IS NULL
          ORDER BY created_at DESC",
    )?;

    let rows = statement
        .query_map([workspace_id], |row| {
            Ok(PendingInvite {
                id: row.get("id")?,
                email: row.get("email")?,
                role: Role::parse(&row.get::<_, String>("role")?).unwrap_or(Role::Viewer),
                expires_at: row.get("expires_at")?,
                created_at: row.get("created_at")?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

pub fn find_by_token(db: &Connection, signing_key: &str, token: &str) -> ApiResult<Option<Invite>> {
    Ok(db
        .query_row(
            "SELECT i.*, w.name AS workspace_name FROM invites i
               JOIN workspaces w ON w.id = i.workspace_id
              WHERE i.token_hash = ?1",
            [hash(signing_key, token)],
            |row| {
                Ok(Invite {
                    id: row.get("id")?,
                    workspace_id: row.get("workspace_id")?,
                    workspace_name: row.get("workspace_name")?,
                    email: row.get("email")?,
                    role: Role::parse(&row.get::<_, String>("role")?).unwrap_or(Role::Viewer),
                    expires_at: row.get("expires_at")?,
                    accepted_at: row.get("accepted_at")?,
                })
            },
        )
        .optional()?)
}

pub fn accept(db: &Connection, invite_id: &str) -> ApiResult<()> {
    db.execute(
        "UPDATE invites SET accepted_at = ?1 WHERE id = ?2",
        params![now(), invite_id],
    )?;

    Ok(())
}

pub fn revoke(db: &Connection, workspace_id: &str, invite_id: &str) -> ApiResult<bool> {
    let affected = db.execute(
        "DELETE FROM invites WHERE id = ?1 AND workspace_id = ?2 AND accepted_at IS NULL",
        params![invite_id, workspace_id],
    )?;

    Ok(affected > 0)
}

impl Invite {
    pub fn is_expired(&self) -> bool {
        self.expires_at <= now()
    }
}

// -- Mail ------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct QueuedMail {
    pub id: String,
    pub recipient: String,
    pub subject: String,
    pub body_html: String,
    pub body_text: String,
    pub attempts: i64,
}

/// Queued rather than sent inline: an SMTP server that is slow or down must not make signing up
/// slow or impossible.
pub fn enqueue_mail(
    db: &Connection,
    recipient: &str,
    subject: &str,
    html: &str,
    text: &str,
) -> ApiResult<String> {
    let id = new_id();

    db.execute(
        "INSERT INTO mail_queue (id, recipient, subject, body_html, body_text, attempts, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
        params![id, recipient, subject, html, text, now()],
    )?;

    Ok(id)
}

pub fn unsent_mail(db: &Connection, limit: i64) -> ApiResult<Vec<QueuedMail>> {
    let mut statement = db.prepare(
        "SELECT * FROM mail_queue WHERE sent_at IS NULL AND attempts < ?1
          ORDER BY created_at LIMIT ?2",
    )?;

    let rows = statement
        .query_map(params![MAX_MAIL_ATTEMPTS, limit], |row| {
            Ok(QueuedMail {
                id: row.get("id")?,
                recipient: row.get("recipient")?,
                subject: row.get("subject")?,
                body_html: row.get("body_html")?,
                body_text: row.get("body_text")?,
                attempts: row.get("attempts")?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// Addresses whose invitation email has not gone out, so the list can say so rather than let a
/// band wonder why nobody arrived.
pub fn undelivered_recipients(db: &Connection) -> ApiResult<Vec<String>> {
    let mut statement =
        db.prepare("SELECT DISTINCT recipient FROM mail_queue WHERE sent_at IS NULL")?;

    let rows = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    Ok(rows)
}

pub fn mark_mail_sent(db: &Connection, id: &str) -> ApiResult<()> {
    db.execute(
        "UPDATE mail_queue SET sent_at = ?1 WHERE id = ?2",
        params![now(), id],
    )?;

    Ok(())
}

/// Five attempts, then it stops trying and stays visible as undelivered rather than
/// disappearing into a log nobody reads.
pub fn mark_mail_failed(db: &Connection, id: &str, attempts: i64, error: &str) -> ApiResult<()> {
    db.execute(
        "UPDATE mail_queue SET attempts = ?1, last_error = ?2 WHERE id = ?3",
        params![attempts + 1, error, id],
    )?;

    Ok(())
}
