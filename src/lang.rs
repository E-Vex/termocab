//! A validated language identifier.
//!
//! Language names arrive as raw CLI arguments and flow straight into
//! paths like `languages/<lang>/stages.json` and `progress/<lang>.txt`.
//! Untyped, that is a path-traversal hole: `termocab add-lang ../escaped`
//! wrote outside the content directory (review item 4). The `LangId`
//! newtype closes it — it is constructed only through [`LangId::parse`],
//! which accepts `[a-z0-9_-]{1,32}`, and the path builders in `cli/`,
//! `content/` and `storage/` accept only a `&LangId`, never a raw
//! `&str`. This module is a leaf: it depends on nothing, so any layer
//! may use it without adding coupling between them.

use std::fmt;

/// A language id matching `[a-z0-9_-]{1,32}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LangId(String);

impl LangId {
    /// Accepts lowercase ascii letters, digits, `-` and `_`, 1-32 chars.
    /// Everything else — `../x`, `a/b`, `.`, ``, uppercase, non-ascii,
    /// over-long names — is rejected with a reason.
    pub fn parse(s: &str) -> Result<LangId, String> {
        if s.is_empty() {
            return Err("a language name must not be empty".into());
        }
        if let Some(bad) = s.chars().find(|c| !is_name_char(*c)) {
            return Err(format!(
                "a language name may only contain a-z, 0-9, '-' and '_' — {s:?} has {bad:?}"
            ));
        }
        // s is pure ASCII here, so bytes == characters.
        if s.len() > 32 {
            return Err(format!(
                "a language name is at most 32 characters long — {s:?} has {}",
                s.len()
            ));
        }
        Ok(LangId(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_name_char(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9' | '-' | '_')
}

impl fmt::Display for LangId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_reasonable_names() {
        for ok in ["french", "a", "es-419", "zh_yue", "x0-y9_z"] {
            let parsed = LangId::parse(ok);
            assert!(parsed.is_ok(), "{ok} should be accepted");
            assert_eq!(parsed.unwrap().as_str(), ok);
        }
    }

    #[test]
    fn rejects_traversal_and_friends() {
        for bad in ["..", "../x", "a/b", ".", "/", "french/", " ", "f rench"] {
            assert!(LangId::parse(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn rejects_uppercase_nonascii_and_long() {
        assert!(LangId::parse("French").is_err());
        assert!(LangId::parse("café").is_err());
        assert!(LangId::parse("").is_err());
        assert!(LangId::parse(&"x".repeat(32)).is_ok());
        assert!(LangId::parse(&"x".repeat(33)).is_err());
    }
}
