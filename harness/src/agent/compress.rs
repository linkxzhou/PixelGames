//! Transcript compression (structural port of miniagent `context/`,
//! batch-mode adaptation). When the message list grows past a threshold,
//! older tool-result payloads are replaced with digest stubs pointing at
//! the persisted `tool-results/` texts. Ordering is never changed; the
//! system prompt and the first user message are always kept verbatim.

use crate::agent::provider::ChatMessage;

/// Compress when the estimated transcript exceeds this many bytes.
pub const COMPRESS_THRESHOLD_BYTES: usize = 96 * 1024;
/// Keep the most recent messages fully intact.
const KEEP_RECENT_BYTES: usize = 32 * 1024;
/// Tool results longer than this get stubbed during compression.
const STUB_ABOVE_CHARS: usize = 2_000;

fn message_weight(m: &ChatMessage) -> usize {
    m.content.len()
}

/// Estimate the transcript size.
pub fn transcript_size(messages: &[ChatMessage]) -> usize {
    messages.iter().map(message_weight).sum()
}

/// Compress in place. Returns the number of stubbed messages.
/// Deterministic: only length triggers it, only tool payloads shrink.
pub fn compress_transcript(messages: &mut Vec<ChatMessage>) -> usize {
    if transcript_size(messages) < COMPRESS_THRESHOLD_BYTES {
        return 0;
    }
    // Walk from the newest message backwards, accumulating protected bytes.
    let mut protected = 0usize;
    let mut protected_from = messages.len();
    for (i, m) in messages.iter().enumerate().rev() {
        protected += message_weight(m);
        if protected > KEEP_RECENT_BYTES {
            protected_from = i + 1;
            break;
        }
        protected_from = i;
    }
    let mut stubbed = 0usize;
    for m in messages[..protected_from].iter_mut() {
        if m.role == "tool" && m.content.len() > STUB_ABOVE_CHARS {
            let digest = crate::digest::digest_bytes(m.content.as_bytes());
            m.content = format!(
                "[tool result compressed: sha256 short {} — full text in tool-results/]",
                digest
            );
            stubbed += 1;
        }
    }
    stubbed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_transcript_is_untouched() {
        let mut messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("hello"),
            ChatMessage::assistant("hi"),
        ];
        assert_eq!(compress_transcript(&mut messages), 0);
        assert_eq!(messages[1].content, "hello");
    }

    #[test]
    fn long_tool_results_get_stubbed_but_order_and_recent_kept() {
        let mut messages = vec![ChatMessage::system("sys"), ChatMessage::user("task")];
        let big = "x".repeat(STUB_ABOVE_CHARS + 1);
        // Push enough volume to cross the threshold.
        while transcript_size(&messages) < COMPRESS_THRESHOLD_BYTES {
            messages.push(ChatMessage::assistant("turn"));
            messages.push(ChatMessage {
                role: "tool".into(),
                content: big.clone(),
                tool_calls: None,
                tool_name: Some("t".into()),
                tool_call_id: Some("c".into()),
            });
        }
        let before = transcript_size(&messages);
        let stubbed = compress_transcript(&mut messages);
        assert!(stubbed > 0);
        assert!(transcript_size(&messages) < before);
        // Ordering preserved, head kept verbatim.
        assert_eq!(messages[0].content, "sys");
        assert_eq!(messages[1].content, "task");
        // The most recent tool result stays intact (inside the keep window).
        let last_tool = messages
            .iter()
            .rev()
            .find(|m| m.role == "tool")
            .unwrap();
        assert_eq!(last_tool.content, big);
        // Every stub references its persisted full text.
        assert!(messages.iter().any(|m| {
            m.role == "tool" && m.content.contains("[tool result compressed")
        }));
    }
}
