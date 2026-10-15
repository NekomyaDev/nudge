//! Credential representations for Nudge authentication subsystem (Pack 5 Provider Auth).
//!
//! Universal hard security boundaries:
//! - Credentials strictly stored in Nudge's own credential store (~/.nudge/credentials.json).
//! - Never harvests foreign tokens from ~/.jcode, ~/.openhands, or other third-party apps.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Credential {
    #[serde(rename = "api_key")]
    ApiKey { api_key: String },
    #[serde(rename = "oauth_token")]
    OAuthToken {
        access_token: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refresh_token: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at: Option<u64>,
        #[serde(default)]
        scopes: Vec<String>,
    },
}

impl Credential {
    pub fn api_key(key: impl Into<String>) -> Self {
        Self::ApiKey {
            api_key: key.into(),
        }
    }

    pub fn oauth(
        access_token: impl Into<String>,
        refresh_token: Option<String>,
        expires_at: Option<u64>,
        scopes: Vec<String>,
    ) -> Self {
        Self::OAuthToken {
            access_token: access_token.into(),
            refresh_token,
            expires_at,
            scopes,
        }
    }

    pub fn access_token(&self) -> &str {
        match self {
            Credential::ApiKey { api_key } => api_key,
            Credential::OAuthToken { access_token, .. } => access_token,
        }
    }

    pub fn has_scope(&self, required_scope: &str) -> bool {
        match self {
            Credential::ApiKey { .. } => true, // ApiKey has ambient scope
            Credential::OAuthToken { scopes, .. } => scopes.iter().any(|s| s == required_scope),
        }
    }

    pub fn is_expired(&self) -> bool {
        match self {
            Credential::ApiKey { .. } => false,
            Credential::OAuthToken { expires_at, .. } => {
                if let Some(exp) = expires_at {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    *exp <= now
                } else {
                    false
                }
            }
        }
    }
}
