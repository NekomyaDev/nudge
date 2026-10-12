use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Create a configured ureq Agent with the specified timeout.
pub fn build_agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(timeout).build()
}

/// Send an HTTP POST request with a JSON body and custom headers using the default timeout (120s).
///
/// Returns `Ok((status_code, response_body))` or `Err(error_message)` on transport/network error.
pub fn post_json(
    url: &str,
    headers: &[(String, String)],
    body: &str,
) -> Result<(u16, String), String> {
    post_json_with_timeout(url, headers, body, DEFAULT_TIMEOUT)
}

/// Send an HTTP POST request with a JSON body, custom headers, and a specified timeout.
pub fn post_json_with_timeout(
    url: &str,
    headers: &[(String, String)],
    body: &str,
    timeout: Duration,
) -> Result<(u16, String), String> {
    let agent = build_agent(timeout);
    let mut req = agent.post(url);

    let mut has_content_type = false;
    for (key, val) in headers {
        if key.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        req = req.set(key, val);
    }
    if !has_content_type {
        req = req.set("Content-Type", "application/json");
    }

    match req.send_string(body) {
        Ok(resp) => {
            let status = resp.status();
            let text = resp
                .into_string()
                .map_err(|e| format!("Failed to read response body: {}", e))?;
            Ok((status, text))
        }
        Err(ureq::Error::Status(status, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            Ok((status, text))
        }
        Err(ureq::Error::Transport(transport_err)) => {
            Err(format!("HTTP transport error: {}", transport_err))
        }
    }
}

/// Send an HTTP GET request with custom headers using the default timeout (120s).
///
/// Returns `Ok((status_code, response_body))` or `Err(error_message)` on transport/network error.
pub fn get(url: &str, headers: &[(String, String)]) -> Result<(u16, String), String> {
    get_with_timeout(url, headers, DEFAULT_TIMEOUT)
}

/// Send an HTTP GET request with custom headers and a specified timeout.
pub fn get_with_timeout(
    url: &str,
    headers: &[(String, String)],
    timeout: Duration,
) -> Result<(u16, String), String> {
    let agent = build_agent(timeout);
    let mut req = agent.get(url);

    for (key, val) in headers {
        req = req.set(key, val);
    }

    match req.call() {
        Ok(resp) => {
            let status = resp.status();
            let text = resp
                .into_string()
                .map_err(|e| format!("Failed to read response body: {}", e))?;
            Ok((status, text))
        }
        Err(ureq::Error::Status(status, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            Ok((status, text))
        }
        Err(ureq::Error::Transport(transport_err)) => {
            Err(format!("HTTP transport error: {}", transport_err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn test_http_post_json_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut req_bytes = Vec::new();
            let mut buf = [0u8; 512];
            let mut content_length = 0;
            let mut header_end = None;

            loop {
                let n = stream.read(&mut buf).expect("read request");
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

            // Verify method, path, headers, and body
            assert!(req_str.starts_with("POST /api/test HTTP/1.1"));
            assert!(req_str
                .to_ascii_lowercase()
                .contains("authorization: bearer test-secret"));
            assert!(req_str
                .to_ascii_lowercase()
                .contains("content-type: application/json"));
            assert!(req_str.to_ascii_lowercase().contains("custom-hdr: 123"));

            let expected_body = r#"{"name":"nudge"}"#;
            assert!(req_str.contains(expected_body));

            let resp_body = r#"{"status":"success","received":true}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let url = format!("http://127.0.0.1:{}/api/test", port);
        let headers = vec![
            (
                "Authorization".to_string(),
                "Bearer test-secret".to_string(),
            ),
            ("Custom-Hdr".to_string(), "123".to_string()),
        ];
        let payload = r#"{"name":"nudge"}"#;

        let (status, body) = post_json(&url, &headers, payload).expect("post_json should succeed");
        assert_eq!(status, 200);
        assert!(body.contains("\"status\":\"success\""));
        assert!(body.contains("\"received\":true"));

        handle.join().unwrap();
    }

    #[test]
    fn test_http_get_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).expect("read request");
            let req_str = String::from_utf8_lossy(&buf[..n]);

            assert!(req_str.starts_with("GET /api/models HTTP/1.1"));
            assert!(
                req_str.contains("x-api-key: secret-key")
                    || req_str.contains("X-Api-Key: secret-key")
            );

            let resp_body = r#"{"models":["m1","m2"]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let url = format!("http://127.0.0.1:{}/api/models", port);
        let headers = vec![("x-api-key".to_string(), "secret-key".to_string())];

        let (status, body) = get(&url, &headers).expect("get should succeed");
        assert_eq!(status, 200);
        assert!(body.contains("\"models\":[\"m1\",\"m2\"]"));

        handle.join().unwrap();
    }

    #[test]
    fn test_http_post_error_status_handling() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);

            let err_body = r#"{"error":{"message":"Invalid API key"}}"#;
            let resp = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                err_body.len(),
                err_body
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let url = format!("http://127.0.0.1:{}/v1/chat/completions", port);
        let (status, body) = post_json(&url, &[], "{}").expect("post_json should capture 401");
        assert_eq!(status, 401);
        assert!(body.contains("Invalid API key"));

        handle.join().unwrap();
    }

    #[test]
    fn test_http_transport_error_on_unreachable_host() {
        // Port 1 is reserved and typically nothing listens on it, or connection refused immediately
        let url = "http://127.0.0.1:1/nonexistent";
        let res = post_json_with_timeout(url, &[], "{}", Duration::from_millis(50));
        assert!(
            res.is_err(),
            "expected connection error to unreachable port"
        );
    }
}
