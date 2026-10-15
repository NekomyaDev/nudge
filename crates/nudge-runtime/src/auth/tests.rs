#[cfg(feature = "live-provider")]
use std::io::{Read, Write};
#[cfg(feature = "live-provider")]
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex;

use super::credential::Credential;
use super::device_flow::{is_tty, validate_scope, DeviceFlowClient, DeviceFlowConfig};
use super::store::CredentialStore;
#[cfg(feature = "live-provider")]
use crate::provider::copilot::CopilotProvider;
#[cfg(feature = "live-provider")]
use crate::provider::{LlmRequest, Provider};

static TEST_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, val: &str) -> Self {
        let original = std::env::var(key).ok();
        std::env::set_var(key, val);
        Self { key, original }
    }

    fn unset(key: &'static str) -> Self {
        let original = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, original }
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
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let temp_dir = std::env::temp_dir().join(format!("nudge_test_store_{}", std::process::id()));
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
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let temp_dir = std::env::temp_dir().join(format!("nudge_test_corrupt_{}", std::process::id()));
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
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock port");
    let port = listener.local_addr().unwrap().port();

    let temp_dir =
        std::env::temp_dir().join(format!("nudge_test_device_flow_{}", std::process::id()));
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
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

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
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

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
