//! Membership, invitations, and the boundary a non-member cannot see past.

mod support;

use axum::http::StatusCode;
use serde_json::json;
use support::{id, server, upsert};

/// A workspace always has an owner. Removing or demoting the last one would leave content
/// nobody can administer.
#[tokio::test]
async fn the_last_owner_cannot_be_removed_or_demoted() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let band = server.band(&ada, "The Band").await;

    let removed = server
        .request(
            "DELETE",
            &format!("/api/v1/workspaces/{band}/members/{}", ada.id),
            Some(&ada.token),
            None,
        )
        .await;

    assert_eq!(removed.status, StatusCode::CONFLICT);
    assert_eq!(removed.code(), "last_owner");

    let demoted = server
        .request(
            "PATCH",
            &format!("/api/v1/workspaces/{band}/members/{}", ada.id),
            Some(&ada.token),
            Some(json!({ "role": "editor" })),
        )
        .await;

    assert_eq!(demoted.status, StatusCode::CONFLICT);
    assert_eq!(demoted.code(), "last_owner");

    // And the role is unchanged after the attempt.
    let members = server
        .get(&format!("/api/v1/workspaces/{band}/members"), &ada.token)
        .await;

    assert_eq!(members.body["members"][0]["role"], "owner");
}

/// An owner can step down once there is another, and be removed once there is another.
#[tokio::test]
async fn an_owner_can_step_down_once_there_is_a_second_one() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let grace = server.account("grace@example.com").await;
    let band = server.band(&ada, "The Band").await;

    join(&server, &ada, &grace, &band, "editor").await;

    // An owner can delete the whole library, so the account that becomes one must already carry
    // a second factor.
    let refused = promote(&server, &ada, &grace, &band).await;

    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(refused.code(), "totp_required_for_owner");

    enrol_totp(&server, &grace).await;

    assert_eq!(
        promote(&server, &ada, &grace, &band).await.status,
        StatusCode::OK
    );

    let stepped_down = server
        .request(
            "PATCH",
            &format!("/api/v1/workspaces/{band}/members/{}", ada.id),
            Some(&ada.token),
            Some(json!({ "role": "editor" })),
        )
        .await;

    assert_eq!(
        stepped_down.status,
        StatusCode::OK,
        "{:?}",
        stepped_down.body
    );
}

/// Deliberately 404, not 403: whether a workspace exists is itself information a non-member must
/// not be able to probe for.
#[tokio::test]
async fn a_non_member_is_told_it_does_not_exist() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let stranger = server.account("stranger@example.com").await;
    let band = server.band(&ada, "The Band").await;

    let answer = server
        .get(
            &format!("/api/v1/workspaces/{band}/members"),
            &stranger.token,
        )
        .await;

    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    assert_eq!(answer.code(), "workspace_not_found");

    // The same answer for a workspace id that never existed, so the two cannot be told apart.
    let invented = server
        .get(
            &format!("/api/v1/workspaces/{}/members", id()),
            &stranger.token,
        )
        .await;

    assert_eq!(invented.status, StatusCode::NOT_FOUND);
    assert_eq!(invented.code(), "workspace_not_found");
}

/// A denied request never opens a workspace file. The check is before the connection, not after.
#[tokio::test]
async fn a_denied_request_never_opens_a_workspace_file() {
    let server = server().await;
    let stranger = server.account("stranger@example.com").await;
    let invented = id();

    server
        .get(
            &format!("/api/v1/workspaces/{invented}/sync/pull"),
            &stranger.token,
        )
        .await;

    assert!(
        !server.state.db.workspace_exists(&invented),
        "the file was created by a request that was refused"
    );
}

#[tokio::test]
async fn a_viewer_reads_but_cannot_write_shared_content() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let grace = server.account("grace@example.com").await;
    let band = server.band(&ada, "The Band").await;

    join(&server, &ada, &grace, &band, "viewer").await;

    let read = server
        .get(
            &format!("/api/v1/workspaces/{band}/sync/pull?since=0"),
            &grace.token,
        )
        .await;

    assert_eq!(read.status, StatusCode::OK);

    let wrote = server
        .post(
            &format!("/api/v1/workspaces/{band}/sync/push"),
            &grace.token,
            json!({ "ops": [upsert("songs", &id(), json!({ "title": "Not allowed" }))] }),
        )
        .await;

    assert_eq!(wrote.body["results"][0]["status"], "rejected");
    assert_eq!(wrote.body["results"][0]["code"], "insufficient_role");

    // But their own preferences are theirs to write.
    let preference = server
        .post(
            &format!("/api/v1/workspaces/{band}/sync/push"),
            &grace.token,
            json!({ "ops": [upsert("preferences", &id(), json!({
                "user_id": grace.id,
                "scope_type": "workspace",
                "name": "chart",
                "value": "{}",
            }))] }),
        )
        .await;

    assert_eq!(preference.body["results"][0]["status"], "applied");
}

/// Members and invitations are the owner's alone.
#[tokio::test]
async fn an_editor_cannot_manage_members() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let grace = server.account("grace@example.com").await;
    let band = server.band(&ada, "The Band").await;

    join(&server, &ada, &grace, &band, "editor").await;

    let listed = server
        .get(&format!("/api/v1/workspaces/{band}/invites"), &grace.token)
        .await;

    assert_eq!(listed.status, StatusCode::FORBIDDEN);
    assert_eq!(listed.code(), "insufficient_role");
}

// -- Invitations -------------------------------------------------------------------------------

/// The emailed link is the only copy of the token: the row holds a keyed hash.
#[tokio::test]
async fn the_token_is_not_stored_and_the_invite_is_found_by_it() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let band = server.band(&ada, "The Band").await;

    let created = server
        .post(
            &format!("/api/v1/workspaces/{band}/invites"),
            &ada.token,
            json!({ "email": "grace@example.com", "role": "editor" }),
        )
        .await;

    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);

    let token = created.body["link"]
        .as_str()
        .and_then(|link| link.rsplit('/').next())
        .expect("a token")
        .to_owned();

    let stored: i64 = server
        .state
        .db
        .open_control()
        .expect("the control database")
        .query_row(
            "SELECT COUNT(*) FROM invites WHERE token_hash = ?1",
            [&token],
            |row| row.get(0),
        )
        .expect("a count");

    assert_eq!(stored, 0, "the token itself is not in the database");

    let previewed = server
        .request("GET", &format!("/api/v1/invites/{token}"), None, None)
        .await;

    assert_eq!(previewed.body["invite"]["workspace_name"], "The Band");
    assert_eq!(previewed.body["invite"]["email"], "grace@example.com");
    assert_eq!(previewed.body["invite"]["used"], false);
}

#[tokio::test]
async fn accepting_marks_it_used_and_it_cannot_be_used_twice() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let grace = server.account("grace@example.com").await;
    let band = server.band(&ada, "The Band").await;
    let token = invite(&server, &ada, &band, &grace.email, "editor").await;

    let accepted = server
        .post(
            "/api/v1/invites/accept",
            &grace.token,
            json!({ "token": token }),
        )
        .await;

    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.body);
    assert_eq!(accepted.body["role"], "editor");

    let again = server
        .post(
            "/api/v1/invites/accept",
            &grace.token,
            json!({ "token": token }),
        )
        .await;

    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.code(), "invite_used");
}

/// A forwarded link must not let somebody else into a band's library.
#[tokio::test]
async fn an_invitation_belongs_to_the_address_it_was_sent_to() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let grace = server.account("grace@example.com").await;
    let stranger = server.account("stranger@example.com").await;
    let band = server.band(&ada, "The Band").await;
    let token = invite(&server, &ada, &band, &grace.email, "editor").await;

    let answer = server
        .post(
            "/api/v1/invites/accept",
            &stranger.token,
            json!({ "token": token }),
        )
        .await;

    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.code(), "invite_wrong_account");
}

/// Resending is the common case, not an error.
#[tokio::test]
async fn inviting_the_same_address_again_replaces_the_first_invite() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let band = server.band(&ada, "The Band").await;

    let first = invite(&server, &ada, &band, "grace@example.com", "editor").await;
    let second = invite(&server, &ada, &band, "grace@example.com", "viewer").await;

    assert_ne!(first, second);

    let listed = server
        .get(&format!("/api/v1/workspaces/{band}/invites"), &ada.token)
        .await;

    assert_eq!(listed.body["invites"].as_array().unwrap().len(), 1);
    assert_eq!(listed.body["invites"][0]["role"], "viewer");

    let stale = server
        .request("GET", &format!("/api/v1/invites/{first}"), None, None)
        .await;

    assert_eq!(stale.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn revoking_stops_the_link_working() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let grace = server.account("grace@example.com").await;
    let band = server.band(&ada, "The Band").await;
    let token = invite(&server, &ada, &band, &grace.email, "editor").await;

    let listed = server
        .get(&format!("/api/v1/workspaces/{band}/invites"), &ada.token)
        .await;
    let invite_id = listed.body["invites"][0]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    let revoked = server
        .request(
            "DELETE",
            &format!("/api/v1/workspaces/{band}/invites/{invite_id}"),
            Some(&ada.token),
            None,
        )
        .await;

    assert_eq!(revoked.status, StatusCode::OK);
    assert!(revoked.body["invites"].as_array().unwrap().is_empty());

    let accepted = server
        .post(
            "/api/v1/invites/accept",
            &grace.token,
            json!({ "token": token }),
        )
        .await;

    assert_eq!(accepted.status, StatusCode::NOT_FOUND);
    assert_eq!(accepted.code(), "invite_not_found");
}

/// Ownership is granted afterwards, once the person has a second factor — so it cannot arrive by
/// invitation to an account with none.
#[tokio::test]
async fn an_invitation_cannot_grant_ownership() {
    let server = server().await;
    let ada = server.account("ada@example.com").await;
    let band = server.band(&ada, "The Band").await;

    let answer = server
        .post(
            &format!("/api/v1/workspaces/{band}/invites"),
            &ada.token,
            json!({ "email": "grace@example.com", "role": "owner" }),
        )
        .await;

    assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(answer.body["error"]["details"]["field"], "role");
}

// -- Helpers -------------------------------------------------------------------------------

async fn invite(
    server: &support::Server,
    owner: &support::Account,
    band: &str,
    email: &str,
    role: &str,
) -> String {
    let created = server
        .post(
            &format!("/api/v1/workspaces/{band}/invites"),
            &owner.token,
            json!({ "email": email, "role": role }),
        )
        .await;

    created.body["link"]
        .as_str()
        .and_then(|link| link.rsplit('/').next())
        .expect("a token")
        .to_owned()
}

async fn join(
    server: &support::Server,
    owner: &support::Account,
    guest: &support::Account,
    band: &str,
    role: &str,
) {
    let token = invite(server, owner, band, &guest.email, role).await;
    let accepted = server
        .post(
            "/api/v1/invites/accept",
            &guest.token,
            json!({ "token": token }),
        )
        .await;

    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.body);
}

async fn promote(
    server: &support::Server,
    owner: &support::Account,
    target: &support::Account,
    band: &str,
) -> support::Answer {
    server
        .request(
            "PATCH",
            &format!("/api/v1/workspaces/{band}/members/{}", target.id),
            Some(&owner.token),
            Some(json!({ "role": "owner" })),
        )
        .await
}

async fn enrol_totp(server: &support::Server, account: &support::Account) {
    use totp_rs::{Algorithm, TOTP};

    let secret = server
        .post("/api/v1/account/totp/enrol", &account.token, json!({}))
        .await
        .body["secret"]
        .as_str()
        .expect("a secret")
        .to_owned();

    let bytes = base32::decode(base32::Alphabet::Rfc4648 { padding: false }, &secret)
        .expect("a base32 secret");
    let code = TOTP::new(Algorithm::SHA1, 6, 1, 30, bytes, None, String::new())
        .expect("a totp")
        .generate_current()
        .expect("a code");

    let confirmed = server
        .post(
            "/api/v1/account/totp/confirm",
            &account.token,
            json!({ "code": code }),
        )
        .await;

    assert_eq!(confirmed.status, StatusCode::OK, "{:?}", confirmed.body);
}
