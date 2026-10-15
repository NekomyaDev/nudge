//! OAuth 2.0 Loopback Flow (RFC 8252 installed-app authorization code flow with PKCE).
//!
//! Enforces:
//! - POSIX local loopback listener on 127.0.0.1 on a free ephemeral port.
//! - RFC 7636 PKCE S256 code challenge and verifier.
//! - Interactive TTY check: fails closed with error[E-auth-0401] in non-interactive sessions.
//! - Provider gate validation: fails closed with error[E-auth-0404] if disabled or unproven.
//! - Clear diagnostics on user denial (error[E-auth-0402]) or timeout (error[E-auth-0402]).
//! - Token persistence strictly into Nudge's OWN credential store (~/.nudge/credentials.json, mode 0600).
//! - Zero foreign token harvesting: NEVER touches ~/.jcode or ~/.openhands.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use super::credential::Credential;
#[cfg(not(target_arch = "wasm32"))]
use super::device_flow::is_tty;
#[cfg(all(not(target_arch = "wasm32"), feature = "live-provider"))]
use super::store::CredentialStore;
use crate::provider::gate::{get_provider_gate_policy, ProviderGatePolicy};

pub const DEFAULT_ANTIGRAVITY_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
// NOTE: no hardcoded client secret. This is an OAuth 2.0 PUBLIC client (RFC 8252
// installed-app flow) authenticated with PKCE (S256), which does NOT use a client
// secret. If a deployment ever needs one, it is supplied at runtime via
// NUDGE_ANTIGRAVITY_CLIENT_SECRET — never committed to source.
pub const DEFAULT_ANTIGRAVITY_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const DEFAULT_ANTIGRAVITY_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

pub const ANTIGRAVITY_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/cclog",
    "https://www.googleapis.com/auth/experimentsandconfigs",
];

#[derive(Debug, Clone)]
pub struct LoopbackOAuthConfig {
    pub provider: String,
    pub auth_url: String,
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_path: String,
}

impl LoopbackOAuthConfig {
    pub fn for_provider(provider_name: &str) -> Result<Self, String> {
        let policy = get_provider_gate_policy(provider_name);
        match policy {
            ProviderGatePolicy::LoopbackOAuth {
                auth_url,
                token_endpoint,
                client_id,
            } => {
                let auth_url = std::env::var("NUDGE_ANTIGRAVITY_AUTH_URL")
                    .unwrap_or_else(|_| auth_url.to_string());
                let token_endpoint = std::env::var("NUDGE_ANTIGRAVITY_TOKEN_ENDPOINT")
                    .unwrap_or_else(|_| token_endpoint.to_string());
                let client_id = std::env::var("NUDGE_ANTIGRAVITY_CLIENT_ID")
                    .unwrap_or_else(|_| client_id.to_string());
                let client_secret = std::env::var("NUDGE_ANTIGRAVITY_CLIENT_SECRET").ok();
                let scopes = ANTIGRAVITY_SCOPES.iter().map(|s| s.to_string()).collect();

                Ok(Self {
                    provider: provider_name.to_string(),
                    auth_url,
                    token_endpoint,
                    client_id,
                    client_secret,
                    scopes,
                    redirect_path: "/oauth-callback".to_string(),
                })
            }
            ProviderGatePolicy::Disabled { reason } => {
                Err(format!("error[E-auth-0404]: {reason}"))
            }
            _ => Err(format!(
                "error[E-auth-0404]: provider adapter '{provider_name}' does not support loopback OAuth"
            )),
        }
    }
}

/// Encode bytes into URL-safe base64 without padding (RFC 7636 Section 3).
pub fn base64url_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((data.len() * 4).div_ceil(3));
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        let idx0 = (b0 >> 2) as usize;
        let idx1 = (((b0 & 0x03) << 4) | (b1 >> 4)) as usize;
        out.push(CHARSET[idx0] as char);
        out.push(CHARSET[idx1] as char);

        if i + 1 < data.len() {
            let idx2 = (((b1 & 0x0f) << 2) | (b2 >> 6)) as usize;
            out.push(CHARSET[idx2] as char);
        }
        if i + 2 < data.len() {
            let idx3 = (b2 & 0x3f) as usize;
            out.push(CHARSET[idx3] as char);
        }

        i += 3;
    }
    out
}

/// Standard percent-encoding for URI query parameter components.
pub fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn generate_random_bytes(len: usize) -> Vec<u8> {
    #[cfg(unix)]
    {
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            let mut buf = vec![0u8; len];
            if f.read_exact(&mut buf).is_ok() {
                return buf;
            }
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seed = now.as_nanos() ^ ((std::process::id() as u128) << 64);
    let mut state = seed as u64 ^ (seed >> 64) as u64;
    let mut buf = Vec::with_capacity(len);
    for _ in 0..len {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        buf.push((state >> 33) as u8);
    }
    buf
}

#[cfg(not(target_arch = "wasm32"))]
pub struct LoopbackAuthFlow {
    pub config: LoopbackOAuthConfig,
    pub account_name: String,
    pub port: u16,
    pub listener: std::net::TcpListener,
    pub state: String,
    pub code_verifier: String,
    pub redirect_uri: String,
    pub auth_url: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl LoopbackAuthFlow {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn auth_url(&self) -> &str {
        &self.auth_url
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Block and wait for the browser redirect callback, then exchange code for token.
    pub fn wait_for_callback_and_exchange(self) -> Result<Credential, String> {
        use std::io::{Read, Write};

        let timeout_secs = std::env::var("NUDGE_AUTH_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(180);
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        let code = loop {
            if start.elapsed() >= timeout {
                return Err(
                    "error[E-auth-0402]: OAuth loopback flow expired, timed out, or was rejected by user"
                        .to_string(),
                );
            }
            if crate::task::is_current_cancelled() {
                return Err("error[E-task-0001]: TaskCancelled".to_string());
            }

            match self.listener.accept() {
                Ok((mut stream, _addr)) => {
                    let mut buf = [0u8; 4096];
                    let mut total_read = 0;
                    loop {
                        match stream.read(&mut buf[total_read..]) {
                            Ok(0) => break,
                            Ok(n) => {
                                total_read += n;
                                if buf[..total_read].windows(4).any(|w| w == b"\r\n\r\n")
                                    || total_read == buf.len()
                                {
                                    break;
                                }
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(20));
                                continue;
                            }
                            Err(e) => {
                                return Err(format!(
                                    "error[E-auth-0401]: failed to read from loopback stream: {e}"
                                ));
                            }
                        }
                    }

                    let req_str = String::from_utf8_lossy(&buf[..total_read]);
                    let first_line = req_str.lines().next().unwrap_or("");
                    let path_part = first_line.split_whitespace().nth(1).unwrap_or("");

                    if path_part.starts_with("/favicon.ico") {
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                        continue;
                    }

                    if !path_part.starts_with(&self.config.redirect_path) {
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                        continue;
                    }

                    // Parse query parameters
                    let query_str = path_part.split_once('?').map(|(_, q)| q).unwrap_or("");
                    let params: std::collections::HashMap<String, String> = query_str
                        .split('&')
                        .filter_map(|pair| {
                            let mut split = pair.splitn(2, '=');
                            let k = split.next()?;
                            let v = split.next().unwrap_or("");
                            Some((k.to_string(), v.to_string()))
                        })
                        .collect();

                    if let Some(err_val) = params.get("error") {
                        let msg = format!(
                            "<!DOCTYPE html><html><body style=\"font-family:sans-serif;text-align:center;padding-top:40px;\"><h2>Authentication Denied</h2><p>Error: {}</p><p>You can close this tab.</p></body></html>",
                            err_val
                        );
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            msg.len(),
                            msg
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                        return Err(
                            "error[E-auth-0402]: OAuth authorization denied by user".to_string()
                        );
                    }

                    if params.get("state").map(|s| s.as_str()) != Some(&self.state) {
                        let body = "OAuth state mismatch";
                        let resp = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                        return Err("error[E-auth-0401]: OAuth state mismatch".to_string());
                    }

                    let code = match params.get("code") {
                        Some(c) => c.clone(),
                        None => {
                            let body = "Missing authorization code";
                            let resp = format!(
                                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(resp.as_bytes());
                            let _ = stream.flush();
                            return Err(
                                "error[E-auth-0401]: OAuth callback missing authorization code"
                                    .to_string(),
                            );
                        }
                    };

                    let body = "<!DOCTYPE html><html><body style=\"font-family:sans-serif;text-align:center;padding-top:40px;\"><h2>Authentication Successful</h2><p>Token exchanged and saved. You can close this tab and return to Nudge.</p></body></html>";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.flush();

                    break code;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(format!("error[E-auth-0401]: loopback listener error: {e}"));
                }
            }
        };

        // Step 2: Code exchange at token endpoint
        exchange_code_at_endpoint(
            &self.config,
            &self.account_name,
            &code,
            &self.code_verifier,
            &self.redirect_uri,
        )
    }
}

pub struct LoopbackOAuthClient {
    pub config: LoopbackOAuthConfig,
}

impl LoopbackOAuthClient {
    pub fn new(config: LoopbackOAuthConfig) -> Self {
        Self { config }
    }

    pub fn for_provider(provider_name: &str) -> Result<Self, String> {
        let config = LoopbackOAuthConfig::for_provider(provider_name)?;
        Ok(Self::new(config))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_flow(&self, account_name: &str) -> Result<LoopbackAuthFlow, String> {
        // Gate check
        let policy = get_provider_gate_policy(&self.config.provider);
        if let ProviderGatePolicy::Disabled { reason } = policy {
            return Err(format!("error[E-auth-0404]: {reason}"));
        }

        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| {
            format!("error[E-auth-0401]: failed to bind local loopback listener: {e}")
        })?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("error[E-auth-0401]: failed to get local port: {e}"))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("error[E-auth-0401]: failed to set listener nonblocking: {e}"))?;

        let verifier_bytes = generate_random_bytes(64);
        let code_verifier = base64url_encode(&verifier_bytes);

        let hash = crate::trace::sha256(code_verifier.as_bytes());
        let code_challenge = base64url_encode(&hash);

        let state_bytes = generate_random_bytes(16);
        let state: String = state_bytes.iter().map(|b| format!("{:02x}", b)).collect();

        let redirect_uri = format!("http://127.0.0.1:{port}{}", self.config.redirect_path);
        let scopes_str = self.config.scopes.join(" ");

        let sep = if self.config.auth_url.contains('?') {
            "&"
        } else {
            "?"
        };
        let auth_url = format!(
            "{}{}response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
            self.config.auth_url,
            sep,
            url_encode(&self.config.client_id),
            url_encode(&redirect_uri),
            url_encode(&scopes_str),
            url_encode(&code_challenge),
            url_encode(&state)
        );

        Ok(LoopbackAuthFlow {
            config: self.config.clone(),
            account_name: account_name.to_string(),
            port,
            listener,
            state,
            code_verifier,
            redirect_uri,
            auth_url,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn authenticate(&self, _account_name: &str) -> Result<Credential, String> {
        Err("error[E-auth-0404]: loopback OAuth is not supported on wasm targets".to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn authenticate(&self, account_name: &str) -> Result<Credential, String> {
        if !is_tty() {
            return Err(format!(
                "error[E-auth-0401]: credential missing or uninitialized for provider '{}': cannot run interactive OAuth loopback in non-interactive session (no TTY)",
                self.config.provider
            ));
        }

        let flow = self.start_flow(account_name)?;

        eprintln!(
            "[nudge auth] Action requires authentication for account '{account_name}' (provider: {}).",
            self.config.provider
        );
        eprintln!("[nudge auth] To complete login, visit: {}", flow.auth_url());
        eprintln!(
            "[nudge auth] Waiting for authorization callback on http://127.0.0.1:{}/oauth-callback...",
            flow.port()
        );

        open_browser(flow.auth_url());

        flow.wait_for_callback_and_exchange()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn();
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = url;
    }
}

/// Exchange an authorization code at the token endpoint using in-process HTTP.
#[cfg(all(not(target_arch = "wasm32"), feature = "live-provider"))]
pub fn exchange_code_at_endpoint(
    config: &LoopbackOAuthConfig,
    account_name: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<Credential, String> {
    use crate::provider::http;

    let mut body_map = serde_json::Map::new();
    body_map.insert(
        "grant_type".to_string(),
        serde_json::Value::String("authorization_code".to_string()),
    );
    body_map.insert(
        "client_id".to_string(),
        serde_json::Value::String(config.client_id.clone()),
    );
    if let Some(cs) = &config.client_secret {
        body_map.insert(
            "client_secret".to_string(),
            serde_json::Value::String(cs.clone()),
        );
    }
    body_map.insert(
        "code".to_string(),
        serde_json::Value::String(code.to_string()),
    );
    body_map.insert(
        "code_verifier".to_string(),
        serde_json::Value::String(code_verifier.to_string()),
    );
    body_map.insert(
        "redirect_uri".to_string(),
        serde_json::Value::String(redirect_uri.to_string()),
    );

    let body_str = serde_json::Value::Object(body_map).to_string();
    let headers = vec![
        ("Accept".to_string(), "application/json".to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];

    let (status, resp_body) = http::post_json(&config.token_endpoint, &headers, &body_str)?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "error[E-auth-0401]: token exchange returned HTTP status {status}: {resp_body}"
        ));
    }

    let resp_json: serde_json::Value = serde_json::from_str(&resp_body)
        .map_err(|e| format!("error[E-auth-0401]: invalid JSON in token exchange response: {e}"))?;

    let access_token = resp_json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "error[E-auth-0401]: token response missing 'access_token'".to_string())?
        .to_string();

    let refresh_token = resp_json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let expires_at = resp_json
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .map(|s| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + s
        });

    let scopes: Vec<String> = if let Some(s) = resp_json.get("scope").and_then(|v| v.as_str()) {
        s.split_whitespace().map(str::to_string).collect()
    } else {
        config.scopes.clone()
    };

    let cred = Credential::oauth(access_token, refresh_token, expires_at, scopes.clone());

    // Persist into Nudge's OWN credential store (~/.nudge/credentials.json, mode 0600)
    let mut store = CredentialStore::load_default().unwrap_or_default();
    store.set_account(account_name, &config.provider, cred.clone(), scopes);
    store.set_provider(&config.provider, cred.clone());
    store.save_default()?;

    eprintln!("[nudge auth] Authorization successful. Token saved to credential store.");
    Ok(cred)
}

#[cfg(any(target_arch = "wasm32", not(feature = "live-provider")))]
pub fn exchange_code_at_endpoint(
    _config: &LoopbackOAuthConfig,
    _account_name: &str,
    _code: &str,
    _code_verifier: &str,
    _redirect_uri: &str,
) -> Result<Credential, String> {
    Err(
        "error[E-auth-0404]: live token exchange requires live-provider and non-wasm target"
            .to_string(),
    )
}

/// Refresh an OAuth token for a given provider if a refresh token is present in the store.
#[cfg(all(not(target_arch = "wasm32"), feature = "live-provider"))]
pub fn refresh_provider_token(provider_name: &str) -> Result<Credential, String> {
    use crate::provider::http;

    let mut store = CredentialStore::load_default()?;
    let cred = store
        .get_provider(provider_name)
        .ok_or_else(|| {
            format!("error[E-auth-0401]: credential missing for provider '{provider_name}'")
        })?
        .clone();

    let refresh_token = match &cred {
        Credential::OAuthToken {
            refresh_token: Some(rt),
            ..
        } => rt.clone(),
        _ => {
            return Err(format!(
                "error[E-auth-0401]: no refresh token available for provider '{provider_name}'"
            ));
        }
    };

    let config = LoopbackOAuthConfig::for_provider(provider_name)?;

    let mut body_map = serde_json::Map::new();
    body_map.insert(
        "grant_type".to_string(),
        serde_json::Value::String("refresh_token".to_string()),
    );
    body_map.insert(
        "refresh_token".to_string(),
        serde_json::Value::String(refresh_token.clone()),
    );
    body_map.insert(
        "client_id".to_string(),
        serde_json::Value::String(config.client_id.clone()),
    );
    if let Some(cs) = &config.client_secret {
        body_map.insert(
            "client_secret".to_string(),
            serde_json::Value::String(cs.clone()),
        );
    }

    let body_str = serde_json::Value::Object(body_map).to_string();
    let headers = vec![
        ("Accept".to_string(), "application/json".to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];

    let (status, resp_body) = http::post_json(&config.token_endpoint, &headers, &body_str)?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "error[E-auth-0401]: token refresh returned HTTP status {status}: {resp_body}"
        ));
    }

    let resp_json: serde_json::Value = serde_json::from_str(&resp_body)
        .map_err(|e| format!("error[E-auth-0401]: invalid JSON in token refresh response: {e}"))?;

    let new_access_token = resp_json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "error[E-auth-0401]: refresh response missing 'access_token'".to_string())?
        .to_string();

    let new_refresh_token = resp_json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or(Some(refresh_token));

    let expires_at = resp_json
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .map(|s| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + s
        });

    let scopes = match &cred {
        Credential::OAuthToken { scopes, .. } => scopes.clone(),
        _ => Vec::new(),
    };

    let updated_cred = Credential::oauth(new_access_token, new_refresh_token, expires_at, scopes);

    // Save updated credential to store
    store.set_provider(provider_name, updated_cred.clone());
    for (acc, entry) in store.accounts.iter_mut() {
        if entry.provider == provider_name {
            entry.credential = updated_cred.clone();
            let _ = acc;
        }
    }
    store.save_default()?;

    Ok(updated_cred)
}

#[cfg(any(target_arch = "wasm32", not(feature = "live-provider")))]
pub fn refresh_provider_token(_provider_name: &str) -> Result<Credential, String> {
    Err(
        "error[E-auth-0404]: live token refresh requires live-provider and non-wasm target"
            .to_string(),
    )
}

/// Interactive login helper for CLI commands (`nudgec auth login <provider>`).
pub fn login_interactive(provider_name: &str) -> Result<Credential, String> {
    match provider_name {
        "antigravity" => {
            let client = LoopbackOAuthClient::for_provider("antigravity")?;
            client.authenticate("default")
        }
        "github_copilot" => {
            let config =
                super::device_flow::DeviceFlowConfig::for_provider("github_copilot", None)?;
            let client = super::device_flow::DeviceFlowClient::new(config);
            client.authenticate("default")
        }
        other => Err(format!(
            "error[E-auth-0404]: provider '{other}' does not support interactive OAuth login"
        )),
    }
}
