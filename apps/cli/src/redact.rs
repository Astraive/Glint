//! Redaction of diagnostics (CLI-003).
//!
//! `glint doctor` and `glint status` redact by default. Two layers:
//!
//! 1. **Not collected.** Diagnostics never read SSIDs, window titles,
//!    access tokens, environment variables beyond the few session hints
//!    listed in `docs/cli/reference.md`, or package contents. There is
//!    nothing to leak.
//! 2. **Replaced.** Every string of the finished report (JSON values and the
//!    human text) has the user-identifying values replaced by stable
//!    placeholders, longest first: the Glint roots (`<glint-config>`,
//!    `<glint-data>`, `<glint-cache>`, `<glint-home>`), the user's home
//!    directory (`<home>`), the user name (`<user>`) and the computer name
//!    (`<host>`). Paths match case-insensitively and with either slash;
//!    values shorter than three characters are not replaced (they would
//!    mangle unrelated text) and are listed as not redacted instead.
//!
//! `--include-sensitive` (confirmed) skips layer 2 only.

use std::path::Path;

use glint_storage::GlintPaths;
use serde_json::Value;

/// The shortest value that is replaced.
const MIN_LEN: usize = 3;

/// Replaces user-identifying substrings with placeholders.
#[derive(Debug, Default)]
pub struct Redactor {
    /// `(needle lowercased, placeholder)`, longest needle first.
    rules: Vec<(String, &'static str)>,
    /// Kinds of value that were too short to replace safely.
    skipped: Vec<&'static str>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

impl Redactor {
    /// A redactor for this user and these Glint paths.
    pub fn for_user(paths: Option<&GlintPaths>) -> Self {
        let mut redactor = Self::default();
        if let Some(paths) = paths {
            redactor.add_path(paths.config_dir(), "<glint-config>");
            redactor.add_path(paths.data_dir(), "<glint-data>");
            redactor.add_path(paths.cache_dir(), "<glint-cache>");
        }
        if let Some(home) = env(glint_storage::HOME_OVERRIDE_ENV) {
            redactor.add_path(Path::new(&home), "<glint-home>");
        }
        for name in ["USERPROFILE", "HOME"] {
            if let Some(home) = env(name) {
                redactor.add_path(Path::new(&home), "<home>");
            }
        }
        for name in ["USERNAME", "USER", "LOGNAME"] {
            if let Some(user) = env(name) {
                redactor.add("user name", &user, "<user>");
            }
        }
        for name in ["COMPUTERNAME", "HOSTNAME"] {
            if let Some(host) = env(name) {
                redactor.add("computer name", &host, "<host>");
            }
        }
        redactor
            .rules
            .sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        redactor.rules.dedup_by(|a, b| a.0 == b.0);
        redactor
    }

    fn add_path(&mut self, path: &Path, placeholder: &'static str) {
        let text = path.to_string_lossy();
        let trimmed = text.trim_end_matches(['/', '\\']);
        self.add("path", trimmed, placeholder);
        self.add("path", &trimmed.replace('\\', "/"), placeholder);
        self.add("path", &trimmed.replace('/', "\\"), placeholder);
        // `\\?\C:\...` verbatim forms of the same path.
        if let Some(rest) = trimmed.strip_prefix(r"\\?\") {
            self.add("path", rest, placeholder);
        } else if cfg!(windows) && trimmed.as_bytes().get(1) == Some(&b':') {
            self.add("path", &format!(r"\\?\{trimmed}"), placeholder);
        }
    }

    fn add(&mut self, kind: &'static str, value: &str, placeholder: &'static str) {
        if value.chars().count() < MIN_LEN {
            if !self.skipped.contains(&kind) {
                self.skipped.push(kind);
            }
            return;
        }
        self.rules.push((value.to_lowercase(), placeholder));
    }

    /// Kinds of value left unredacted because they were too short.
    pub fn skipped(&self) -> &[&'static str] {
        &self.skipped
    }

    /// `text` with every rule applied (case-insensitive).
    pub fn text(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for (needle, placeholder) in &self.rules {
            out = replace_ignore_case(&out, needle, placeholder);
        }
        out
    }

    /// `value` with every string (and object key) redacted.
    pub fn value(&self, value: Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            Value::Array(items) => Value::Array(items.into_iter().map(|v| self.value(v)).collect()),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, v)| (self.text(&key), self.value(v)))
                    .collect(),
            ),
            other => other,
        }
    }
}

/// Replace every case-insensitive occurrence of `needle` (already
/// lowercase) in `haystack`.
fn replace_ignore_case(haystack: &str, needle: &str, placeholder: &str) -> String {
    if needle.is_empty() {
        return haystack.to_owned();
    }
    let lower = haystack.to_lowercase();
    // Lowercasing can change byte lengths outside ASCII; fall back to an
    // exact match then.
    if lower.len() != haystack.len() {
        return haystack.replace(needle, placeholder);
    }
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0;
    for (start, _) in lower.match_indices(needle) {
        if start < last {
            continue;
        }
        out.push_str(&haystack[last..start]);
        out.push_str(placeholder);
        last = start + needle.len();
    }
    out.push_str(&haystack[last..]);
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Redactor, replace_ignore_case};

    fn redactor(rules: &[(&str, &'static str)]) -> Redactor {
        let mut redactor = Redactor::default();
        for (needle, placeholder) in rules {
            redactor.add("test", needle, placeholder);
        }
        redactor
            .rules
            .sort_by_key(|rule| std::cmp::Reverse(rule.0.len()));
        redactor
    }

    #[test]
    fn longest_rule_wins_and_case_is_ignored() {
        let r = redactor(&[
            (r"C:\Users\Alice", "<home>"),
            (r"C:\Users\Alice\AppData\Local\Glint\data", "<glint-data>"),
            ("alice", "<user>"),
        ]);
        assert_eq!(
            r.text(r"db at c:\users\alice\appdata\local\glint\data\glint.db by Alice"),
            r"db at <glint-data>\glint.db by <user>"
        );
        assert_eq!(r.text(r"C:\Users\ALICE\x"), r"<home>\x");
    }

    #[test]
    fn values_are_redacted_recursively_and_short_values_are_reported() {
        let mut r = redactor(&[("secret-home", "<home>")]);
        r.add("user name", "ab", "<user>");
        assert_eq!(r.skipped(), ["user name"]);
        let value = r.value(json!({"a": ["x secret-home y"], "secret-home": 1, "n": 2}));
        assert_eq!(value, json!({"a": ["x <home> y"], "<home>": 1, "n": 2}));
    }

    #[test]
    fn non_ascii_text_falls_back_to_exact_matching() {
        assert_eq!(replace_ignore_case("İx home", "home", "<h>"), "İx <h>");
    }
}
