//! The endpoints the shell needs before any screen can show anything.

use serde_json::json;

use super::models::{Account, CreatedWorkspace, SignIn, Workspace, Workspaces};
use super::{Api, ApiError};

impl Api {
    pub async fn me(&self) -> Result<Account, ApiError> {
        self.get("/account").await
    }

    pub async fn register(
        &self,
        email: &str,
        display_name: &str,
        password: &str,
    ) -> Result<SignIn, ApiError> {
        let answer: SignIn = self
            .post(
                "/auth/register",
                json!({ "email": email, "display_name": display_name, "password": password }),
            )
            .await?;

        Api::set_access_token(answer.access_token.clone());

        Ok(answer)
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> Result<SignIn, ApiError> {
        let answer: SignIn = self
            .post(
                "/auth/login",
                json!({ "email": email, "password": password }),
            )
            .await?;

        // Only a completed sign-in carries one. A challenge deliberately does not.
        if answer.access_token.is_some() {
            Api::set_access_token(answer.access_token.clone());
        }

        Ok(answer)
    }

    pub async fn answer_challenge(
        &self,
        challenge_id: &str,
        code: &str,
    ) -> Result<SignIn, ApiError> {
        let answer: SignIn = self
            .post(
                "/auth/login/totp",
                json!({ "challenge_id": challenge_id, "code": code }),
            )
            .await?;

        Api::set_access_token(answer.access_token.clone());

        Ok(answer)
    }

    pub async fn sign_out(&self) -> Result<(), ApiError> {
        let _: serde_json::Value = self.post("/auth/logout", json!({})).await?;

        Api::set_access_token(None);

        Ok(())
    }

    pub async fn forgot_password(&self, email: &str) -> Result<(), ApiError> {
        let _: serde_json::Value = self
            .post("/auth/password/forgot", json!({ "email": email }))
            .await?;

        Ok(())
    }

    pub async fn reset_password(&self, token: &str, password: &str) -> Result<(), ApiError> {
        let _: serde_json::Value = self
            .post(
                "/auth/password/reset",
                json!({ "token": token, "password": password }),
            )
            .await?;

        Ok(())
    }

    pub async fn workspaces(&self) -> Result<Vec<Workspace>, ApiError> {
        let answer: Workspaces = self.get("/workspaces").await?;

        Ok(answer.workspaces)
    }

    /// The client may name the id: it mints ids offline, and a workspace started before signing
    /// in keeps the id its rows already point at.
    pub async fn create_workspace(
        &self,
        name: &str,
        id: Option<&str>,
    ) -> Result<Workspace, ApiError> {
        let answer: CreatedWorkspace = self
            .post("/workspaces", json!({ "name": name, "id": id }))
            .await?;

        Ok(answer.workspace)
    }
}
