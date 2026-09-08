//! The shapes the API answers with. Only what the client actually reads.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub role: String,
    pub created_at: String,
}

impl Workspace {
    pub fn can_edit(&self) -> bool {
        self.role != "viewer"
    }

    pub fn can_manage(&self) -> bool {
        self.role == "owner"
    }

    pub fn is_personal(&self) -> bool {
        self.kind == "personal"
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct Totp {
    pub enrolled: bool,
    pub required: bool,
    pub recovery_codes_remaining: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub display_name: String,
    #[serde(default)]
    pub totp: Totp,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
}

/// What a sign-in came back with: a session, a challenge, or a demand to enrol first.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SignIn {
    pub access_token: Option<String>,
    #[serde(default)]
    pub totp_required: bool,
    pub challenge_id: Option<String>,
    #[serde(default)]
    pub totp_enrolment_required: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Enrolment {
    pub secret: String,
    pub provisioning_uri: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Confirmed {
    pub recovery_codes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Workspaces {
    pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreatedWorkspace {
    pub workspace: Workspace,
}
