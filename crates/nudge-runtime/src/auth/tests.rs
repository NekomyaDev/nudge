#[cfg(feature = "live-provider")]
use std::io::{Read, Write};
#[cfg(feature = "live-provider")]
use std::net::TcpListener;
use std::path::PathBuf;

use super::credential::Credential;
use super::device_flow::{is_tty, validate_scope, DeviceFlowClient, DeviceFlowConfig};
use super::loopback::base64url_encode;
#[cfg(feature = "live-provider")]
use super::loopback::{self, LoopbackOAuthClient, LoopbackOAuthConfig};
use super::store::CredentialStore;
#[cfg(feature = "live-provider")]
use crate::provider::antigravity::AntigravityProvider;
#[cfg(feature = "live-provider")]
use crate::provider::copilot::CopilotProvider;
#[cfg(feature = "live-provider")]
use crate::provider::{LlmRequest, Provider};

thread_local! {
    static AUTH_TEST_LOCK_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static AUTH_TEST_GUARD: std::cell::RefCell<Option<std::sync::MutexGuard<'static, ()>>> = const { std::cell::RefCell::new(None) };
}

pub(crate) struct AuthTestLockGuard;

impl AuthTestLockGuard {
    fn acquire() -> Self {
        AUTH_TEST_LOCK_DEPTH.with(|d| {
            let cur = d.get();
            if cur == 0 {
                let guard = crate::trace::testing::TEST_STATE_LOCK
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                AUTH_TEST_GUARD.with(|g| {
                    *g.borrow_mut() = Some(guard);
                });
            }
            d.set(cur + 1);
        });
        Self
    }
}

impl Drop for AuthTestLockGuard {
    fn drop(&mut self) {
        AUTH_TEST_LOCK_DEPTH.with(|d| {
            let cur = d.get();
            if cur <= 1 {
                d.set(0);
                AUTH_TEST_GUARD.with(|g| g.borrow_mut().take());
            } else {
                d.set(cur - 1);
            }
        });
    }
}

fn get_test_lock() -> AuthTestLockGuard {
    AuthTestLockGuard::acquire()
}

fn test_uid() -> u64 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

struct EnvGuard {
    key: &'static str,
    original: Option<String>,
    _lock: AuthTestLockGuard,
}

impl EnvGuard {
    fn set(key: &'static str, val: &str) -> Self {
        let lock = AuthTestLockGuard::acquire();
        let original = std::env::var(key).ok();
        std::env::set_var(key, val);
        Self {
            key,
            original,
            _lock: lock,
        }
    }

    fn unset(key: &'static str) -> Self {
        let lock = AuthTestLockGuard::acquire();
        let original = std::env::var(key).ok();
        std::env::remove_var(key);
        Self {
            key,
            original,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(ref val) = self.original {
            std::env::set_var(self.key, val);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

#[test]
fn test_auth_store_roundtrip_and_file_mode() {
    let _lock = get_test_lock();
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_store_{}_{}",
        std::process::id(),
        test_uid()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    let store_path = temp_dir.join("credentials.json");

    let mut store = CredentialStore::new();
    let cred = Credential::oauth(
        "gho_test_12345",
        Some("ghr_refresh_67890".to_string()),
        Some(1900000000),
        vec!["model:inference".to_string(), "read:user".to_string()],
    );

    store.set_account(
        "dev_tier",
        "github_copilot",
        cred.clone(),
        vec!["model:inference".to_string()],
    );
    store.set_provider("github_copilot", cred.clone());

    store.save_to(&store_path).expect("save to temp path");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&store_path)
            .expect("metadata")
            .permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "Credential store file must have mode 0600"
        );
        let parent_perms = std::fs::metadata(&temp_dir)
            .expect("parent metadata")
            .permissions();
        assert_eq!(
            parent_perms.mode() & 0o777,
            0o700,
            "Credential store dir must have mode 0700"
        );
    }

    let loaded = CredentialStore::load_from(&store_path).expect("load from path");
    assert_eq!(loaded, store);
    assert_eq!(
        loaded.get_account("dev_tier").unwrap().provider,
        "github_copilot"
    );
    assert_eq!(
        loaded
            .get_account("dev_tier")
            .unwrap()
            .credential
            .access_token(),
        "gho_test_12345"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_auth_store_corrupted_file_trips_e_auth_0405() {
    let _lock = get_test_lock();
    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_corrupt_{}_{}",
        std::process::id(),
        test_uid()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    let store_path = temp_dir.join("credentials.json");

    std::fs::write(&store_path, "{ broken invalid json").unwrap();

    let res = CredentialStore::load_from(&store_path);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("error[E-auth-0405]"),
        "Corrupted credential store must emit error[E-auth-0405], got: {err}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_scope_validation_and_deficit_e_auth_0403() {
    let cred = Credential::oauth("tok_123", None, None, vec!["read:user".to_string()]);

    let res = validate_scope(&cred, "model:inference", "dev_tier");
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("error[E-auth-0403]"),
        "Scope deficit must yield error[E-auth-0403], got: {err}"
    );
    assert!(err.contains("dev_tier"));
    assert!(err.contains("model:inference"));

    let valid_cred = Credential::oauth(
        "tok_123",
        None,
        None,
        vec!["model:inference".to_string(), "read:user".to_string()],
    );
    assert!(validate_scope(&valid_cred, "model:inference", "dev_tier").is_ok());
}

#[test]
fn test_provider_without_proven_device_flow_disabled_e_auth_0404() {
    // antigravity: disabled
    let res = DeviceFlowConfig::for_provider("antigravity", None);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("error[E-auth-0404]"));

    // openai: API key only, OAuth device flow disabled
    let res_openai = DeviceFlowConfig::for_provider("openai", None);
    assert!(res_openai.is_err());
    assert!(res_openai.unwrap_err().contains("error[E-auth-0404]"));

    // claude: API key only, OAuth device flow disabled
    let res_claude = DeviceFlowConfig::for_provider("claude", None);
    assert!(res_claude.is_err());
    assert!(res_claude.unwrap_err().contains("error[E-auth-0404]"));

    // github_copilot: proven official RFC 8628 path enabled
    let res_copilot = DeviceFlowConfig::for_provider("github_copilot", None);
    assert!(res_copilot.is_ok());
    let cfg = res_copilot.unwrap();
    assert_eq!(cfg.issuer, "https://github.com/login/device/code");
    assert_eq!(cfg.client_id, "Iv1.b507a08c87ecfe81");
}

#[test]
fn test_non_tty_fails_closed_e_auth_0401_no_mock_no_stall() {
    let _lock = get_test_lock();
    let _guard_non_interactive = EnvGuard::set("NUDGE_NON_INTERACTIVE", "1");
    let _guard_force_tty = EnvGuard::unset("NUDGE_FORCE_TTY");

    assert!(
        !is_tty(),
        "is_tty must be false when NUDGE_NON_INTERACTIVE=1"
    );

    let cfg = DeviceFlowConfig::for_provider("github_copilot", None).unwrap();
    let client = DeviceFlowClient::new(cfg);

    let res = client.authenticate("dev_tier");
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("error[E-auth-0401]"),
        "Missing credential in non-interactive session must fail closed with error[E-auth-0401], got: {err}"
    );
    assert!(err.contains("no TTY"));
}

#[test]
#[cfg(feature = "live-provider")]
fn test_in_process_rfc8628_device_flow_with_mock_server() {
    let _lock = get_test_lock();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock port");
    let port = listener.local_addr().unwrap().port();

    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_test_device_flow_{}_{}",
        std::process::id(),
        test_uid()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    let store_file = temp_dir.join("credentials.json");

    let _guard_cred_path = EnvGuard::set("NUDGE_CREDENTIALS_PATH", store_file.to_str().unwrap());
    let _guard_tty = EnvGuard::set("NUDGE_FORCE_TTY", "1");
    let _guard_non_interactive = EnvGuard::unset("NUDGE_NON_INTERACTIVE");
    let _guard_poll = EnvGuard::set("NUDGE_AUTH_POLL_INTERVAL_MS", "15");

    let handle = std::thread::spawn(move || {
        let mut poll_count = 0;

        for _ in 0..3 {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut req_bytes = Vec::new();
            let mut buf = [0u8; 512];
            let mut content_length = 0;
            let mut header_end = None;

            loop {
                let n = stream.read(&mut buf).expect("read");
                if n == 0 {
                    break;
                }
                req_bytes.extend_from_slice(&buf[..n]);

                if header_end.is_none() {
                    if let Some(pos) = req_bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers_str = String::from_utf8_lossy(&req_bytes[..pos]);
                        for line in headers_str.lines() {
                            if line.to_ascii_lowercase().starts_with("content-length:") {
                                if let Some(len_str) = line.split(':').nth(1) {
                                    content_length = len_str.trim().parse::<usize>().unwrap_or(0);
                                }
                            }
                        }
                        header_end = Some(pos + 4);
                    }
                }

                if let Some(start) = header_end {
                    if req_bytes.len() >= start + content_length {
                        break;
                    }
                }
            }

            let req_str = String::from_utf8_lossy(&req_bytes);

            if req_str.starts_with("POST /device/code") {
                let body = serde_json::json!({
                    "device_code": "mock_device_code_abc",
                    "user_code": "WDJB-MJHT",
                    "verification_uri": "https://example.com/login/device",
                    "expires_in": 30,
                    "interval": 1
                })
                .to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).unwrap();
            } else if req_str.starts_with("POST /login/oauth/access_token") {
                poll_count += 1;
                if poll_count == 1 {
                    let body = serde_json::json!({
                        "error": "authorization_pending"
                    })
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(resp.as_bytes()).unwrap();
                } else {
                    let body = serde_json::json!({
                        "access_token": "gho_mock_token_777888",
                        "token_type": "bearer",
                        "scope": "model:inference read:user",
                        "expires_in": 3600
                    })
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(resp.as_bytes()).unwrap();
                }
            }
        }
    });

    let config = DeviceFlowConfig {
        provider: "github_copilot".to_string(),
        issuer: format!("http://127.0.0.1:{port}/device/code"),
        token_endpoint: format!("http://127.0.0.1:{port}/login/oauth/access_token"),
        client_id: "Iv1.b507a08c87ecfe81".to_string(),
        scope: "model:inference read:user".to_string(),
    };

    let client = DeviceFlowClient::new(config);
    let cred = client
        .authenticate("dev_tier")
        .expect("device flow succeed");

    assert_eq!(cred.access_token(), "gho_mock_token_777888");
    assert!(cred.has_scope("model:inference"));
    assert!(cred.has_scope("read:user"));
    assert!(!cred.is_expired());

    handle.join().expect("server thread finish");

    // Verify token was saved into Nudge's credential store
    let store = CredentialStore::load_from(&store_file).expect("load saved store");
    let account_entry = store.get_account("dev_tier").expect("dev_tier in store");
    assert_eq!(account_entry.provider, "github_copilot");
    assert_eq!(
        account_entry.credential.access_token(),
        "gho_mock_token_777888"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(feature = "live-provider")]
fn test_copilot_provider_attaches_bearer_token() {
    let _lock = get_test_lock();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock port");
    let port = listener.local_addr().unwrap().port();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut req_bytes = Vec::new();
        let mut buf = [0u8; 512];
        let mut content_length = 0;
        let mut header_end = None;

        loop {
            let n = stream.read(&mut buf).expect("read");
            if n == 0 {
                break;
            }
            req_bytes.extend_from_slice(&buf[..n]);

            if header_end.is_none() {
                if let Some(pos) = req_bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers_str = String::from_utf8_lossy(&req_bytes[..pos]);
                    for line in headers_str.lines() {
                        if line.to_ascii_lowercase().starts_with("content-length:") {
                            if let Some(len_str) = line.split(':').nth(1) {
                                content_length = len_str.trim().parse::<usize>().unwrap_or(0);
                            }
                        }
                    }
                    header_end = Some(pos + 4);
                }
            }

            if let Some(start) = header_end {
                if req_bytes.len() >= start + content_length {
                    break;
                }
            }
        }

        let req_str = String::from_utf8_lossy(&req_bytes);

        // Verify Bearer header is attached
        assert!(
            req_str
                .to_ascii_lowercase()
                .contains("authorization: bearer gho_test_token_999"),
            "Request must carry Bearer header with token"
        );

        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "4"
                }
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 1
            }
        })
        .to_string();

        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes()).unwrap();
    });

    let mock_url = format!("http://127.0.0.1:{port}/chat/completions");
    let provider = CopilotProvider::with_token("gho_test_token_999", Some(mock_url));

    let req = LlmRequest {
        prompt: "2+2=".to_string(),
        model: "copilot-chat".to_string(),
        schema: None,
        temperature: 0.1,
        budget_usd: None,
    };

    let resp = provider.call(&req).expect("call copilot provider");
    assert_eq!(resp.text, "4");
    assert_eq!(resp.tokens_in, 5);
    assert_eq!(resp.tokens_out, 1);

    handle.join().expect("server finish");
}

#[test]
fn test_never_reads_foreign_credentials_directories() {
    let _lock = get_test_lock();

    // Verify source code itself has zero harvesting logic for foreign paths
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    for entry in std::fs::read_dir(src_dir).expect("read src dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "rs").unwrap_or(false) {
            let content = std::fs::read_to_string(&path).expect("read rs file");
            assert!(
                !content.contains("read_oauth_token"),
                "Source file '{}' contains forbidden foreign token harvester read_oauth_token",
                path.display()
            );
            assert!(
                !content.contains("read_openhands_profile"),
                "Source file '{}' contains forbidden foreign token harvester read_openhands_profile",
                path.display()
            );
        }
    }
}

#[test]
fn test_pkce_base64url_encode() {
    assert_eq!(base64url_encode(b""), "");
    assert_eq!(base64url_encode(b"f"), "Zg");
    assert_eq!(base64url_encode(b"fo"), "Zm8");
    assert_eq!(base64url_encode(b"foo"), "Zm9v");
    assert_eq!(base64url_encode(b"foob"), "Zm9vYg");
    assert_eq!(base64url_encode(b"fooba"), "Zm9vYmE");
    assert_eq!(base64url_encode(b"foobar"), "Zm9vYmFy");

    let h = crate::trace::sha256(b"test_verifier");
    let enc = base64url_encode(&h);
    assert_eq!(enc, "0Ku4rR8EgR1w3HyHLBCxVLtPsAAks5HOlpmTEt0XhVA");
}

#[cfg(feature = "live-provider")]
fn read_full_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut temp = [0u8; 1024];
    let mut content_length = None;
    let mut header_end = None;

    loop {
        let n = stream.read(&mut temp).unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&temp[..n]);

        if header_end.is_none() {
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = Some(pos + 4);
                let headers_str = String::from_utf8_lossy(&buf[..pos]);
                for line in headers_str.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("content-length") {
                            content_length = v.trim().parse::<usize>().ok();
                        }
                    }
                }
            }
        }

        if let Some(end) = header_end {
            let body_len = buf.len() - end;
            if let Some(cl) = content_length {
                if body_len >= cl {
                    break;
                }
            } else {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buf).to_string()
}

#[test]
#[cfg(feature = "live-provider")]
fn test_loopback_oauth_flow_with_mock_token_server() {
    let _lock = get_test_lock();
    let _guard_opt_in = EnvGuard::set("NUDGE_ENABLE_ANTIGRAVITY", "1");
    let _guard_port = EnvGuard::set("NUDGE_ANTIGRAVITY_REDIRECT_PORT", "0");

    let token_listener = TcpListener::bind("127.0.0.1:0").expect("bind mock token port");
    let token_port = token_listener.local_addr().unwrap().port();

    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_loopback_test_{}_{}",
        std::process::id(),
        test_uid()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    let store_path = temp_dir.join("credentials.json");
    let _guard_store = EnvGuard::set("NUDGE_CREDENTIALS_PATH", store_path.to_str().unwrap());

    // Spawn mock token server thread
    let token_handle = std::thread::spawn(move || {
        let (mut stream, _) = token_listener.accept().expect("accept token exchange");
        let req_str = read_full_http_request(&mut stream);

        assert!(req_str.starts_with("POST /mock/token"));
        assert!(req_str.contains("authorization_code"));
        assert!(req_str.contains("mock_code_12345"));

        let resp_body = serde_json::json!({
            "access_token": "ya29.mock_antigravity_token_loopback",
            "refresh_token": "1//mock_refresh_token_abcde",
            "expires_in": 3600,
            "scope": "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email"
        })
        .to_string();

        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        stream.write_all(resp.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let config = LoopbackOAuthConfig {
        provider: "antigravity".to_string(),
        auth_url: "http://127.0.0.1/auth".to_string(),
        token_endpoint: format!("http://127.0.0.1:{token_port}/mock/token"),
        client_id: "test-client-id".to_string(),
        client_secret: Some("test-secret".to_string()),
        scopes: vec!["https://www.googleapis.com/auth/cloud-platform".to_string()],
        redirect_path: "/oauth-callback".to_string(),
    };

    let client = LoopbackOAuthClient::new(config);
    let flow = client.start_flow("default").expect("start flow");
    let port = flow.port();
    let state = flow.state().to_string();

    // In a background thread or connection, simulate the browser redirecting back with the code
    let client_handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut browser_stream =
            std::net::TcpStream::connect(format!("127.0.0.1:{port}")).expect("connect to loopback");
        let get_req = format!(
            "GET /oauth-callback?code=mock_code_12345&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        );
        browser_stream.write_all(get_req.as_bytes()).unwrap();
        browser_stream.flush().unwrap();

        let mut resp_buf = [0u8; 1024];
        let n = browser_stream.read(&mut resp_buf).unwrap();
        let resp_str = String::from_utf8_lossy(&resp_buf[..n]);
        assert!(resp_str.contains("200 OK"));
        assert!(resp_str.contains("Authentication Successful"));
    });

    let cred = flow
        .wait_for_callback_and_exchange()
        .expect("exchange token");
    assert_eq!(cred.access_token(), "ya29.mock_antigravity_token_loopback");

    token_handle.join().expect("token server join");
    client_handle.join().expect("client join");

    // Verify persisted into store with 0600 mode
    let store = CredentialStore::load_from(&store_path).expect("load saved store");
    let stored_cred = store.get_provider("antigravity").expect("has antigravity");
    assert_eq!(
        stored_cred.access_token(),
        "ya29.mock_antigravity_token_loopback"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&store_path).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "credentials store must have 0600 permissions"
        );
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(feature = "live-provider")]
fn test_loopback_oauth_denial_emits_e_auth_0402() {
    let _lock = get_test_lock();
    let _guard_opt_in = EnvGuard::set("NUDGE_ENABLE_ANTIGRAVITY", "1");
    let _guard_port = EnvGuard::set("NUDGE_ANTIGRAVITY_REDIRECT_PORT", "0");

    let config = LoopbackOAuthConfig {
        provider: "antigravity".to_string(),
        auth_url: "http://127.0.0.1/auth".to_string(),
        token_endpoint: "http://127.0.0.1/mock/token".to_string(),
        client_id: "test-client-id".to_string(),
        client_secret: None,
        scopes: vec!["cloud-platform".to_string()],
        redirect_path: "/oauth-callback".to_string(),
    };

    let client = LoopbackOAuthClient::new(config);
    let flow = client.start_flow("default").expect("start flow");
    let port = flow.port();

    let client_handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut browser_stream =
            std::net::TcpStream::connect(format!("127.0.0.1:{port}")).expect("connect to loopback");
        let get_req = format!(
            "GET /oauth-callback?error=access_denied HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        );
        browser_stream.write_all(get_req.as_bytes()).unwrap();
        browser_stream.flush().unwrap();
    });

    let res = flow.wait_for_callback_and_exchange();
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(
        err.contains("error[E-auth-0402]"),
        "denial must emit E-auth-0402, got: {err}"
    );

    client_handle.join().expect("client join");
}

#[test]
#[cfg(feature = "live-provider")]
fn test_loopback_oauth_state_mismatch_emits_e_auth_0401() {
    let _lock = get_test_lock();
    let _guard_opt_in = EnvGuard::set("NUDGE_ENABLE_ANTIGRAVITY", "1");
    let _guard_port = EnvGuard::set("NUDGE_ANTIGRAVITY_REDIRECT_PORT", "0");

    let config = LoopbackOAuthConfig {
        provider: "antigravity".to_string(),
        auth_url: "http://127.0.0.1/auth".to_string(),
        token_endpoint: "http://127.0.0.1/mock/token".to_string(),
        client_id: "test-client-id".to_string(),
        client_secret: None,
        scopes: vec!["cloud-platform".to_string()],
        redirect_path: "/oauth-callback".to_string(),
    };

    let client = LoopbackOAuthClient::new(config);
    let flow = client.start_flow("default").expect("start flow");
    let port = flow.port();

    let client_handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut browser_stream =
            std::net::TcpStream::connect(format!("127.0.0.1:{port}")).expect("connect to loopback");
        let get_req = format!(
            "GET /oauth-callback?code=mock_code&state=bogus_state HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        );
        browser_stream.write_all(get_req.as_bytes()).unwrap();
        browser_stream.flush().unwrap();
    });

    let res = flow.wait_for_callback_and_exchange();
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(
        err.contains("error[E-auth-0401]"),
        "state mismatch must emit E-auth-0401, got: {err}"
    );

    client_handle.join().expect("client join");
}

#[test]
#[cfg(feature = "live-provider")]
fn test_loopback_oauth_refresh_flow() {
    let _lock = get_test_lock();
    let _guard_opt_in = EnvGuard::set("NUDGE_ENABLE_ANTIGRAVITY", "1");

    let token_listener = TcpListener::bind("127.0.0.1:0").expect("bind refresh token port");
    let token_port = token_listener.local_addr().unwrap().port();

    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_refresh_test_{}_{}",
        std::process::id(),
        test_uid()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    let store_path = temp_dir.join("credentials.json");
    let _guard_store = EnvGuard::set("NUDGE_CREDENTIALS_PATH", store_path.to_str().unwrap());
    let _guard_endpoint = EnvGuard::set(
        "NUDGE_ANTIGRAVITY_TOKEN_ENDPOINT",
        &format!("http://127.0.0.1:{token_port}/token"),
    );

    // Seed expired token with refresh_token in store
    let mut store = CredentialStore::new();
    let expired_cred = Credential::oauth(
        "ya29.stale_expired_token",
        Some("1//my_real_refresh_token_xyz".to_string()),
        Some(100), // long in the past
        vec!["cloud-platform".to_string()],
    );
    store.set_provider("antigravity", expired_cred.clone());
    store.set_account(
        "default",
        "antigravity",
        expired_cred,
        vec!["cloud-platform".to_string()],
    );
    store.save_to(&store_path).unwrap();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = token_listener.accept().unwrap();
        let req_str = read_full_http_request(&mut stream);

        assert!(req_str.contains("refresh_token"));
        assert!(req_str.contains("1//my_real_refresh_token_xyz"));

        let resp_body = serde_json::json!({
            "access_token": "ya29.fresh_refreshed_token_success",
            "expires_in": 3600
        })
        .to_string();

        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        stream.write_all(resp.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let refreshed = loopback::refresh_provider_token("antigravity").expect("refresh success");
    assert_eq!(
        refreshed.access_token(),
        "ya29.fresh_refreshed_token_success"
    );

    server_handle.join().unwrap();

    // Verify store was updated with refreshed token
    let updated_store = CredentialStore::load_from(&store_path).unwrap();
    let cred = updated_store.get_provider("antigravity").unwrap();
    assert_eq!(cred.access_token(), "ya29.fresh_refreshed_token_success");
    assert!(!cred.is_expired());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(feature = "live-provider")]
fn test_antigravity_provider_reads_only_nudge_store_not_jcode() {
    let _lock = get_test_lock();

    let temp_dir = std::env::temp_dir().join(format!(
        "nudge_antigravity_store_test_{}_{}",
        std::process::id(),
        test_uid()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    let jcode_dir = temp_dir.join(".jcode");
    std::fs::create_dir_all(&jcode_dir).unwrap();

    // Put foreign token in ~/.jcode
    std::fs::write(
        jcode_dir.join("antigravity_oauth.json"),
        r#"{"access_token": "parasitic_jcode_token_must_be_ignored"}"#,
    )
    .unwrap();

    let nudge_store_path = temp_dir.join("credentials.json");
    let mut store = CredentialStore::new();
    let nudge_cred = Credential::oauth(
        "ya29.nudge_own_legitimate_token",
        Some("refresh_tok".to_string()),
        Some(2000000000), // future
        vec!["cloud-platform".to_string()],
    );
    store.set_provider("antigravity", nudge_cred.clone());
    store.set_account(
        "default",
        "antigravity",
        nudge_cred,
        vec!["cloud-platform".to_string()],
    );
    store.save_to(&nudge_store_path).unwrap();

    let temp_home = temp_dir.to_str().unwrap();
    let _guard_home = EnvGuard::set("HOME", temp_home);
    let _guard_store = EnvGuard::set("NUDGE_CREDENTIALS_PATH", nudge_store_path.to_str().unwrap());
    let _guard_api_key = EnvGuard::unset("NUDGE_API_KEY");
    let _guard_antigravity_key = EnvGuard::unset("ANTIGRAVITY_API_KEY");
    let _guard_auth_antigravity = EnvGuard::unset("NUDGE_AUTH_ANTIGRAVITY");
    let _guard_enable_antigravity = EnvGuard::unset("NUDGE_ENABLE_ANTIGRAVITY");

    let provider = AntigravityProvider::new();
    assert!(
        provider
            .call(&LlmRequest {
                prompt: "ping".to_string(),
                model: "default".to_string(),
                schema: None,
                temperature: 0.0,
                budget_usd: None,
            })
            .is_err(),
        "Fails on network connecting to 8642, NOT on missing auth!"
    );

    // Verify token used is Nudge's own, not jcode's
    let mock_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mock_port = mock_listener.local_addr().unwrap().port();
    let mock_handle = std::thread::spawn(move || {
        let (mut stream, _) = mock_listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        let req_str = String::from_utf8_lossy(&buf[..n]);

        assert!(
            req_str.contains("authorization: Bearer ya29.nudge_own_legitimate_token")
                || req_str.contains("Authorization: Bearer ya29.nudge_own_legitimate_token"),
            "Request must carry Nudge's token, got: {req_str}"
        );
        assert!(
            !req_str.contains("parasitic_jcode_token"),
            "Request must NOT carry foreign token"
        );

        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "pong"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })
        .to_string();
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        stream.write_all(resp.as_bytes()).unwrap();
    });

    let mock_url = format!("http://127.0.0.1:{mock_port}");
    let custom_provider =
        AntigravityProvider::with_token("ya29.nudge_own_legitimate_token", Some(mock_url));
    let resp = custom_provider
        .call(&LlmRequest {
            prompt: "ping".to_string(),
            model: "default".to_string(),
            schema: None,
            temperature: 0.0,
            budget_usd: None,
        })
        .unwrap();
    assert_eq!(resp.text, "pong");

    mock_handle.join().unwrap();
    let _ = std::fs::remove_dir_all(&temp_dir);
}
