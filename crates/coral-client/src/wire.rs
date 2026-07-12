//! The flat `VERB key=value key=value ...` wire grammar real CoralOS agents
//! use on the market thread, since `send_message` only carries a plain
//! `content` string (no structured-payload field exists on the real
//! transport). Port of `pay`'s `packages/agent-runtime/src/market/protocol.ts`
//! token helpers — pure, network-free, and unit-testable.

/// The leading verb of a message (`PROOF_REQUESTED`, `PROOF_VERDICT`, ...),
/// or empty if none.
#[must_use]
pub fn verb(text: &str) -> String {
    text.split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
}

/// Extract a bare (non-quoted) `key=value` token.
#[must_use]
pub fn tok<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    for word in text.split_whitespace() {
        if let Some(value) = word.strip_prefix(key).and_then(|w| w.strip_prefix('=')) {
            return Some(value);
        }
    }
    None
}

/// Extract a numeric `key=value` token.
#[must_use]
pub fn num(text: &str, key: &str) -> Option<f64> {
    tok(text, key).and_then(|v| v.parse().ok())
}

/// Extract a `key="quoted value"` token (may contain spaces).
#[must_use]
pub fn quoted(text: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = text.find(&needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Extract the raw JSON value of a `key={...}` or `key=[...]` token.
///
/// This is the one shared extractor for every field that carries JSON on
/// the flat grammar (`wager=`, `signal=`, `decision=`, `toolTrail=`). It
/// replaces the per-agent copies that were either greedy to end-of-string
/// (proof-guard, fan-pundit — which broke as soon as any key followed the
/// JSON) or brace-matching but not string-aware (settlement,
/// trading-specialist — which broke on a `{` inside a thesis string).
///
/// Matching rules:
/// - `key=` must start a whitespace-delimited word, so the same byte
///   sequence *inside* another field's JSON string is not mistaken for the
///   key.
/// - The value must begin with `{` or `[` immediately after the `=`.
/// - The value ends at the matching close bracket, tracked through nested
///   containers, JSON strings, and `\"` escapes — everything after it
///   (other `key=value` tokens) is ignored.
#[must_use]
pub fn json_val<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=");
    let mut search_from = 0;
    let idx = loop {
        let rel = text.get(search_from..)?.find(&needle)?;
        let abs = search_from + rel;
        let at_word_start =
            abs == 0 || text[..abs].chars().next_back().is_some_and(char::is_whitespace);
        let value = &text[abs + needle.len()..];
        if at_word_start && (value.starts_with('{') || value.starts_with('[')) {
            break abs;
        }
        search_from = abs + needle.len();
    };

    let rest = &text[idx + needle.len()..];
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in rest.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_verb_and_tokens() {
        let text = r#"PROOF_VERDICT round=3 passed=true wager=abc123 reason="edge inconsistent""#;
        assert_eq!(verb(text), "PROOF_VERDICT");
        assert_eq!(num(text, "round"), Some(3.0));
        assert_eq!(tok(text, "wager"), Some("abc123"));
        assert_eq!(quoted(text, "reason"), Some("edge inconsistent".to_string()));
    }

    #[test]
    fn missing_tokens_are_none() {
        assert_eq!(tok("PROOF_REQUESTED round=1", "wager"), None);
        assert_eq!(quoted("PROOF_REQUESTED round=1", "reason"), None);
    }

    #[test]
    fn json_val_extracts_trailing_object() {
        let text = r#"WAGER_PROOF_REQUESTED round=1 wager={"wagerId":"w-1","edge":0.05}"#;
        assert_eq!(json_val(text, "wager"), Some(r#"{"wagerId":"w-1","edge":0.05}"#));
    }

    #[test]
    fn json_val_stops_at_matching_brace_when_keys_follow() {
        let text = r#"SETTLE_REQUESTED wager={"a":{"b":1}} proofRef=txoracle:x"#;
        assert_eq!(json_val(text, "wager"), Some(r#"{"a":{"b":1}}"#));
    }

    #[test]
    fn json_val_survives_braces_and_quotes_inside_strings() {
        let text = r#"V toolTrail=[] wager={"thesis":"odd {text} with \" and wager= inside","n":1} tail=x"#;
        assert_eq!(
            json_val(text, "wager"),
            Some(r#"{"thesis":"odd {text} with \" and wager= inside","n":1}"#)
        );
    }

    #[test]
    fn json_val_ignores_key_lookalikes_inside_other_json() {
        // "wager=" appears inside the toolTrail JSON string but not as a
        // word-initial key; the real wager= token later must win.
        let text = r#"V trail={"note":"wager={fake}"} wager={"real":true}"#;
        assert_eq!(json_val(text, "wager"), Some(r#"{"real":true}"#));
    }

    #[test]
    fn json_val_requires_immediate_brace_and_returns_none_otherwise() {
        assert_eq!(json_val("V wager=abc123", "wager"), None);
        assert_eq!(json_val("V wager= {\"a\":1}", "wager"), None);
        assert_eq!(json_val("V other={\"a\":1}", "wager"), None);
        // Unterminated object.
        assert_eq!(json_val("V wager={\"a\":1", "wager"), None);
    }

    #[test]
    fn json_val_extracts_arrays() {
        let text = r#"V toolTrail=[{"tool":"a","result":{"x":1}},{"tool":"b","result":[]}] wager={"real":true}"#;
        assert_eq!(
            json_val(text, "toolTrail"),
            Some(r#"[{"tool":"a","result":{"x":1}},{"tool":"b","result":[]}]"#)
        );
        assert_eq!(json_val(text, "wager"), Some(r#"{"real":true}"#));
    }
}
