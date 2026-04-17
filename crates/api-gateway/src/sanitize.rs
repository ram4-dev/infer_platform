use once_cell::sync::Lazy;
use regex::Regex;

use crate::routes::chat::ChatCompletionRequest;

const ALLOWED_ROLES: &[&str] = &["system", "user", "assistant", "tool", "function"];
const MAX_MESSAGES: usize = 100;
const MAX_CONTENT_CHARS: usize = 32_768;
const MAX_MODEL_LEN: usize = 256;

#[derive(Debug)]
pub enum SanitizeError {
    InvalidModel,
    EmptyMessages,
    TooManyMessages,
    InvalidRole(String),
    ContentTooLong,
}

impl std::fmt::Display for SanitizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidModel => write!(f, "model name is invalid"),
            Self::EmptyMessages => write!(f, "messages must not be empty"),
            Self::TooManyMessages => write!(
                f,
                "too many messages (max {MAX_MESSAGES})"
            ),
            Self::InvalidRole(r) => write!(
                f,
                "invalid role '{r}' — must be one of: system, user, assistant, tool, function"
            ),
            Self::ContentTooLong => write!(
                f,
                "message content exceeds {MAX_CONTENT_CHARS} character limit"
            ),
        }
    }
}

/// Validate and clean an incoming chat completion request in-place.
///
/// Rejects: invalid model names, empty/oversized message lists, unknown roles,
/// oversized content. Strips null bytes and non-printable control characters
/// from message content (preserving \t, \n, \r).
pub fn sanitize_request(req: &mut ChatCompletionRequest) -> Result<(), SanitizeError> {
    if req.model.is_empty()
        || req.model.len() > MAX_MODEL_LEN
        || req.model.contains('\x00')
        || req.model.contains("..")
        || req.model.contains('/')
        || req.model.contains('\\')
    {
        return Err(SanitizeError::InvalidModel);
    }

    if req.messages.is_empty() {
        return Err(SanitizeError::EmptyMessages);
    }
    if req.messages.len() > MAX_MESSAGES {
        return Err(SanitizeError::TooManyMessages);
    }

    for msg in &mut req.messages {
        if !ALLOWED_ROLES.contains(&msg.role.as_str()) {
            return Err(SanitizeError::InvalidRole(msg.role.clone()));
        }

        // Strip null bytes and control characters, keeping tab/newline/carriage-return.
        msg.content = msg
            .content
            .chars()
            .filter(|&c| c == '\t' || c == '\n' || c == '\r' || !c.is_control())
            .collect();

        if msg.content.len() > MAX_CONTENT_CHARS {
            return Err(SanitizeError::ContentTooLong);
        }
    }

    Ok(())
}

// ── PII patterns ─────────────────────────────────────────────────────────────

static SSN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap());

static CREDIT_CARD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b").unwrap()
});

static EMAIL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap()
});

static PHONE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:\+?1[\s\-.]?)?\(?\d{3}\)?[\s\-.]?\d{3}[\s\-.]?\d{4}\b").unwrap()
});

/// Redact common PII patterns from a model response string.
///
/// Patterns replaced (in order to avoid double-redaction):
///   SSN → [SSN REDACTED]
///   Credit card → [CARD REDACTED]
///   Email → [EMAIL REDACTED]
///   Phone → [PHONE REDACTED]
pub fn filter_pii(text: &str) -> String {
    let s = SSN_RE.replace_all(text, "[SSN REDACTED]");
    let s = CREDIT_CARD_RE.replace_all(&s, "[CARD REDACTED]");
    let s = EMAIL_RE.replace_all(&s, "[EMAIL REDACTED]");
    let s = PHONE_RE.replace_all(&s, "[PHONE REDACTED]");
    s.into_owned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::chat::ChatMessage;

    fn make_req(model: &str, messages: Vec<(&str, &str)>) -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: model.to_string(),
            messages: messages
                .into_iter()
                .map(|(role, content)| ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                })
                .collect(),
            stream: false,
            max_tokens: None,
            temperature: None,
            top_p: None,
        }
    }

    #[test]
    fn valid_request_passes() {
        let mut req = make_req("llama3", vec![("user", "Hello!")]);
        assert!(sanitize_request(&mut req).is_ok());
    }

    #[test]
    fn rejects_path_traversal_model() {
        let mut req = make_req("../../etc/passwd", vec![("user", "hi")]);
        assert!(matches!(sanitize_request(&mut req), Err(SanitizeError::InvalidModel)));
    }

    #[test]
    fn rejects_model_with_slash() {
        let mut req = make_req("models/llama3", vec![("user", "hi")]);
        assert!(matches!(sanitize_request(&mut req), Err(SanitizeError::InvalidModel)));
    }

    #[test]
    fn rejects_empty_messages() {
        let mut req = make_req("llama3", vec![]);
        assert!(matches!(sanitize_request(&mut req), Err(SanitizeError::EmptyMessages)));
    }

    #[test]
    fn rejects_invalid_role() {
        let mut req = make_req("llama3", vec![("admin", "do something")]);
        assert!(matches!(sanitize_request(&mut req), Err(SanitizeError::InvalidRole(_))));
    }

    #[test]
    fn strips_null_bytes() {
        let mut req = make_req("llama3", vec![("user", "hello\x00world")]);
        sanitize_request(&mut req).unwrap();
        assert_eq!(req.messages[0].content, "helloworld");
    }

    #[test]
    fn preserves_newlines_and_tabs() {
        let mut req = make_req("llama3", vec![("user", "line1\nline2\ttabbed")]);
        sanitize_request(&mut req).unwrap();
        assert_eq!(req.messages[0].content, "line1\nline2\ttabbed");
    }

    #[test]
    fn pii_filter_ssn() {
        let out = filter_pii("SSN is 123-45-6789 for patient");
        assert_eq!(out, "SSN is [SSN REDACTED] for patient");
    }

    #[test]
    fn pii_filter_credit_card() {
        let out = filter_pii("Card: 4111 1111 1111 1111");
        assert_eq!(out, "Card: [CARD REDACTED]");
    }

    #[test]
    fn pii_filter_email() {
        let out = filter_pii("Contact user@example.com for details");
        assert_eq!(out, "Contact [EMAIL REDACTED] for details");
    }

    #[test]
    fn pii_filter_phone() {
        let out = filter_pii("Call 555-867-5309 now");
        assert_eq!(out, "Call [PHONE REDACTED] now");
    }

    #[test]
    fn pii_filter_clean_text() {
        let text = "The weather today is nice.";
        assert_eq!(filter_pii(text), text);
    }
}
