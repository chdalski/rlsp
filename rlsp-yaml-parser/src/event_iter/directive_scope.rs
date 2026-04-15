// SPDX-License-Identifier: MIT

use std::borrow::Cow;
use std::collections::HashMap;

use crate::error::Error;
use crate::limits::MAX_RESOLVED_TAG_LEN;
use crate::pos::Pos;

/// Percent-decode a tag suffix per YAML 1.2 §6.8.1.
///
/// Only `%XX` sequences where both hex digits are valid are decoded.
/// Invalid sequences (non-hex digits, truncated `%`) are passed through
/// unchanged — the caller has already validated the tag token.
fn percent_decode(s: &str) -> Cow<'_, str> {
    if !s.contains('%') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push(char::from(((h << 4) | l) as u8));
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Cow::Owned(out)
}

/// Per-document directive state accumulated from `%YAML` and `%TAG` directives.
///
/// Cleared at the start of each new document (on `---` in `BetweenDocs`, on
/// `...`, or at EOF).  The default handles (`!!` and `!`) are **not** stored
/// here — they are resolved directly in [`DirectiveScope::resolve_tag`].
#[derive(Debug, Default)]
pub struct DirectiveScope {
    /// Version from `%YAML`, if any.
    pub(in crate::event_iter) version: Option<(u8, u8)>,
    /// Custom tag handles declared via `%TAG` directives.
    ///
    /// Key: handle (e.g. `"!foo!"`).  Value: prefix (e.g. `"tag:example.com:"`).
    pub(in crate::event_iter) tag_handles: HashMap<String, String>,
    /// Total directive count (YAML + TAG combined) for the `DoS` limit check.
    pub(in crate::event_iter) directive_count: usize,
}

impl DirectiveScope {
    /// Resolve a raw tag slice (as stored in `pending_tag`) to its final form.
    ///
    /// Resolution rules:
    /// - Verbatim tag (no leading `!`, i.e. already a bare URI from `!<URI>` scanning) → returned as-is.
    /// - `!!suffix` → look up `"!!"` in custom handles; fall back to default `tag:yaml.org,2002:`.
    /// - `!suffix` (no inner `!`) → returned as-is (local tag, no expansion).
    /// - `!handle!suffix` → look up `"!handle!"` in custom handles; error if not found.
    /// - `!` (bare) → returned as-is.
    ///
    /// Returns `Ok(Cow::Borrowed(raw))` when no allocation is needed, or
    /// `Ok(Cow::Owned(resolved))` after prefix expansion.  Returns `Err` when
    /// a named handle has no registered prefix.
    pub(in crate::event_iter) fn resolve_tag<'a>(
        &self,
        raw: &'a str,
        indicator_pos: Pos,
    ) -> Result<Cow<'a, str>, Error> {
        // Verbatim tags arrive as bare URIs (scan_tag strips the `!<` / `>` wrappers).
        // They do not start with `!`, so no resolution is needed.
        if !raw.starts_with('!') {
            return Ok(Cow::Borrowed(raw));
        }

        let after_first_bang = &raw[1..];

        // `!!suffix` — secondary handle.
        if let Some(suffix) = after_first_bang.strip_prefix('!') {
            let prefix = self
                .tag_handles
                .get("!!")
                .map_or("tag:yaml.org,2002:", String::as_str);
            let decoded_suffix = percent_decode(suffix);
            let resolved = format!("{prefix}{decoded_suffix}");
            if resolved.len() > MAX_RESOLVED_TAG_LEN {
                return Err(Error {
                    pos: indicator_pos,
                    message: format!(
                        "resolved tag exceeds maximum length of {MAX_RESOLVED_TAG_LEN} bytes"
                    ),
                });
            }
            return Ok(Cow::Owned(resolved));
        }

        // `!handle!suffix` — named handle.
        if let Some(inner_bang) = after_first_bang.find('!') {
            let handle = &raw[..inner_bang + 2]; // `!handle!`
            let suffix = &after_first_bang[inner_bang + 1..];
            if let Some(prefix) = self.tag_handles.get(handle) {
                let decoded_suffix = percent_decode(suffix);
                let resolved = format!("{prefix}{decoded_suffix}");
                if resolved.len() > MAX_RESOLVED_TAG_LEN {
                    return Err(Error {
                        pos: indicator_pos,
                        message: format!(
                            "resolved tag exceeds maximum length of {MAX_RESOLVED_TAG_LEN} bytes"
                        ),
                    });
                }
                return Ok(Cow::Owned(resolved));
            }
            return Err(Error {
                pos: indicator_pos,
                message: format!("undefined tag handle: {handle}"),
            });
        }

        // `!suffix` — check for a registered primary `!` handle (set via `%TAG ! prefix`).
        // If present, expand; otherwise return as local tag.  A bare `!` (after_first_bang
        // is empty) is a non-specific tag and is never expanded.
        if !after_first_bang.is_empty() {
            if let Some(prefix) = self.tag_handles.get("!") {
                let decoded_suffix = percent_decode(after_first_bang);
                let resolved = format!("{prefix}{decoded_suffix}");
                if resolved.len() > MAX_RESOLVED_TAG_LEN {
                    return Err(Error {
                        pos: indicator_pos,
                        message: format!(
                            "resolved tag exceeds maximum length of {MAX_RESOLVED_TAG_LEN} bytes"
                        ),
                    });
                }
                return Ok(Cow::Owned(resolved));
            }
        }

        // `!suffix` with no registered `!` handle (local tag) or bare `!` — no expansion.
        Ok(Cow::Borrowed(raw))
    }

    /// Collect the tag handle/prefix pairs for inclusion in `DocumentStart`.
    pub(in crate::event_iter) fn tag_directives(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .tag_handles
            .iter()
            .map(|(h, p)| (h.clone(), p.clone()))
            .collect();
        // Sort for deterministic ordering in tests and events.
        pairs.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        pairs
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::field_reassign_with_default,
    reason = "test code"
)]
mod tests {
    use super::*;
    use crate::limits::MAX_RESOLVED_TAG_LEN;
    use crate::pos::Pos;

    const POS: Pos = Pos::ORIGIN;

    // -----------------------------------------------------------------------
    // resolve_tag — verbatim tags (no leading `!`)
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_tag_verbatim_returns_input_as_is() {
        let scope = DirectiveScope::default();
        let result = scope.resolve_tag("tag:yaml.org,2002:str", POS).unwrap();
        assert_eq!(result, "tag:yaml.org,2002:str");
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn resolve_tag_verbatim_empty_string() {
        let scope = DirectiveScope::default();
        let result = scope.resolve_tag("", POS).unwrap();
        assert_eq!(result, "");
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
    }

    // -----------------------------------------------------------------------
    // resolve_tag — secondary handle `!!`
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_tag_double_bang_uses_default_yaml_prefix() {
        let scope = DirectiveScope::default();
        assert_eq!(
            scope.resolve_tag("!!str", POS).unwrap(),
            "tag:yaml.org,2002:str"
        );
    }

    #[test]
    fn resolve_tag_double_bang_empty_suffix() {
        let scope = DirectiveScope::default();
        assert_eq!(scope.resolve_tag("!!", POS).unwrap(), "tag:yaml.org,2002:");
    }

    #[test]
    fn resolve_tag_double_bang_uses_custom_prefix_when_registered() {
        let mut scope = DirectiveScope::default();
        scope
            .tag_handles
            .insert("!!".to_string(), "tag:example.com:".to_string());
        assert_eq!(
            scope.resolve_tag("!!local", POS).unwrap(),
            "tag:example.com:local"
        );
    }

    // -----------------------------------------------------------------------
    // resolve_tag — named handles `!handle!`
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_tag_named_handle_expands_to_registered_prefix() {
        let mut scope = DirectiveScope::default();
        scope
            .tag_handles
            .insert("!foo!".to_string(), "tag:example.com:foo:".to_string());
        assert_eq!(
            scope.resolve_tag("!foo!bar", POS).unwrap(),
            "tag:example.com:foo:bar"
        );
    }

    #[test]
    fn resolve_tag_named_handle_empty_suffix() {
        let mut scope = DirectiveScope::default();
        scope
            .tag_handles
            .insert("!foo!".to_string(), "tag:example.com:foo:".to_string());
        assert_eq!(
            scope.resolve_tag("!foo!", POS).unwrap(),
            "tag:example.com:foo:"
        );
    }

    #[test]
    fn resolve_tag_named_handle_unknown_errors() {
        let scope = DirectiveScope::default();
        let err = scope.resolve_tag("!bar!baz", POS).unwrap_err();
        assert!(err.message.contains("undefined tag handle"));
        assert!(err.message.contains("!bar!"));
    }

    #[test]
    fn resolve_tag_named_handle_multiple_handles_resolves_correct_one() {
        let mut scope = DirectiveScope::default();
        scope
            .tag_handles
            .insert("!a!".to_string(), "tag:a.com:".to_string());
        scope
            .tag_handles
            .insert("!b!".to_string(), "tag:b.com:".to_string());
        assert_eq!(scope.resolve_tag("!b!val", POS).unwrap(), "tag:b.com:val");
    }

    // -----------------------------------------------------------------------
    // resolve_tag — primary handle `!` (local tags)
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_tag_local_tag_returns_as_is() {
        let scope = DirectiveScope::default();
        let result = scope.resolve_tag("!local", POS).unwrap();
        assert_eq!(result, "!local");
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn resolve_tag_bare_bang_returns_as_is() {
        let scope = DirectiveScope::default();
        let result = scope.resolve_tag("!", POS).unwrap();
        assert_eq!(result, "!");
        assert!(matches!(result, std::borrow::Cow::Borrowed(_)));
    }

    // -----------------------------------------------------------------------
    // resolve_tag — MAX_RESOLVED_TAG_LEN boundary
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_tag_double_bang_at_exact_max_length_succeeds() {
        let scope = DirectiveScope::default();
        let prefix = "tag:yaml.org,2002:";
        let suffix = "a".repeat(MAX_RESOLVED_TAG_LEN - prefix.len());
        let raw = format!("!!{suffix}");
        let result = scope.resolve_tag(&raw, POS).unwrap();
        assert_eq!(result.len(), MAX_RESOLVED_TAG_LEN);
    }

    #[test]
    fn resolve_tag_double_bang_one_byte_over_max_length_errors() {
        let scope = DirectiveScope::default();
        let prefix = "tag:yaml.org,2002:";
        let suffix = "a".repeat(MAX_RESOLVED_TAG_LEN - prefix.len() + 1);
        let raw = format!("!!{suffix}");
        let err = scope.resolve_tag(&raw, POS).unwrap_err();
        assert!(err.message.contains("exceeds maximum length"));
    }

    #[test]
    fn resolve_tag_named_handle_at_exact_max_length_succeeds() {
        let mut scope = DirectiveScope::default();
        let prefix = "tag:x.com:";
        scope
            .tag_handles
            .insert("!x!".to_string(), prefix.to_string());
        let suffix = "a".repeat(MAX_RESOLVED_TAG_LEN - prefix.len());
        let raw = format!("!x!{suffix}");
        let result = scope.resolve_tag(&raw, POS).unwrap();
        assert_eq!(result.len(), MAX_RESOLVED_TAG_LEN);
    }

    #[test]
    fn resolve_tag_named_handle_one_byte_over_max_length_errors() {
        let mut scope = DirectiveScope::default();
        let prefix = "tag:x.com:";
        scope
            .tag_handles
            .insert("!x!".to_string(), prefix.to_string());
        let suffix = "a".repeat(MAX_RESOLVED_TAG_LEN - prefix.len() + 1);
        let raw = format!("!x!{suffix}");
        let err = scope.resolve_tag(&raw, POS).unwrap_err();
        assert!(err.message.contains("exceeds maximum length"));
    }

    // -----------------------------------------------------------------------
    // tag_directives
    // -----------------------------------------------------------------------

    #[test]
    fn tag_directives_empty_when_no_handles_registered() {
        let scope = DirectiveScope::default();
        assert_eq!(scope.tag_directives(), vec![]);
    }

    #[test]
    fn tag_directives_returns_single_registered_handle() {
        let mut scope = DirectiveScope::default();
        scope
            .tag_handles
            .insert("!foo!".to_string(), "tag:foo.com:".to_string());
        assert_eq!(
            scope.tag_directives(),
            vec![("!foo!".to_string(), "tag:foo.com:".to_string())]
        );
    }

    #[test]
    fn tag_directives_returns_multiple_handles_sorted() {
        let mut scope = DirectiveScope::default();
        scope
            .tag_handles
            .insert("!z!".to_string(), "z:".to_string());
        scope
            .tag_handles
            .insert("!a!".to_string(), "a:".to_string());
        assert_eq!(
            scope.tag_directives(),
            vec![
                ("!a!".to_string(), "a:".to_string()),
                ("!z!".to_string(), "z:".to_string()),
            ]
        );
    }

    // -----------------------------------------------------------------------
    // Scope lifecycle
    // -----------------------------------------------------------------------

    #[test]
    fn fresh_scope_has_no_version() {
        let scope = DirectiveScope::default();
        assert_eq!(scope.version, None);
    }

    #[test]
    fn fresh_scope_resolves_double_bang_with_default_prefix() {
        let scope = DirectiveScope::default();
        assert_eq!(
            scope.resolve_tag("!!str", POS).unwrap(),
            "tag:yaml.org,2002:str"
        );
    }

    #[test]
    fn registered_handle_is_resolved_after_direct_write() {
        let mut scope = DirectiveScope::default();
        scope
            .tag_handles
            .insert("!ns!".to_string(), "tag:ns.example.com:".to_string());
        scope.directive_count = 1;
        assert_eq!(
            scope.resolve_tag("!ns!item", POS).unwrap(),
            "tag:ns.example.com:item"
        );
    }

    #[test]
    fn scope_reset_clears_handles() {
        let mut scope = DirectiveScope::default();
        scope
            .tag_handles
            .insert("!ns!".to_string(), "tag:ns.example.com:".to_string());
        let reset_scope = DirectiveScope::default();
        let err = reset_scope.resolve_tag("!ns!item", POS).unwrap_err();
        assert!(err.message.contains("undefined tag handle"));
    }

    #[test]
    fn scope_reset_clears_version() {
        let mut scope = DirectiveScope::default();
        scope.version = Some((1, 2));
        let reset_scope = DirectiveScope::default();
        assert_eq!(reset_scope.version, None);
    }
}
