//! Accounts, credentials, second factors, and the rate limiter that guards them.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::{in_seconds, new_id};
use crate::db::now;
use crate::error::ApiResult;

#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub email_verified_at: Option<String>,
    pub created_at: String,
}

fn user_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get("id")?,
        email: row.get("email")?,
        display_name: row.get("display_name")?,
        email_verified_at: row.get("email_verified_at")?,
        created_at: row.get("created_at")?,
    })
}

pub fn find_by_email(db: &Connection, email: &str) -> ApiResult<Option<User>> {
    Ok(db
        .query_row("SELECT * FROM users WHERE email = ?1", [email], user_from)
        .optional()?)
}

pub fn find_by_id(db: &Connection, id: &str) -> ApiResult<Option<User>> {
    Ok(db
        .query_row("SELECT * FROM users WHERE id = ?1", [id], user_from)
        .optional()?)
}

pub fn create(
    db: &mut Connection,
    email: &str,
    display_name: &str,
    hash: &str,
) -> ApiResult<String> {
    let id = new_id();
    let now = now();
    let transaction = db.transaction()?;

    transaction.execute(
        "INSERT INTO users (id, email, display_name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![id, email, display_name, now],
    )?;
    transaction.execute(
        "INSERT INTO credentials (user_id, password_hash, updated_at) VALUES (?1, ?2, ?3)",
        params![id, hash, now],
    )?;
    transaction.commit()?;

    Ok(id)
}

pub fn password_hash(db: &Connection, user_id: &str) -> ApiResult<Option<String>> {
    Ok(db
        .query_row(
            "SELECT password_hash FROM credentials WHERE user_id = ?1",
            [user_id],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn update_password_hash(db: &Connection, user_id: &str, hash: &str) -> ApiResult<()> {
    db.execute(
        "UPDATE credentials SET password_hash = ?1, updated_at = ?2 WHERE user_id = ?3",
        params![hash, now(), user_id],
    )?;

    Ok(())
}

// -- Second factor -------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct TotpSecret {
    pub secret_encrypted: Vec<u8>,
    pub last_accepted_step: Option<i64>,
    pub confirmed_at: Option<String>,
}

fn totp_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<TotpSecret> {
    Ok(TotpSecret {
        secret_encrypted: row.get("secret_encrypted")?,
        last_accepted_step: row.get("last_accepted_step")?,
        confirmed_at: row.get("confirmed_at")?,
    })
}

pub fn confirmed_totp(db: &Connection, user_id: &str) -> ApiResult<Option<TotpSecret>> {
    Ok(db
        .query_row(
            "SELECT * FROM totp_secrets WHERE user_id = ?1 AND confirmed_at IS NOT NULL",
            [user_id],
            totp_from,
        )
        .optional()?)
}

pub fn pending_totp(db: &Connection, user_id: &str) -> ApiResult<Option<TotpSecret>> {
    Ok(db
        .query_row(
            "SELECT * FROM totp_secrets WHERE user_id = ?1",
            [user_id],
            totp_from,
        )
        .optional()?)
}

/// Re-enrolling replaces any half-finished attempt: an abandoned enrolment must never leave an
/// account holding a second factor its owner does not know about.
pub fn stage_totp_secret(db: &Connection, user_id: &str, encrypted: &[u8]) -> ApiResult<()> {
    db.execute("DELETE FROM totp_secrets WHERE user_id = ?1", [user_id])?;
    db.execute(
        "INSERT INTO totp_secrets (user_id, secret_encrypted, created_at) VALUES (?1, ?2, ?3)",
        params![user_id, encrypted, now()],
    )?;

    Ok(())
}

pub fn confirm_totp(db: &Connection, user_id: &str, step: i64) -> ApiResult<()> {
    db.execute(
        "UPDATE totp_secrets SET confirmed_at = ?1, last_accepted_step = ?2 WHERE user_id = ?3",
        params![now(), step, user_id],
    )?;

    Ok(())
}

pub fn record_totp_step(db: &Connection, user_id: &str, step: i64) -> ApiResult<()> {
    db.execute(
        "UPDATE totp_secrets SET last_accepted_step = ?1 WHERE user_id = ?2",
        params![step, user_id],
    )?;

    Ok(())
}

pub fn remove_totp(db: &Connection, user_id: &str) -> ApiResult<()> {
    db.execute("DELETE FROM totp_secrets WHERE user_id = ?1", [user_id])?;
    db.execute("DELETE FROM recovery_codes WHERE user_id = ?1", [user_id])?;

    Ok(())
}

pub fn replace_recovery_codes(
    db: &mut Connection,
    user_id: &str,
    hashes: &[String],
) -> ApiResult<()> {
    let now = now();
    let transaction = db.transaction()?;

    transaction.execute("DELETE FROM recovery_codes WHERE user_id = ?1", [user_id])?;

    for hash in hashes {
        transaction.execute(
            "INSERT INTO recovery_codes (id, user_id, code_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![new_id(), user_id, hash, now],
        )?;
    }

    transaction.commit()?;

    Ok(())
}

/// True only if the code existed and had not been used: a recovery code is single-use, and the
/// check and the spend are the same statement so two racing attempts cannot both win.
pub fn consume_recovery_code(db: &Connection, user_id: &str, hash: &str) -> ApiResult<bool> {
    let affected = db.execute(
        "UPDATE recovery_codes SET used_at = ?1
          WHERE user_id = ?2 AND code_hash = ?3 AND used_at IS NULL",
        params![now(), user_id, hash],
    )?;

    Ok(affected == 1)
}

pub fn count_unused_recovery_codes(db: &Connection, user_id: &str) -> ApiResult<i64> {
    Ok(db.query_row(
        "SELECT COUNT(*) FROM recovery_codes WHERE user_id = ?1 AND used_at IS NULL",
        [user_id],
        |row| row.get(0),
    )?)
}

// -- The half-signed-in state between a password and a code ----------------------------------

#[derive(Clone, Debug)]
pub struct Challenge {
    pub id: String,
    pub user_id: String,
    pub attempts: i64,
    pub expires_at: String,
}

pub fn create_challenge(db: &Connection, user_id: &str, ttl_seconds: i64) -> ApiResult<String> {
    let id = new_id();

    db.execute(
        "INSERT INTO totp_challenges (id, user_id, expires_at, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, user_id, in_seconds(ttl_seconds), now()],
    )?;

    Ok(id)
}

pub fn find_challenge(db: &Connection, id: &str) -> ApiResult<Option<Challenge>> {
    Ok(db
        .query_row("SELECT * FROM totp_challenges WHERE id = ?1", [id], |row| {
            Ok(Challenge {
                id: row.get("id")?,
                user_id: row.get("user_id")?,
                attempts: row.get("attempts")?,
                expires_at: row.get("expires_at")?,
            })
        })
        .optional()?)
}

pub fn count_challenge_attempt(db: &Connection, id: &str) -> ApiResult<i64> {
    db.execute(
        "UPDATE totp_challenges SET attempts = attempts + 1 WHERE id = ?1",
        [id],
    )?;

    Ok(db.query_row(
        "SELECT attempts FROM totp_challenges WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?)
}

pub fn delete_challenge(db: &Connection, id: &str) -> ApiResult<()> {
    db.execute("DELETE FROM totp_challenges WHERE id = ?1", [id])?;

    Ok(())
}

// -- Rate limiting -------------------------------------------------------------------------

pub fn record_login_attempt(
    db: &Connection,
    email: &str,
    ip: Option<&str>,
    successful: bool,
) -> ApiResult<()> {
    db.execute(
        "INSERT INTO login_attempts (id, email, ip, successful, at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![new_id(), email, ip, i64::from(successful), now()],
    )?;

    Ok(())
}

/// Counted by email *or* address: guessing one password across many accounts is the attack the
/// per-email count alone would miss.
pub fn recent_failures(
    db: &Connection,
    email: &str,
    ip: Option<&str>,
    within_seconds: i64,
) -> ApiResult<i64> {
    Ok(db.query_row(
        "SELECT COUNT(*) FROM login_attempts
          WHERE successful = 0 AND at > ?1 AND (email = ?2 OR (ip IS NOT NULL AND ip = ?3))",
        params![in_seconds(-within_seconds), email, ip],
        |row| row.get(0),
    )?)
}

pub fn clear_failures(db: &Connection, email: &str) -> ApiResult<()> {
    db.execute(
        "DELETE FROM login_attempts WHERE email = ?1 AND successful = 0",
        [email],
    )?;

    Ok(())
}

// -- Password resets -------------------------------------------------------------------------

pub fn create_password_reset(
    db: &Connection,
    user_id: &str,
    token_hash: &str,
    ttl_seconds: i64,
) -> ApiResult<()> {
    db.execute(
        "INSERT INTO password_resets (id, user_id, token_hash, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            new_id(),
            user_id,
            token_hash,
            in_seconds(ttl_seconds),
            now()
        ],
    )?;

    Ok(())
}

/// Spends the reset if it is unused and unexpired, and returns whose it was.
pub fn consume_password_reset(db: &Connection, token_hash: &str) -> ApiResult<Option<String>> {
    let found: Option<String> = db
        .query_row(
            "SELECT user_id FROM password_resets
              WHERE token_hash = ?1 AND used_at IS NULL AND expires_at > ?2",
            params![token_hash, now()],
            |row| row.get(0),
        )
        .optional()?;

    if found.is_some() {
        db.execute(
            "UPDATE password_resets SET used_at = ?1 WHERE token_hash = ?2",
            params![now(), token_hash],
        )?;
    }

    Ok(found)
}
