/// Secret scrubber — applied at every learn ingestion to prevent immortalizing credentials.
///
/// ## Design philosophy: allowlist, not denylist
/// We never store raw file contents, env vars, or arbitrary text blobs.
/// We store structured fields (paths, hashes, error classes, tool names).
/// What we never store cannot leak.
///
/// The scrubber enforces this contract:
///   1. Entropy detection — catches API keys, tokens, passwords by information density.
///   2. Pattern matching — catches explicitly named secrets (key=, token=, password=).
///   3. Length guard — very long single tokens (> 80 chars) are suspicious.
///
/// ## Entropy threshold
/// Natural English text: ~4.0 bits/char
/// Code identifiers:     ~3.5 bits/char  
/// Random base64/hex:    ~5.5–6.0 bits/char
/// Threshold: 4.5 bits/char (conservative — catches keys, misses prose)
///
/// ## False positive handling
/// The scrubber removes the suspicious token from the stored string and replaces it with
/// `[REDACTED]`. It does NOT reject the entire learn call. A partially-redacted memory
/// is still useful and avoids silently dropping information the agent intended to store.
use std::collections::HashSet;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Result of scrubbing a text field.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrubResult {
    /// The scrubbed text (suspicious tokens replaced with `[REDACTED]`).
    pub text: String,
    /// Number of tokens that were redacted.
    pub redacted_count: usize,
    /// Whether any redactions occurred.
    pub was_modified: bool,
}

/// Scrub a text field, replacing high-entropy / pattern-matched tokens with `[REDACTED]`.
///
/// Returns the scrubbed text and metadata about what was redacted.
/// Never panics. Never returns an error — partial scrubbing is always better than rejection.
pub fn scrub(text: &str) -> ScrubResult {
    let mut redacted_count = 0;
    let mut output_words: Vec<String> = Vec::new();

    // First pass: redact inline key=value patterns before word-splitting
    let text = redact_kv_patterns(text, &mut redacted_count);

    for word in text.split_whitespace() {
        if is_suspicious(word) {
            output_words.push("[REDACTED]".to_string());
            redacted_count += 1;
        } else {
            output_words.push(word.to_string());
        }
    }

    let result = output_words.join(" ");
    let was_modified = redacted_count > 0;

    if was_modified {
        tracing::warn!(
            "scrub: redacted {} suspicious token(s) from learn text",
            redacted_count
        );
    }

    ScrubResult {
        text: result,
        redacted_count,
        was_modified,
    }
}

/// Scrub all three fields of a learn call. Returns (state, action, outcome, total_redacted).
pub fn scrub_learn(state: &str, action: &str, outcome: &str) -> (String, String, String, usize) {
    let s = scrub(state);
    let a = scrub(action);
    let o = scrub(outcome);
    let total = s.redacted_count + a.redacted_count + o.redacted_count;
    (s.text, a.text, o.text, total)
}

// ─── Detection logic ──────────────────────────────────────────────────────────

/// Returns true if the token looks like a secret/credential.
fn is_suspicious(token: &str) -> bool {
    // Remove surrounding punctuation for analysis
    let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '+' && c != '/');

    if t.len() < 8 {
        return false; // Too short to be a real secret
    }

    // Length guard: very long single tokens are suspicious
    if t.len() > 80 {
        return true;
    }

    // Entropy check
    if shannon_entropy(t) > 4.5 {
        return true;
    }

    // Base64-like token of meaningful length
    if t.len() >= 24 && is_base64_like(t) {
        return true;
    }

    // Hex string of meaningful length (32+ chars = 128-bit key)
    if t.len() >= 32 && is_hex_string(t) {
        return true;
    }

    false
}

/// Redact inline `keyword=value` and `keyword: value` patterns where the key suggests a secret.
///
/// Uses simple byte-level scanning for correctness — no regex dependency.
/// Input text must be ASCII or valid UTF-8 (guaranteed by Rust string type).
fn redact_kv_patterns(text: &str, redacted_count: &mut usize) -> String {
    let secret_keywords: &[&str] = &[
        "api_key", "apikey", "api-key",
        "token", "secret", "password", "passwd", "pwd",
        "auth", "credential", "bearer", "private_key", "access_key",
        "client_secret", "client_id",
    ];

    let mut result = text.to_string();

    for kw in secret_keywords {
        let mut output = String::with_capacity(result.len());
        let lower = result.to_lowercase();
        let bytes = lower.as_bytes();
        let kw_bytes = kw.as_bytes();
        let mut pos = 0;

        while pos < bytes.len() {
            // Find keyword starting at pos
            if let Some(rel) = bytes[pos..].windows(kw_bytes.len()).position(|w| w == kw_bytes) {
                let kw_start = pos + rel;
                let kw_end   = kw_start + kw_bytes.len();

                // Keyword must be at a word boundary (preceded by non-alphanumeric or start)
                let before_ok = kw_start == 0
                    || !bytes[kw_start - 1].is_ascii_alphanumeric() && bytes[kw_start - 1] != b'_';

                if !before_ok {
                    // Not a word boundary — copy up to kw_start+1 and continue
                    output.push_str(&result[pos..kw_start + 1]);
                    pos = kw_start + 1;
                    continue;
                }

                // Skip whitespace between keyword end and separator
                let mut sep_pos = kw_end;
                while sep_pos < bytes.len() && bytes[sep_pos] == b' ' {
                    sep_pos += 1;
                }

                // Expect separator: = or :
                if sep_pos < bytes.len() && (bytes[sep_pos] == b'=' || bytes[sep_pos] == b':') {
                    let val_start = sep_pos + 1;

                    // Skip whitespace after separator
                    let mut val_begin = val_start;
                    while val_begin < bytes.len() && bytes[val_begin] == b' ' {
                        val_begin += 1;
                    }

                    // Value ends at next whitespace or end of string
                    let val_end = bytes[val_begin..]
                        .iter()
                        .position(|&b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r')
                        .map(|p| val_begin + p)
                        .unwrap_or(bytes.len());

                    let value = &result[val_begin..val_end];

                    if value.len() >= 6 && !is_clearly_safe(value) {
                        // Redact: emit everything up to val_begin, then [REDACTED]
                        output.push_str(&result[pos..val_begin]);
                        output.push_str("[REDACTED]");
                        *redacted_count += 1;
                        pos = val_end;
                        continue;
                    }
                }

                // No redaction — emit up to kw_end and continue
                output.push_str(&result[pos..kw_end]);
                pos = kw_end;
            } else {
                // No more keywords — emit remainder
                output.push_str(&result[pos..]);
                pos = bytes.len();
            }
        }

        result = output;
    }

    result
}

/// Values that are clearly not secrets (common placeholders, short words).
fn is_clearly_safe(value: &str) -> bool {
    let lower = value.to_lowercase();
    matches!(lower.as_str(),
        "true" | "false" | "null" | "none" | "yes" | "no"
        | "enabled" | "disabled" | "on" | "off"
        | "0" | "1" | "localhost" | "default"
    )
}

// ─── Entropy ──────────────────────────────────────────────────────────────────

/// Shannon entropy in bits/char: H = -Σ p(c) log₂ p(c)
///
/// A truly random byte stream has H = 8 bits/char (uniform distribution).
/// Natural English text is ~4.0 bits/char (skewed distribution).
/// The threshold 4.5 bits/char is a well-established heuristic for credential detection
/// (see: Meli et al., "How Bad Can It Git?", IEEE EuroS&P 2019).
fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    let len = s.len();
    for &b in s.as_bytes() {
        freq[b as usize] += 1;
    }
    let mut h = 0.0f64;
    for &count in &freq {
        if count > 0 {
            let p = count as f64 / len as f64;
            h -= p * p.log2();
        }
    }
    h
}

/// Returns true if the string looks like base64: [A-Za-z0-9+/=] with no spaces.
fn is_base64_like(s: &str) -> bool {
    let valid: HashSet<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/="
        .chars().collect();
    s.chars().all(|c| valid.contains(&c))
}

/// Returns true if the string is entirely hex digits: [0-9a-fA-F].
fn is_hex_string(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_hexdigit())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- Shannon entropy -------------------------------------------------------

    #[test]
    fn entropy_natural_text_below_threshold() {
        let prose = "cargo build failed linker error missing symbol crate library";
        let h = shannon_entropy(prose);
        assert!(h < 4.5, "natural prose entropy {h:.2} should be below 4.5");
    }

    #[test]
    fn entropy_api_key_above_threshold() {
        // Typical API key format — high entropy
        let key = "sk-proj-xK9mNpQr4vW2LjZ8eHuT1cX7aBfYnM6dEoOkISw3PgV0RqDy";
        let h = shannon_entropy(key);
        assert!(h > 4.5, "API key entropy {h:.2} should be above 4.5");
    }

    // --- is_suspicious --------------------------------------------------------

    #[test]
    fn short_words_not_suspicious() {
        assert!(!is_suspicious("cargo"));
        assert!(!is_suspicious("true"));
        assert!(!is_suspicious("ok"));
    }

    #[test]
    fn long_random_string_suspicious() {
        let token = "A3kd92mXp0QwJnBv7rT1LcYeZsGhFiNoU";
        assert!(is_suspicious(token), "random token must be flagged");
    }

    #[test]
    fn hex_key_32_chars_suspicious() {
        let hex = "0a1b2c3d4e5f6789abcdef0123456789";
        assert!(is_suspicious(hex), "32-char hex must be flagged as potential key");
    }

    #[test]
    fn normal_identifier_not_suspicious() {
        assert!(!is_suspicious("ContextRuntime"));
        assert!(!is_suspicious("memory_learn_text"));
        assert!(!is_suspicious("unwrap_or_else"));
    }

    // --- scrub ----------------------------------------------------------------

    #[test]
    fn scrub_clean_text_unchanged() {
        let text = "cargo build failed with linker error";
        let result = scrub(text);
        assert!(!result.was_modified, "clean text must not be modified");
        assert_eq!(result.redacted_count, 0);
        assert_eq!(result.text, text);
    }

    #[test]
    fn scrub_removes_high_entropy_token() {
        let text = "api call failed with key sk-proj-xK9mNpQr4vW2LjZ8eHuT1cX7aBfYnM6d returned 401";
        let result = scrub(text);
        assert!(result.was_modified, "high-entropy token must be redacted");
        assert!(result.text.contains("[REDACTED]"), "must contain [REDACTED]");
        assert!(!result.text.contains("sk-proj"), "original key must be removed");
    }

    #[test]
    fn scrub_kv_pattern_api_key() {
        let text = "set api_key=sk-abc123XYZ987morerandombytes in config";
        let result = scrub(text);
        assert!(result.was_modified, "api_key=value pattern must be redacted");
        assert!(result.text.contains("[REDACTED]"));
    }

    #[test]
    fn scrub_kv_pattern_token_colon() {
        let text = "authorization: Bearer ghp_A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6";
        let result = scrub(text);
        // The bearer token is high-entropy
        assert!(result.was_modified);
    }

    #[test]
    fn scrub_preserves_useful_context() {
        let text = "build failed at src/main.rs line 42 error E0502 cannot borrow";
        let result = scrub(text);
        // All these tokens are low-entropy identifiers — must survive
        assert!(result.text.contains("src/main.rs") || result.text.contains("main"));
        assert!(result.text.contains("failed"));
        assert!(result.text.contains("error") || result.text.contains("E0502"));
    }

    #[test]
    fn scrub_three_fields_combined() {
        let (s, _a, o, total) = scrub_learn(
            "cargo build nvidia provider",
            "added provider.nvidia.models to opencode.jsonc",
            "model now appears in catalog token: ghp_abc123xyz789longtoken",
        );
        assert!(total > 0, "scrub_learn must detect at least one token (got total={total}, state={s:?}, outcome={o:?})");
        assert!(o.contains("[REDACTED]"), "outcome must have the token redacted; got: {o:?}");
        assert!(!o.contains("ghp_"), "original token prefix must not appear; got: {o:?}");
        // Useful context must survive
        assert!(o.contains("model") || o.contains("catalog"), "useful context must survive redaction");
    }

    #[test]
    fn scrub_partial_redaction_not_full_rejection() {
        // The scrubber must NOT reject the whole string — only redact the secret token
        let text = "build succeeded using sk-proj-reallyLongApiKeyValue123XYZ as credential";
        let result = scrub(text);
        assert!(result.was_modified);
        // Useful context must survive
        assert!(result.text.contains("build"));
        assert!(result.text.contains("succeeded"));
    }
}
