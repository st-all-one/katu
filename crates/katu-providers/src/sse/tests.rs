//! Testes do parser SSE: fragmentação arbitrária, CRLF, comentários e cancelamento.

use super::SseParser;
use katu_core::provider::Flow;

/// Recolhe os eventos de dados de uma sequência de fragmentos.
fn collect(chunks: &[&[u8]]) -> Vec<String> {
    let mut parser = SseParser::new();
    let mut events = Vec::new();
    for chunk in chunks {
        parser.push(chunk, &mut |data| {
            events.push(data.to_string());
            Flow::Continue
        });
    }
    events
}

#[test]
fn parses_openai_stream_across_every_boundary() {
    let raw = b"data: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n";
    for step in 1..=raw.len() {
        let chunks: Vec<&[u8]> = raw.chunks(step).collect();
        assert_eq!(
            collect(&chunks),
            vec!["{\"a\":1}", "{\"b\":2}", "[DONE]"],
            "falhou com fragmentos de {step} bytes"
        );
    }
}

#[test]
fn handles_crlf_and_comments() {
    let raw = b": keep-alive\r\ndata: hello\r\n\r\ndata: world\r\n\r\n";
    assert_eq!(collect(&[raw.as_slice()]), vec!["hello", "world"]);
}

#[test]
fn joins_multiline_data_with_newline() {
    let raw = b"data: line1\ndata: line2\n\n";
    assert_eq!(collect(&[raw.as_slice()]), vec!["line1\nline2"]);
}

#[test]
fn keeps_multibyte_characters_intact_byte_by_byte() {
    let raw = "data: {\"t\":\"olá — fim\"}\n\n".as_bytes();
    for step in 1..=raw.len() {
        let chunks: Vec<&[u8]> = raw.chunks(step).collect();
        assert_eq!(collect(&chunks), vec!["{\"t\":\"olá — fim\"}"]);
    }
}

#[test]
fn break_propagates_and_stops_parsing() {
    let mut parser = SseParser::new();
    let mut seen = 0_usize;
    let flow = parser.push(b"data: one\n\ndata: two\n\ndata: three\n\n", &mut |_data| {
        seen = seen.saturating_add(1);
        Flow::Break
    });
    assert!(matches!(flow, Flow::Break));
    assert_eq!(seen, 1);
}
