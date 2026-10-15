//! Low-level Server-Sent Events (text/event-stream) parser and LLM streaming primitives.
//!
//! Implements W3C SSE line/event parsing over any [`std::io::BufRead`] source,
//! multi-line data concatenation, comment filtering, blank-line event boundaries,
//! CRLF tolerance, clean EOF termination, and OpenAI-compatible chunk delta extraction.

use std::io::BufRead;

/// A parsed Server-Sent Event (SSE).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// Optional event type declared by `event: <name>` lines.
    pub event: Option<String>,
    /// Accumulated payload data from `data: <chunk>` lines joined by newlines.
    pub data: String,
}

impl SseEvent {
    /// Create a new `SseEvent` with optional event name and data payload.
    pub fn new(event: Option<String>, data: impl Into<String>) -> Self {
        Self {
            event,
            data: data.into(),
        }
    }

    /// Check if this event's data payload is the stream termination sentinel (`[DONE]`).
    pub fn is_done(&self) -> bool {
        is_done_sentinel(&self.data)
    }

    /// Parse this event's data payload as an OpenAI-compatible chunk.
    pub fn parse_openai(&self) -> ChunkOutcome {
        parse_openai_chunk(&self.data)
    }
}

/// Outcome of parsing a chunk payload in OpenAI-compatible streaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkOutcome {
    /// A text delta chunk containing incremental content.
    Delta(String),
    /// The stream completion sentinel (`[DONE]`).
    Done,
    /// Any other chunk (e.g. metadata, empty choices, role-only, finish_reason).
    Other,
}

/// Checks whether a raw SSE data string is the OpenAI `[DONE]` termination sentinel.
///
/// Accepts `"[DONE]"`, `"data: [DONE]"`, or whitespace-padded variations.
pub fn is_done_sentinel(data: &str) -> bool {
    let trimmed = data.trim();
    trimmed == "[DONE]" || trimmed.strip_prefix("data:").map(str::trim) == Some("[DONE]")
}

/// Alias for [`is_done_sentinel`].
pub fn is_done(data: &str) -> bool {
    is_done_sentinel(data)
}

/// Extracts the text delta from an OpenAI-compatible chunk JSON payload.
///
/// Returns:
/// - [`ChunkOutcome::Done`] if the data matches the `[DONE]` sentinel.
/// - [`ChunkOutcome::Delta`] if the JSON contains `choices[0].delta.content` (or concatenated delta contents across choices).
/// - [`ChunkOutcome::Other`] if the JSON is valid but does not carry delta content, or if parsing fails.
pub fn parse_openai_chunk(data: &str) -> ChunkOutcome {
    if is_done_sentinel(data) {
        return ChunkOutcome::Done;
    }

    let trimmed = data.trim();
    let json_str = trimmed
        .strip_prefix("data:")
        .map(str::trim)
        .unwrap_or(trimmed);

    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return ChunkOutcome::Other,
    };

    if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
        let mut delta_acc = String::new();
        let mut found = false;

        for choice in choices {
            if let Some(content) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                delta_acc.push_str(content);
                found = true;
            } else if let Some(text) = choice.get("text").and_then(|t| t.as_str()) {
                delta_acc.push_str(text);
                found = true;
            }
        }

        if found {
            return ChunkOutcome::Delta(delta_acc);
        }
    }

    ChunkOutcome::Other
}

/// Convenience helper that extracts the text delta if present, returning `None` for Done or Other.
pub fn extract_openai_delta(data: &str) -> Option<String> {
    match parse_openai_chunk(data) {
        ChunkOutcome::Delta(text) => Some(text),
        _ => None,
    }
}

/// A generic Server-Sent Events parser over any reader implementing [`BufRead`].
///
/// Conforms to the W3C SSE event stream format:
/// - Lines starting with `:` are comments and ignored.
/// - Lines starting with `data:` append to the payload, joined with newlines for multi-line data.
/// - Lines starting with `event:` set the event name.
/// - Blank lines trigger event dispatch.
/// - Both LF and CRLF line terminators are supported.
/// - Unterminated trailing events at EOF are dispatched before the stream yields `None`.
pub struct SseReader<R: BufRead> {
    reader: R,
    current_event: Option<String>,
    current_data: String,
    has_data: bool,
    ended: bool,
    line_buf: String,
}

impl<R: BufRead> SseReader<R> {
    /// Create a new `SseReader` wrapping a [`BufRead`] source.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            current_event: None,
            current_data: String::new(),
            has_data: false,
            ended: false,
            line_buf: String::new(),
        }
    }

    /// Read the next event from the stream.
    ///
    /// Returns:
    /// - `Ok(Some(event))` when an event is parsed.
    /// - `Ok(None)` when the stream has terminated at EOF.
    /// - `Err(message)` if an underlying I/O read error occurs.
    pub fn read_event(&mut self) -> Result<Option<SseEvent>, String> {
        if crate::task::is_current_cancelled() {
            self.ended = true;
            return Err("error[E-task-0001]: TaskCancelled".to_string());
        }
        if self.ended {
            return Ok(None);
        }

        loop {
            if crate::task::is_current_cancelled() {
                self.ended = true;
                return Err("error[E-task-0001]: TaskCancelled".to_string());
            }
            self.line_buf.clear();
            let bytes_read = match self.reader.read_line(&mut self.line_buf) {
                Ok(n) => n,
                Err(e) => {
                    self.ended = true;
                    return Err(format!("I/O error reading SSE stream: {}", e));
                }
            };

            if bytes_read == 0 {
                self.ended = true;
                // If stream ended at EOF without a trailing blank line, dispatch any pending event.
                if self.has_data || self.current_event.is_some() {
                    let event = SseEvent {
                        event: self.current_event.take(),
                        data: std::mem::take(&mut self.current_data),
                    };
                    self.has_data = false;
                    return Ok(Some(event));
                }
                return Ok(None);
            }

            let mut line = self.line_buf.as_str();
            if let Some(stripped) = line.strip_suffix('\n') {
                line = stripped;
            }
            if let Some(stripped) = line.strip_suffix('\r') {
                line = stripped;
            }

            // Blank line marks the boundary between events.
            if line.is_empty() {
                if self.has_data || self.current_event.is_some() {
                    let event = SseEvent {
                        event: self.current_event.take(),
                        data: std::mem::take(&mut self.current_data),
                    };
                    self.has_data = false;
                    return Ok(Some(event));
                }
                continue;
            }

            // Comment lines start with ':' and are ignored.
            if line.starts_with(':') {
                continue;
            }

            // Parse field and value.
            let (field, value) = if let Some((f, v)) = line.split_once(':') {
                let v = v.strip_prefix(' ').unwrap_or(v);
                (f, v)
            } else {
                (line, "")
            };

            match field {
                "data" => {
                    if self.has_data {
                        self.current_data.push('\n');
                    }
                    self.current_data.push_str(value);
                    self.has_data = true;
                }
                "event" => {
                    self.current_event = Some(value.to_string());
                }
                _ => {
                    // Ignore other fields (id, retry, unknown) per SSE specification.
                }
            }
        }
    }

    /// Consume the `SseReader` and return the underlying reader.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: BufRead> Iterator for SseReader<R> {
    type Item = Result<SseEvent, String>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_event() {
            Ok(Some(ev)) => Some(Ok(ev)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn test_simple_two_event_stream() {
        let input = "data: first event\n\ndata: second event\n\n";
        let reader = SseReader::new(BufReader::new(input.as_bytes()));
        let events: Result<Vec<_>, _> = reader.collect();
        let events = events.expect("stream should parse cleanly");

        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            SseEvent {
                event: None,
                data: "first event".to_string(),
            }
        );
        assert_eq!(
            events[1],
            SseEvent {
                event: None,
                data: "second event".to_string(),
            }
        );
    }

    #[test]
    fn test_multi_line_data() {
        let input = "data: line one\ndata: line two\ndata: line three\n\n";
        let reader = SseReader::new(BufReader::new(input.as_bytes()));
        let events: Result<Vec<_>, _> = reader.collect();
        let events = events.expect("stream should parse cleanly");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, None);
        assert_eq!(events[0].data, "line one\nline two\nline three");
    }

    #[test]
    fn test_comment_lines_ignored() {
        let input = ": stream initial comment\ndata: event one\n: comment in event\n\n: standalone comment\n: another comment\ndata: event two\n\n: trailing comment\n";
        let reader = SseReader::new(BufReader::new(input.as_bytes()));
        let events: Result<Vec<_>, _> = reader.collect();
        let events = events.expect("stream should parse cleanly");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "event one");
        assert_eq!(events[1].data, "event two");
    }

    #[test]
    fn test_crlf_line_endings() {
        let input = "event: update\r\ndata: chunk 1\r\ndata: chunk 2\r\n\r\nevent: done\r\ndata: [DONE]\r\n\r\n";
        let reader = SseReader::new(BufReader::new(input.as_bytes()));
        let events: Result<Vec<_>, _> = reader.collect();
        let events = events.expect("stream should parse cleanly");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.as_deref(), Some("update"));
        assert_eq!(events[0].data, "chunk 1\nchunk 2");
        assert_eq!(events[1].event.as_deref(), Some("done"));
        assert_eq!(events[1].data, "[DONE]");
    }

    #[test]
    fn test_openai_chunk_parsing() {
        let chunk_json =
            r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{"content":"Hello world"}}]}"#;
        assert_eq!(
            parse_openai_chunk(chunk_json),
            ChunkOutcome::Delta("Hello world".to_string())
        );

        let done_payload = "[DONE]";
        assert_eq!(parse_openai_chunk(done_payload), ChunkOutcome::Done);
        assert!(is_done_sentinel(done_payload));
        assert!(is_done(done_payload));

        let done_with_prefix = "data: [DONE]";
        assert_eq!(parse_openai_chunk(done_with_prefix), ChunkOutcome::Done);
        assert!(is_done_sentinel(done_with_prefix));

        let role_chunk =
            r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_openai_chunk(role_chunk), ChunkOutcome::Other);

        let finish_chunk =
            r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        assert_eq!(parse_openai_chunk(finish_chunk), ChunkOutcome::Other);

        let empty_choices = r#"{"id":"chatcmpl-1","choices":[]}"#;
        assert_eq!(parse_openai_chunk(empty_choices), ChunkOutcome::Other);

        let invalid_json = "this is not json";
        assert_eq!(parse_openai_chunk(invalid_json), ChunkOutcome::Other);
    }

    #[test]
    fn test_unterminated_trailing_event_at_eof() {
        // Documented behavior: when the stream terminates at EOF without a final blank line,
        // any accumulated event is dispatched rather than silently dropped.
        let input = "data: unterminated trailing payload";
        let reader = SseReader::new(BufReader::new(input.as_bytes()));
        let events: Result<Vec<_>, _> = reader.collect();
        let events = events.expect("stream should parse cleanly");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "unterminated trailing payload");
        assert_eq!(events[0].event, None);
    }

    #[test]
    fn test_anthropic_event_passthrough() {
        let input = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"hi\"}}\n\n";
        let reader = SseReader::new(BufReader::new(input.as_bytes()));
        let mut events = reader.into_iter();

        let first = events.next().expect("has event").expect("parses correctly");
        assert_eq!(first.event.as_deref(), Some("content_block_delta"));
        assert_eq!(
            first.data,
            "{\"type\":\"content_block_delta\",\"delta\":{\"text\":\"hi\"}}"
        );

        assert!(events.next().is_none());
    }

    #[test]
    fn test_multiple_consecutive_blank_lines() {
        let input = "\n\n\ndata: first\n\n\n\ndata: second\n\n\n";
        let reader = SseReader::new(BufReader::new(input.as_bytes()));
        let events: Vec<_> = reader.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].data, "second");
    }

    #[test]
    fn test_extract_openai_delta_helper() {
        let chunk_json = r#"{"choices":[{"delta":{"content":"token1"}}]}"#;
        assert_eq!(extract_openai_delta(chunk_json), Some("token1".to_string()));

        let done_str = "[DONE]";
        assert_eq!(extract_openai_delta(done_str), None);

        let other_json = r#"{"choices":[{"delta":{"role":"system"}}]}"#;
        assert_eq!(extract_openai_delta(other_json), None);
    }

    #[test]
    fn test_event_methods() {
        let event_done = SseEvent::new(None, "[DONE]");
        assert!(event_done.is_done());
        assert_eq!(event_done.parse_openai(), ChunkOutcome::Done);

        let event_chunk = SseEvent::new(
            Some("message".to_string()),
            r#"{"choices":[{"delta":{"content":"abc"}}]}"#,
        );
        assert!(!event_chunk.is_done());
        assert_eq!(
            event_chunk.parse_openai(),
            ChunkOutcome::Delta("abc".to_string())
        );
    }
}
