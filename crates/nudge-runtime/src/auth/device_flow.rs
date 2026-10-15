//! RFC 8628 (OAuth 2.0 Device Authorization Grant) interactive client.
//!
//! Enforces:
//! - Interactive TTY check: fails closed with error[E-auth-0401] in non-interactive / CI sessions.
//! - Per-provider authorization gate: unproven providers fail closed with error[E-auth-0404].
//! - Exponential backoff / interval polling against official token endpoint.
//! - Token persistence strictly into Nudge's own credential store (~/.nudge/credentials.json).
//! - Scope verification gating (error[E-auth-0403]).

use std::time::Duration;

use super::credential::Credential;
use super::store::CredentialStore;
use crate::provider::gate::{get_provider_gate_policy, ProviderGatePolicy};
use crate::provider::http;

/// Check whether the current session is an interactive terminal with a user.
pub fn is_tty() -> bool {
    if let Ok(v) = std::env::var("NUDGE_FORCE_TTY") {
        return v == "1" || v.eq_ignore_ascii_case("true");
    }
    if let Ok(v) = std::env::var("NUDGE_NON_INTERACTIVE") {
        if v == "1" || v.eq_ignore_ascii_case("true") {
            return false;
        }
    }
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

#[derive(Debug, Clone)]
pub struct DeviceFlowConfig {
    pub provider: String,
    pub issuer: String,
    pub token_endpoint: String,
    pub client_id: String,
    pub scope: String,
}

impl DeviceFlowConfig {
    /// Create configuration for a known provider according to the provider gate.
    pub fn for_provider(provider_name: &str, scope: Option<&str>) -> Result<Self, String> {
        let policy = get_provider_gate_policy(provider_name);
        match policy {
            ProviderGatePolicy::DeviceFlow {
                issuer,
                token_endpoint,
                client_id,
            } => Ok(Self {
                provider: provider_name.to_string(),
                issuer: issuer.to_string(),
                token_endpoint: token_endpoint.to_string(),
                client_id: client_id.to_string(),
                scope: scope.unwrap_or("model:inference").to_string(),
            }),
            ProviderGatePolicy::Disabled { reason } => {
                Err(format!("error[E-auth-0404]: {reason}"))
            }
            _ => Err(format!(
                "error[E-auth-0404]: provider adapter '{provider_name}' does not support OAuth device flow"
            )),
        }
    }
}

pub struct DeviceFlowClient {
    pub config: DeviceFlowConfig,
}

impl DeviceFlowClient {
    pub fn new(config: DeviceFlowConfig) -> Self {
        Self { config }
    }

    /// Execute the RFC 8628 device authorization grant.
    ///
    /// In non-interactive mode (no TTY), fails closed immediately with error[E-auth-0401].
    pub fn authenticate(&self, account_name: &str) -> Result<Credential, String> {
        // Gate 1: Fail closed if non-interactive
        if !is_tty() {
            return Err(format!(
                "error[E-auth-0401]: credential missing or uninitialized for provider '{}': cannot run interactive device-flow in non-interactive session (no TTY)",
                self.config.provider
            ));
        }

        // Gate 2: Provider authorization gate check
        let policy = get_provider_gate_policy(&self.config.provider);
        if let ProviderGatePolicy::Disabled { reason } = policy {
            return Err(format!("error[E-auth-0404]: {reason}"));
        }

        // Step 1: Request device code from issuer
        let req_body = serde_json::json!({
            "client_id": self.config.client_id,
            "scope": self.config.scope,
        })
        .to_string();

        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];

        let (status, resp_body) = http::post_json(&self.config.issuer, &headers, &req_body)?;
        if !(200..300).contains(&status) {
            return Err(format!(
                "error[E-auth-0401]: device authorization request to '{}' failed with status {status}: {resp_body}",
                self.config.issuer
            ));
        }

        let resp_json: serde_json::Value = serde_json::from_str(&resp_body).map_err(|e| {
            format!("error[E-auth-0401]: invalid JSON in device authorization response: {e}")
        })?;

        let device_code = resp_json
            .get("device_code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                "error[E-auth-0401]: device authorization response missing 'device_code'"
                    .to_string()
            })?
            .to_string();

        let user_code = resp_json
            .get("user_code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                "error[E-auth-0401]: device authorization response missing 'user_code'".to_string()
            })?
            .to_string();

        let verification_uri = resp_json
            .get("verification_uri")
            .or_else(|| resp_json.get("verification_url"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                "error[E-auth-0401]: device authorization response missing 'verification_uri'"
                    .to_string()
            })?
            .to_string();

        let expires_in = resp_json
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(900);

        let mut interval = resp_json
            .get("interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .max(1);

        // Allow overriding poll interval for fast automated test suites
        if let Ok(ms_str) = std::env::var("NUDGE_AUTH_POLL_INTERVAL_MS") {
            if let Ok(ms) = ms_str.parse::<u64>() {
                interval = ms.max(1);
            }
        }

        // Present instructions to terminal
        eprintln!("[nudge auth] Action requires authentication for account '{account_name}'.");
        eprintln!("[nudge auth] To complete login, visit: {verification_uri}");
        eprintln!("[nudge auth] Enter code: {user_code}");
        eprintln!("[nudge auth] Waiting for authorization...");

        let start_time = std::time::Instant::now();
        let timeout = Duration::from_secs(expires_in);

        let poll_body = serde_json::json!({
            "client_id": self.config.client_id,
            "device_code": device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        })
        .to_string();

        loop {
            if start_time.elapsed() >= timeout {
                return Err(
                    "error[E-auth-0402]: device code flow expired, timed out, or was rejected by user"
                        .to_string(),
                );
            }

            if crate::task::is_current_cancelled() {
                return Err("error[E-task-0001]: TaskCancelled".to_string());
            }

            // Sleep for interval
            let sleep_dur = if std::env::var("NUDGE_AUTH_POLL_INTERVAL_MS").is_ok() {
                Duration::from_millis(interval)
            } else {
                Duration::from_secs(interval)
            };
            std::thread::sleep(sleep_dur);

            let (poll_status, poll_resp_text) =
                http::post_json(&self.config.token_endpoint, &headers, &poll_body)?;

            if let Ok(poll_json) = serde_json::from_str::<serde_json::Value>(&poll_resp_text) {
                if let Some(err_code) = poll_json.get("error").and_then(|v| v.as_str()) {
                    match err_code {
                        "authorization_pending" => {
                            continue;
                        }
                        "slow_down" => {
                            interval += 5;
                            continue;
                        }
                        "expired_token" => {
                            return Err(
                                "error[E-auth-0402]: device code flow expired, timed out, or was rejected by user"
                                    .to_string(),
                            );
                        }
                        "access_denied" => {
                            return Err(
                                "error[E-auth-0402]: device code flow expired, timed out, or was rejected by user"
                                    .to_string(),
                            );
                        }
                        other => {
                            return Err(format!(
                                "error[E-auth-0401]: device authorization polling failed: {other}"
                            ));
                        }
                    }
                }

                if let Some(token) = poll_json.get("access_token").and_then(|v| v.as_str()) {
                    let refresh_token = poll_json
                        .get("refresh_token")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let exp = poll_json
                        .get("expires_in")
                        .and_then(|v| v.as_u64())
                        .map(|s| {
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs()
                                + s
                        });
                    let scopes: Vec<String> =
                        if let Some(s) = poll_json.get("scope").and_then(|v| v.as_str()) {
                            s.split_whitespace().map(str::to_string).collect()
                        } else {
                            self.config
                                .scope
                                .split_whitespace()
                                .map(str::to_string)
                                .collect()
                        };

                    let cred = Credential::oauth(token, refresh_token, exp, scopes.clone());

                    // Persist to Nudge's credential store (~/.nudge/credentials.json)
                    let mut store = CredentialStore::load_default().unwrap_or_default();
                    store.set_account(account_name, &self.config.provider, cred.clone(), scopes);
                    store.set_provider(&self.config.provider, cred.clone());
                    store.save_default()?;

                    eprintln!(
                        "[nudge auth] Authorization successful. Token saved to credential store."
                    );
                    return Ok(cred);
                }
            }

            if poll_status >= 400 {
                return Err(format!(
                    "error[E-auth-0401]: token polling returned HTTP status {poll_status}: {poll_resp_text}"
                ));
            }
        }
    }
}

/// Verify that the credential contains the required scope for the operation.
/// Fails closed with error[E-auth-0403] on scope deficit.
pub fn validate_scope(
    cred: &Credential,
    required_scope: &str,
    account_name: &str,
) -> Result<(), String> {
    if !cred.has_scope(required_scope) {
        return Err(format!(
            "error[E-auth-0403]: account '{account_name}' lacks required scope '{required_scope}'"
        ));
    }
    Ok(())
}
