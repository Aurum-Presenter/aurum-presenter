//! Workspaces, their members, and the invitations that add one.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::db::blocking;
use crate::error::{ApiError, ApiResult};
use crate::extract::{
    Body, Caller, Manage, Read, Workspace, object, optional_string, require_string,
};
use crate::repo::workspaces::Role;
use crate::repo::{accounts, invites, workspaces};
use crate::state::AppState;

pub async fn list(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Value>> {
    blocking(move || {
        let db = state.db.open_control()?;

        Ok(Json(json!({
            "workspaces": workspaces::for_user(&db, &caller.user.id)?,
        })))
    })
    .await
}

/// Creating a band workspace. The client may name the id, because it mints ids offline and a
/// workspace it created before it had signal has to keep the id its rows already point at.
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Body(body): Body<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let body = object(&body);
    let name = require_string(&body, "name", 120)?;
    let requested = optional_string(&body, "id");

    if let Some(id) = &requested
        && !aurum_core::ids::is_uuid(id)
    {
        return Err(ApiError::validation("id", "\"id\" must be a UUID."));
    }

    blocking(move || {
        let mut db = state.db.open_control()?;

        if let Some(id) = &requested
            && (workspaces::find(&db, id)?.is_some() || state.db.workspace_exists(id))
        {
            return Err(ApiError::conflict("That workspace id is already in use.")
                .with_code("workspace_exists"));
        }

        let workspace =
            workspaces::create_with_id(&mut db, requested, &name, "band", &caller.user.id)?;

        state.db.open_workspace(&workspace.id)?;

        Ok((StatusCode::CREATED, Json(json!({ "workspace": workspace }))))
    })
    .await
}

pub async fn members(
    State(state): State<AppState>,
    workspace: Workspace<Read>,
) -> ApiResult<Json<Value>> {
    blocking(move || {
        let db = state.db.open_control()?;

        Ok(Json(json!({
            "members": workspaces::members(&db, &workspace.id)?,
            "invites": invites::pending(&db, &workspace.id)?,
        })))
    })
    .await
}

pub async fn update_member(
    State(state): State<AppState>,
    workspace: Workspace<Manage>,
    Path((_, target_id)): Path<(String, String)>,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let role = Role::parse(&require_string(&object(&body), "role", 16)?)
        .ok_or_else(|| ApiError::validation("role", "Role must be owner, editor or viewer."))?;

    blocking(move || {
        let db = state.db.open_control()?;

        if workspaces::role_of(&db, &target_id, &workspace.id)?.is_none() {
            return Err(
                ApiError::not_found("That person is not a member of this workspace.")
                    .with_code("not_a_member"),
            );
        }

        // An owner can delete the whole library, so the account that becomes one must already
        // carry a second factor — not be asked to add one afterwards.
        if role == Role::Owner && accounts::confirmed_totp(&db, &target_id)?.is_none() {
            return Err(ApiError::conflict(
                "That member must enable two-factor authentication before they can be made an owner.",
            )
            .with_code("totp_required_for_owner"));
        }

        assert_not_last_owner(&db, &workspace.id, &target_id, Some(role))?;
        workspaces::update_member_role(&db, &workspace.id, &target_id, role)?;

        Ok(Json(json!({ "members": workspaces::members(&db, &workspace.id)? })))
    })
    .await
}

pub async fn remove_member(
    State(state): State<AppState>,
    workspace: Workspace<Manage>,
    Path((_, target_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    blocking(move || {
        let db = state.db.open_control()?;

        if workspaces::role_of(&db, &target_id, &workspace.id)?.is_none() {
            return Err(
                ApiError::not_found("That person is not a member of this workspace.")
                    .with_code("not_a_member"),
            );
        }

        assert_not_last_owner(&db, &workspace.id, &target_id, None)?;
        workspaces::remove_member(&db, &workspace.id, &target_id)?;

        // Their per-user rows in the content tier go with them: a preference belonging to
        // somebody who is no longer here is nobody's.
        workspace
            .open(&state)?
            .execute("DELETE FROM preferences WHERE user_id = ?1", [&target_id])?;

        Ok(Json(
            json!({ "members": workspaces::members(&db, &workspace.id)? }),
        ))
    })
    .await
}

/// A workspace always has an owner. Removing or demoting the last one would leave content nobody
/// can administer, so both are refused in the same place.
fn assert_not_last_owner(
    db: &rusqlite::Connection,
    workspace_id: &str,
    target_id: &str,
    becoming: Option<Role>,
) -> ApiResult<()> {
    let leaving = workspaces::role_of(db, target_id, workspace_id)? == Some(Role::Owner)
        && becoming != Some(Role::Owner);

    if leaving && workspaces::owner_count(db, workspace_id)? <= 1 {
        return Err(ApiError::conflict(
            "A workspace needs an owner. Make somebody else an owner first.",
        )
        .with_code("last_owner"));
    }

    Ok(())
}

// -- Invitations -------------------------------------------------------------------------------

pub async fn list_invites(
    State(state): State<AppState>,
    workspace: Workspace<Manage>,
) -> ApiResult<Json<Value>> {
    blocking(move || {
        let db = state.db.open_control()?;
        let pending = invites::pending(&db, &workspace.id)?;
        let stuck: Vec<String> = invites::undelivered_recipients(&db)?;

        let listed: Vec<Value> = pending
            .iter()
            .map(|invite| {
                let mut value = serde_json::to_value(invite).unwrap_or_default();
                value["not_sent"] = json!(stuck.contains(&invite.email));

                value
            })
            .collect();

        Ok(Json(json!({ "invites": listed })))
    })
    .await
}

pub async fn create_invite(
    State(state): State<AppState>,
    workspace: Workspace<Manage>,
    Body(body): Body<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let body = object(&body);
    let email = require_string(&body, "email", 320)?.to_lowercase();
    let role = Role::parse(&require_string(&body, "role", 16)?);

    if !super::auth::looks_like_an_email(&email) {
        return Err(ApiError::validation(
            "email",
            "That is not an email address.",
        ));
    }

    // Ownership is granted afterwards, once the person has a second factor — so it cannot be
    // handed out by an invitation that reaches an account with no 2FA at all.
    let Some(role) = role.filter(|role| *role != Role::Owner) else {
        return Err(ApiError::validation(
            "role",
            "Invite as editor or viewer; ownership is granted afterwards, once they have \
             two-factor authentication.",
        ));
    };

    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:5173".to_owned());

    blocking(move || {
        let db = state.db.open_control()?;
        let inviter = &workspace.caller.user.display_name;
        let name = &workspace.record.name;

        let (id, token) = invites::create(
            &db,
            &state.config.auth.signing_key,
            &workspace.id,
            &email,
            role,
            &workspace.caller.user.id,
        )?;

        let link = format!("{}/invite/{token}", app_url.trim_end_matches('/'));
        let days = invites::TTL_DAYS;

        invites::enqueue_mail(
            &db,
            &email,
            &format!("{inviter} invited you to {name} on Aurum"),
            &format!(
                "<p>{} has invited you to join <strong>{}</strong> on Aurum Presenter.</p>\
                 <p><a href=\"{link}\">Accept the invitation</a></p>\
                 <p>The link works once and expires in {days} days.</p>",
                escape(inviter),
                escape(name),
            ),
            &format!(
                "{inviter} has invited you to join {name} on Aurum Presenter.\n\n{link}\n\n\
                 The link works once and expires in {days} days.\n"
            ),
        )?;

        Ok((
            StatusCode::CREATED,
            Json(json!({
                "invite": { "id": id, "email": email, "role": role },
                // Returned so the inviter can hand it over directly — a band in a rehearsal room
                // should not have to wait for email to arrive.
                "link": link,
                "pending": invites::pending(&db, &workspace.id)?,
            })),
        ))
    })
    .await
}

pub async fn revoke_invite(
    State(state): State<AppState>,
    workspace: Workspace<Manage>,
    Path((_, invite_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    blocking(move || {
        let db = state.db.open_control()?;

        if !invites::revoke(&db, &workspace.id, &invite_id)? {
            return Err(
                ApiError::not_found("That invitation has already been used or withdrawn.")
                    .with_code("invite_not_found"),
            );
        }

        Ok(Json(
            json!({ "invites": invites::pending(&db, &workspace.id)? }),
        ))
    })
    .await
}

/// Readable without signing in: somebody following the link has to be able to see what they are
/// being asked to join before they decide to make an account.
pub async fn preview_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<Json<Value>> {
    blocking(move || {
        let db = state.db.open_control()?;

        let invite = invites::find_by_token(&db, &state.config.auth.signing_key, &token)?
            .ok_or_else(|| {
                ApiError::not_found("That invitation link is not valid.")
                    .with_code("invite_not_found")
            })?;

        Ok(Json(json!({
            "invite": {
                "workspace_name": invite.workspace_name,
                "email": invite.email,
                "role": invite.role,
                "expires_at": invite.expires_at,
                "used": invite.accepted_at.is_some(),
                "expired": invite.is_expired(),
            },
        })))
    })
    .await
}

pub async fn accept_invite(
    State(state): State<AppState>,
    caller: Caller,
    Body(body): Body<Value>,
) -> ApiResult<Json<Value>> {
    let token = require_string(&object(&body), "token", 128)?;

    blocking(move || {
        let db = state.db.open_control()?;

        let invite = invites::find_by_token(&db, &state.config.auth.signing_key, &token)?
            .ok_or_else(|| {
                ApiError::not_found("That invitation link is not valid.")
                    .with_code("invite_not_found")
            })?;

        if invite.accepted_at.is_some() {
            return Err(ApiError::conflict("That invitation has already been used.")
                .with_code("invite_used"));
        }

        if invite.is_expired() {
            return Err(
                ApiError::conflict("That invitation has expired. Ask for a new one.")
                    .with_code("invite_expired"),
            );
        }

        // The invitation names an address, and it is that account's to accept. Otherwise a
        // forwarded link would let anyone into a band's library.
        if !invite.email.eq_ignore_ascii_case(&caller.user.email) {
            return Err(ApiError::forbidden(format!(
                "That invitation was sent to {}. Sign in as that account to accept it.",
                invite.email
            ))
            .with_code("invite_wrong_account"));
        }

        if workspaces::role_of(&db, &caller.user.id, &invite.workspace_id)?.is_none() {
            workspaces::add_member(&db, &invite.workspace_id, &caller.user.id, invite.role)?;
        }

        invites::accept(&db, &invite.id)?;

        Ok(Json(json!({
            "workspace": workspaces::find(&db, &invite.workspace_id)?,
            "role": invite.role,
        })))
    })
    .await
}

/// Enough to keep a display name out of the markup around it.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_display_name_out_of_the_markup_around_it() {
        assert_eq!(
            escape("<script>alert(\"x\")</script> & co"),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; co"
        );
    }
}
