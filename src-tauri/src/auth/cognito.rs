//! Thin AWS Cognito Identity Provider client.
//!
//! We only need three unauthenticated `AWSCognitoIdentityProviderService.*`
//! JSON calls, so we hand-roll them over `reqwest` rather than pulling in the
//! full `aws-sdk-cognitoidentityprovider` (which drags aws-config + credential
//! providers + hyper machinery we never use). This matches the Go app's actual
//! on-wire behavior: `USER_PASSWORD_AUTH` (no SRP), `NEW_PASSWORD_REQUIRED`
//! challenge completion, and `REFRESH_TOKEN_AUTH`.

use serde::Deserialize;
use serde_json::json;

use crate::error::AppError;

const CONTENT_TYPE: &str = "application/x-amz-json-1.1";
const TARGET_INITIATE: &str = "AWSCognitoIdentityProviderService.InitiateAuth";
const TARGET_RESPOND: &str = "AWSCognitoIdentityProviderService.RespondToAuthChallenge";

/// Raw tokens as returned by Cognito.
#[derive(Debug, Clone)]
pub struct RawTokens {
    pub id_token: String,
    pub access_token: String,
    /// Absent on refresh responses — the caller keeps the prior refresh token.
    pub refresh_token: Option<String>,
    pub expires_in: i64,
}

/// Result of an initial login attempt.
pub enum AuthOutcome {
    Tokens(RawTokens),
    /// First-login flow: the user must set a permanent password. The `session`
    /// is held backend-side and passed to `respond_new_password` — it never
    /// crosses the IPC boundary to the webview.
    NewPasswordRequired { session: String },
}

pub struct Cognito {
    http: reqwest::Client,
    endpoint: String,
    client_id: String,
}

impl Cognito {
    pub fn new(http: reqwest::Client, region: &str, client_id: &str) -> Self {
        Self {
            http,
            endpoint: format!("https://cognito-idp.{region}.amazonaws.com/"),
            client_id: client_id.to_string(),
        }
    }

    /// `USER_PASSWORD_AUTH` — returns tokens, or a `NEW_PASSWORD_REQUIRED`
    /// challenge for first login.
    pub async fn initiate_password_auth(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AuthOutcome, AppError> {
        let body = json!({
            "AuthFlow": "USER_PASSWORD_AUTH",
            "ClientId": self.client_id,
            "AuthParameters": { "USERNAME": username, "PASSWORD": password },
        });
        let resp: InitiateAuthResponse = self.call(TARGET_INITIATE, &body).await?;

        if let Some(result) = resp.authentication_result {
            return Ok(AuthOutcome::Tokens(result.into()));
        }
        match resp.challenge_name.as_deref() {
            Some("NEW_PASSWORD_REQUIRED") => {
                let session = resp.session.ok_or_else(|| {
                    AppError::Message("Cognito omitted challenge session".into())
                })?;
                Ok(AuthOutcome::NewPasswordRequired { session })
            }
            Some(other) => Err(AppError::Message(format!(
                "Unsupported sign-in challenge: {other}"
            ))),
            None => Err(AppError::Message("Cognito returned no tokens".into())),
        }
    }

    /// Complete the `NEW_PASSWORD_REQUIRED` challenge with the user's new
    /// permanent password.
    pub async fn respond_new_password(
        &self,
        username: &str,
        new_password: &str,
        session: &str,
    ) -> Result<RawTokens, AppError> {
        let body = json!({
            "ChallengeName": "NEW_PASSWORD_REQUIRED",
            "ClientId": self.client_id,
            "ChallengeResponses": { "USERNAME": username, "NEW_PASSWORD": new_password },
            "Session": session,
        });
        let resp: InitiateAuthResponse = self.call(TARGET_RESPOND, &body).await?;
        resp.authentication_result
            .map(Into::into)
            .ok_or_else(|| AppError::Message("Password set but no tokens returned".into()))
    }

    /// `REFRESH_TOKEN_AUTH` — exchange a refresh token for fresh access/ID
    /// tokens. Cognito does not return a new refresh token here.
    pub async fn refresh(&self, refresh_token: &str) -> Result<RawTokens, AppError> {
        let body = json!({
            "AuthFlow": "REFRESH_TOKEN_AUTH",
            "ClientId": self.client_id,
            "AuthParameters": { "REFRESH_TOKEN": refresh_token },
        });
        let resp: InitiateAuthResponse = self.call(TARGET_INITIATE, &body).await?;
        resp.authentication_result
            .map(Into::into)
            .ok_or_else(|| AppError::Unauthorized)
    }

    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        target: &str,
        body: &serde_json::Value,
    ) -> Result<T, AppError> {
        let resp = self
            .http
            .post(&self.endpoint)
            .header("Content-Type", CONTENT_TYPE)
            .header("X-Amz-Target", target)
            .body(serde_json::to_vec(body).expect("serialize cognito body"))
            .send()
            .await
            .map_err(|e| AppError::Message(format!("Network error contacting Cognito: {e}")))?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Message(format!("Failed reading Cognito response: {e}")))?;

        if status.is_success() {
            serde_json::from_slice::<T>(&bytes)
                .map_err(|e| AppError::Message(format!("Malformed Cognito response: {e}")))
        } else {
            Err(map_cognito_error(&bytes))
        }
    }
}

impl From<AuthenticationResult> for RawTokens {
    fn from(r: AuthenticationResult) -> Self {
        RawTokens {
            id_token: r.id_token,
            access_token: r.access_token,
            refresh_token: r.refresh_token,
            expires_in: r.expires_in,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InitiateAuthResponse {
    authentication_result: Option<AuthenticationResult>,
    challenge_name: Option<String>,
    session: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthenticationResult {
    access_token: String,
    id_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: i64,
}

#[derive(Deserialize)]
struct CognitoErrorBody {
    #[serde(rename = "__type")]
    typ: Option<String>,
    message: Option<String>,
}

/// Translate a Cognito error envelope into a user-facing `AppError`.
fn map_cognito_error(bytes: &[u8]) -> AppError {
    let parsed: CognitoErrorBody = serde_json::from_slice(bytes).unwrap_or(CognitoErrorBody {
        typ: None,
        message: None,
    });
    let typ = parsed.typ.as_deref().unwrap_or("");
    // Strip the "com.amazonaws.cognito...#" prefix Cognito sometimes prepends.
    let short = typ.rsplit('#').next().unwrap_or(typ);
    match short {
        "NotAuthorizedException" => AppError::Message("Incorrect email or password.".into()),
        "UserNotFoundException" => AppError::Message("No account found for that email.".into()),
        "UserNotConfirmedException" => {
            AppError::Message("Account not verified. Check your email for a verification link.".into())
        }
        "PasswordResetRequiredException" => {
            AppError::Message("A password reset is required for this account.".into())
        }
        "TooManyRequestsException" | "LimitExceededException" => {
            AppError::Message("Too many attempts. Please wait and try again.".into())
        }
        "InvalidPasswordException" => AppError::Message(
            parsed
                .message
                .unwrap_or_else(|| "Password does not meet requirements.".into()),
        ),
        // Don't leak raw SDK error strings to the UI for unknown cases.
        _ => AppError::Message(
            parsed
                .message
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "Sign-in failed. Please try again.".into()),
        ),
    }
}
