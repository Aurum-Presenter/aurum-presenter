//! Everything the settings screens ask the server: who is in a workspace, who has been invited,
//! and the second factor on an account.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Api, ApiError};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Member {
    pub user_id: String,
    pub display_name: String,
    pub email: String,
    pub role: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Members {
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Invite {
    pub id: String,
    pub email: String,
    pub role: String,
    pub expires_at: String,
    /// The mail server refused it five times. The link still works; it has to be handed over.
    #[serde(default)]
    pub not_sent: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Invites {
    #[serde(default)]
    pub invites: Vec<Invite>,
    #[serde(default)]
    pub pending: Vec<Invite>,
    #[serde(default)]
    pub link: Option<String>,
}

/// What a link says about itself before anybody acts on it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct InvitePreview {
    pub workspace_name: String,
    pub email: String,
    pub role: String,
    #[serde(default)]
    pub used: bool,
    #[serde(default)]
    pub expired: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PreviewedInvite {
    pub invite: InvitePreview,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct JoinedWorkspace {
    pub workspace: super::models::Workspace,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Enrolment {
    pub secret: String,
    pub provisioning_uri: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RecoveryCodes {
    pub recovery_codes: Vec<String>,
}

impl Api {
    pub async fn members(&self, workspace_id: &str) -> Result<Members, ApiError> {
        self.get(&format!("/workspaces/{workspace_id}/members"))
            .await
    }

    pub async fn set_role(
        &self,
        workspace_id: &str,
        user_id: &str,
        role: &str,
    ) -> Result<Members, ApiError> {
        self.patch(
            &format!("/workspaces/{workspace_id}/members/{user_id}"),
            json!({ "role": role }),
        )
        .await
    }

    pub async fn remove_member(
        &self,
        workspace_id: &str,
        user_id: &str,
    ) -> Result<Members, ApiError> {
        self.delete(
            &format!("/workspaces/{workspace_id}/members/{user_id}"),
            json!({}),
        )
        .await
    }

    pub async fn invites(&self, workspace_id: &str) -> Result<Invites, ApiError> {
        self.get(&format!("/workspaces/{workspace_id}/invites"))
            .await
    }

    pub async fn invite(
        &self,
        workspace_id: &str,
        email: &str,
        role: &str,
    ) -> Result<Invites, ApiError> {
        self.post(
            &format!("/workspaces/{workspace_id}/invites"),
            json!({ "email": email, "role": role }),
        )
        .await
    }

    pub async fn revoke_invite(
        &self,
        workspace_id: &str,
        invite_id: &str,
    ) -> Result<Invites, ApiError> {
        self.delete(
            &format!("/workspaces/{workspace_id}/invites/{invite_id}"),
            json!({}),
        )
        .await
    }

    pub async fn preview_invite(&self, token: &str) -> Result<PreviewedInvite, ApiError> {
        self.get(&format!("/invites/{token}")).await
    }

    pub async fn accept_invite(&self, token: &str) -> Result<JoinedWorkspace, ApiError> {
        self.post("/invites/accept", json!({ "token": token }))
            .await
    }

    // -- The second factor ------------------------------------------------------------------

    pub async fn enrol_totp(&self) -> Result<Enrolment, ApiError> {
        self.post("/account/totp/enrol", json!({})).await
    }

    pub async fn confirm_totp(&self, code: &str) -> Result<RecoveryCodes, ApiError> {
        self.post("/account/totp/confirm", json!({ "code": code }))
            .await
    }

    pub async fn disable_totp(&self, password: &str, code: &str) -> Result<(), ApiError> {
        let _: serde_json::Value = self
            .delete(
                "/account/totp",
                json!({ "password": password, "code": code }),
            )
            .await?;

        Ok(())
    }

    pub async fn new_recovery_codes(
        &self,
        password: &str,
        code: &str,
    ) -> Result<RecoveryCodes, ApiError> {
        self.post(
            "/account/totp/recovery-codes",
            json!({ "password": password, "code": code }),
        )
        .await
    }
}
