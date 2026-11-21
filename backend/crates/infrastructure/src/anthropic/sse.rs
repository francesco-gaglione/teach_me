//! Incremental Server-Sent Events parser (WHATWG rules, the subset we need).

use domain::llm::LlmError;
use std::collections::VecDeque;

fn too_long() -> LlmError {
    LlmError::MalformedResponse("event stream line exceeds 1 MiB".into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// Empty when the stream did not name the event.
    pub event: String,
    pub data: String,
}

/// Longest line accepted. Real events are far smaller; a line without a
/// newline past this size is a broken or hostile stream, not memory to keep.
const MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    event: String,
    data: Vec<String>,
    ready: VecDeque<SseEvent>,
}

impl SseParser {
    /// Feeds raw bytes; chunks may split lines (and UTF-8 sequences) anywhere.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), LlmError> {
        self.buffer.extend_from_slice(bytes);
        while let Some(newline) = self.buffer.iter().position(|b| *b == b'\n') {
            if newline > MAX_LINE_BYTES {
                return Err(too_long());
            }
            let mut line: Vec<u8> = self.buffer.drain(..=newline).collect();
            line.pop(); // '\n'
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8(line)
                .map_err(|_| LlmError::MalformedResponse("invalid UTF-8 in event stream".into()))?;
            self.line(&line);
        }
        if self.buffer.len() > MAX_LINE_BYTES {
            return Err(too_long());
        }
        Ok(())
    }

    pub fn next_event(&mut self) -> Option<SseEvent> {
        self.ready.pop_front()
    }

    fn line(&mut self, line: &str) {
        if line.is_empty() {
            // Blank line: dispatch what was gathered (nothing if no data).
            if !self.data.is_empty() {
                self.ready.push_back(SseEvent {
                    event: std::mem::take(&mut self.event),
                    data: self.data.join("\n"),
                });
            }
            self.event.clear();
            self.data.clear();
            return;
        }
        if line.starts_with(':') {
            return; // comment / keep-alive
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => value.clone_into(&mut self.event),
            "data" => self.data.push(value.to_owned()),
            _ => {} // id, retry and unknown fields are not used
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(chunks: &[&[u8]]) -> Vec<SseEvent> {
        let mut p = SseParser::default();
        let mut out = Vec::new();
        for c in chunks {
            p.push(c).unwrap();
            while let Some(e) = p.next_event() {
                out.push(e);
            }
        }
        out
    }

    fn ev(event: &str, data: &str) -> SseEvent {
        SseEvent {
            event: event.into(),
            data: data.into(),
        }
    }

    #[test]
    fn parses_events_and_ignores_comments() {
        let got = parse(&[b": hi\n\nevent: a\ndata: 1\n\nevent: b\ndata: 2\n\n"]);
        assert_eq!(got, [ev("a", "1"), ev("b", "2")]);
    }

    #[test]
    fn handles_crlf_and_multiline_data() {
        let got = parse(&[b"event: a\r\ndata: x\r\ndata: y\r\n\r\n"]);
        assert_eq!(got, [ev("a", "x\ny")]);
    }

    #[test]
    fn chunks_may_split_anywhere_including_inside_utf8() {
        let whole = "event: a\ndata: perché 日本\n\n".as_bytes();
        for split in 1..whole.len() {
            let got = parse(&[&whole[..split], &whole[split..]]);
            assert_eq!(got, [ev("a", "perché 日本")], "split at {split}");
        }
        let bytes: Vec<&[u8]> = whole.chunks(1).collect();
        assert_eq!(parse(&bytes), [ev("a", "perché 日本")]);
    }

    #[test]
    fn event_without_data_is_not_dispatched_and_state_resets() {
        let got = parse(&[b"event: orphan\n\ndata: 1\n\n"]);
        assert_eq!(got, [ev("", "1")]);
    }

    #[test]
    fn unterminated_event_is_not_dispatched() {
        assert_eq!(parse(&[b"event: a\ndata: 1\n"]).len(), 0);
    }

    #[test]
    fn a_line_over_one_mebibyte_is_malformed_with_or_without_a_newline() {
        let big = vec![b'a'; MAX_LINE_BYTES + 1];
        let mut endless = SseParser::default();
        assert!(matches!(
            endless.push(&big),
            Err(LlmError::MalformedResponse(_))
        ));

        let mut terminated = SseParser::default();
        let mut line = b"data: ".to_vec();
        line.extend_from_slice(&big);
        line.push(b'\n');
        assert!(matches!(
            terminated.push(&line),
            Err(LlmError::MalformedResponse(_))
        ));

        // Right at the limit is fine.
        let mut ok = SseParser::default();
        let mut line = b"data: ".to_vec();
        line.extend(std::iter::repeat_n(b'a', MAX_LINE_BYTES - 10));
        line.extend_from_slice(b"\n\n");
        ok.push(&line).unwrap();
        assert!(ok.next_event().is_some());
    }

    #[test]
    fn invalid_utf8_is_malformed() {
        let mut p = SseParser::default();
        assert!(matches!(
            p.push(b"data: \xff\n"),
            Err(LlmError::MalformedResponse(_))
        ));
    }
}
