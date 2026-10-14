use std::io::BufRead;
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Create a configured ureq Agent with the specified timeout.
pub fn build_agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(timeout).build()
}

/// Merge request headers with default headers (`Content-Type: application/json`),
/// avoiding unnecessary string allocations.
pub fn merge_request_headers(headers: &[(String, String)]) -> Vec<(&str, &str)> {
    let mut result = Vec::with_capacity(headers.len() + 1);
    let mut has_content_type = false;
    for (key, val) in headers {
        if key.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        result.push((key.as_str(), val.as_str()));
    }
    if !has_content_type {
        result.push(("Content-Type", "application/json"));
    }
    result
}

fn apply_headers(mut req: ureq::Request, headers: &[(String, String)]) -> ureq::Request {
    for (key, val) in merge_request_headers(headers) {
        req = req.set(key, val);
    }
    req
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
    let req = agent.post(url);
    let req = apply_headers(req, headers);

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

/// Prepare the JSON request body for streaming chat completion,
/// ensuring `"stream": true` is set on the object.
pub fn prepare_streaming_body(body: &serde_json::Value) -> Result<serde_json::Value, String> {
    match body {
        serde_json::Value::Object(map) => {
            let mut updated = map.clone();
            updated.insert("stream".to_string(), serde_json::Value::Bool(true));
            Ok(serde_json::Value::Object(updated))
        }
        _ => Err("Request body must be a JSON object".to_string()),
    }
}

/// Serialize a streaming chat completion request body to JSON string with `"stream": true`.
pub fn prepare_streaming_payload(body: &serde_json::Value) -> Result<String, String> {
    let stream_body = prepare_streaming_body(body)?;
    serde_json::to_string(&stream_body).map_err(|e| format!("Failed to serialize JSON body: {}", e))
}

/// Send an HTTP POST request for streaming chat completion using default timeout (120s).
///
/// Ensures `"stream": true` is included in the request body, and returns the response body
/// as a buffered reader ([`BufRead`]) for incremental SSE consumption.
pub fn post_chat_stream(
    url: &str,
    headers: &[(String, String)],
    body: &serde_json::Value,
) -> Result<Box<dyn BufRead>, String> {
    post_chat_stream_with_timeout(url, headers, body, DEFAULT_TIMEOUT)
}

/// Send an HTTP POST request for streaming chat completion with a specified timeout.
///
/// Ensures `"stream": true` is included in the request body, and returns the response body
/// as a buffered reader ([`BufRead`]) for incremental SSE consumption.
pub fn post_chat_stream_with_timeout(
    url: &str,
    headers: &[(String, String)],
    body: &serde_json::Value,
    timeout: Duration,
) -> Result<Box<dyn BufRead>, String> {
    let payload = prepare_streaming_payload(body)?;
    let agent = build_agent(timeout);
    let req = agent.post(url);
    let req = apply_headers(req, headers);

    match req.send_string(&payload) {
        Ok(resp) => {
            let status = resp.status();
            if !(200..300).contains(&status) {
                let text = resp.into_string().unwrap_or_default();
                if text.is_empty() {
                    return Err(format!("HTTP error status {}", status));
                } else {
                    return Err(format!("HTTP error status {}: {}", status, text));
                }
            }
            Ok(Box::new(std::io::BufReader::new(resp.into_reader())))
        }
        Err(ureq::Error::Status(status, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            if text.is_empty() {
                Err(format!("HTTP error status {}", status))
            } else {
                Err(format!("HTTP error status {}: {}", status, text))
            }
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

    #[test]
    fn test_merge_request_headers_adds_content_type_when_missing() {
        let headers = vec![("Authorization".to_string(), "Bearer test-key".to_string())];
        let merged = merge_request_headers(&headers);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0], ("Authorization", "Bearer test-key"));
        assert_eq!(merged[1], ("Content-Type", "application/json"));
    }

    #[test]
    fn test_merge_request_headers_preserves_case_insensitive_content_type() {
        let headers = vec![
            ("authorization".to_string(), "Bearer token".to_string()),
            (
                "CONTENT-TYPE".to_string(),
                "application/json; charset=utf-8".to_string(),
            ),
        ];
        let merged = merge_request_headers(&headers);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0], ("authorization", "Bearer token"));
        assert_eq!(
            merged[1],
            ("CONTENT-TYPE", "application/json; charset=utf-8")
        );
    }

    #[test]
    fn test_prepare_streaming_body_injects_stream_true() {
        let body = serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello world"}]
        });
        let result = prepare_streaming_body(&body).expect("should succeed on json object");
        assert_eq!(result["stream"], serde_json::Value::Bool(true));
        assert_eq!(result["model"], "gpt-4o");
        assert_eq!(result["messages"][0]["content"], "hello world");
    }

    #[test]
    fn test_prepare_streaming_body_overwrites_existing_stream_false() {
        let body = serde_json::json!({
            "model": "claude-3-opus",
            "stream": false
        });
        let result = prepare_streaming_body(&body).expect("should succeed on json object");
        assert_eq!(result["stream"], serde_json::Value::Bool(true));
        assert_eq!(result["model"], "claude-3-opus");
    }

    #[test]
    fn test_prepare_streaming_body_rejects_non_object() {
        let str_val = serde_json::json!("a raw string");
        assert!(prepare_streaming_body(&str_val).is_err());

        let arr_val = serde_json::json!([1, 2, 3]);
        assert!(prepare_streaming_body(&arr_val).is_err());

        let num_val = serde_json::json!(42);
        assert!(prepare_streaming_body(&num_val).is_err());

        let null_val = serde_json::Value::Null;
        assert!(prepare_streaming_body(&null_val).is_err());
    }

    #[test]
    fn test_prepare_streaming_payload_produces_valid_json() {
        let body = serde_json::json!({"model": "test-model"});
        let payload = prepare_streaming_payload(&body).expect("serialization succeeds");
        let parsed: serde_json::Value =
            serde_json::from_str(&payload).expect("payload is valid json");
        assert_eq!(parsed["stream"], true);
        assert_eq!(parsed["model"], "test-model");
    }
}
