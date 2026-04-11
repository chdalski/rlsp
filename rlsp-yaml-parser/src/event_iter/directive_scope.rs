// SPDX-License-Identifier: MIT

use std::borrow::Cow;
use std::collections::HashMap;

use crate::error::Error;
use crate::limits::MAX_RESOLVED_TAG_LEN;
use crate::pos::Pos;

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

        // `!!suffix` — primary handle.
        if let Some(suffix) = after_first_bang.strip_prefix('!') {
            let prefix = self
                .tag_handles
                .get("!!")
                .map_or("tag:yaml.org,2002:", String::as_str);
            let resolved = format!("{prefix}{suffix}");
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
                let resolved = format!("{prefix}{suffix}");
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

        // `!suffix` (local tag) or bare `!` — no expansion.
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
