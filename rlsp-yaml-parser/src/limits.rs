// SPDX-License-Identifier: MIT

/// Maximum combined block-collection nesting depth accepted from untrusted
/// input.
///
/// This limit covers all open [`crate::Event::SequenceStart`] and
/// [`crate::Event::MappingStart`] events combined.  Using a unified limit prevents
/// an attacker from nesting 512 sequences inside 512 mappings (total depth
/// 1024) by exploiting separate per-type limits.
///
/// 512 is generous for all real-world YAML (Kubernetes / Helm documents are
/// typically under 20 levels deep) and small enough that the explicit-stack
/// overhead stays within a few KB.
pub const MAX_COLLECTION_DEPTH: usize = 512;

/// Maximum byte length of an anchor name accepted from untrusted input.
///
/// Maximum byte length of an anchor or alias name.
///
/// The YAML spec places no upper limit on anchor names, but scanning a name
/// consisting of millions of valid `ns-anchor-char` bytes would exhaust CPU
/// time without any heap allocation.  This limit caps anchor and alias name
/// scanning at 1 KiB — generous for all real-world YAML (Kubernetes names are
/// typically under 64 bytes) while preventing degenerate-input stalls.
///
/// The limit is enforced by [`crate::parse_events`] for both `&name` (anchors) and
/// `*name` (aliases).  Exceeding it returns an [`crate::Error`], not a panic.
pub const MAX_ANCHOR_NAME_BYTES: usize = 1024;

/// Maximum byte length of a tag accepted from untrusted input.
///
/// The YAML spec places no upper limit on tag length, but scanning a tag
/// consisting of millions of valid bytes would exhaust CPU time without any
/// heap allocation.  This limit caps tag scanning at 4 KiB — generous for all
/// real-world YAML (standard tags like `tag:yaml.org,2002:str` are under 30
/// bytes; custom namespace URIs are rarely over 200 bytes) while preventing
/// degenerate-input stalls.
///
/// The limit applies to the raw scanned portion: the URI content between `<`
/// and `>` for verbatim tags, or the suffix portion for shorthand tags.
/// Exceeding it returns an [`crate::Error`], not a panic.
pub const MAX_TAG_LEN: usize = 4096;

/// Maximum byte length of a comment body accepted from untrusted input.
///
/// The YAML spec places no upper limit on comment length.  With zero-copy
/// `&'input str` slices, comment scanning itself allocates nothing, but
/// character-by-character iteration over a very long comment line still burns
/// CPU proportional to the line length.  This limit matches `MAX_TAG_LEN` —
/// comment-only files produce one `Comment` event per line (O(input size),
/// acceptable) as long as individual lines are bounded.
///
/// Exceeding this limit returns an [`crate::Error`], not a panic or truncation.
pub const MAX_COMMENT_LEN: usize = 4096;

/// Maximum number of directives (`%YAML` + `%TAG` combined) per document.
///
/// Without this cap, an attacker could supply thousands of distinct `%TAG`
/// directives, each allocating a `HashMap` entry, to exhaust heap memory.
/// 64 is generous for all real-world YAML (the typical document has 0–2
/// directives) while bounding per-document directive overhead.
///
/// Exceeding this limit returns an [`crate::Error`], not a panic.
pub const MAX_DIRECTIVES_PER_DOC: usize = 64;

/// Maximum byte length of a `%TAG` handle (e.g. `!foo!`) accepted from
/// untrusted input.
///
/// Tag handles are short by design; a 256-byte cap is generous while
/// preventing `DoS` via scanning very long handle strings.
///
/// Exceeding this limit returns an [`crate::Error`], not a panic.
pub const MAX_TAG_HANDLE_BYTES: usize = 256;

/// Maximum byte length of the fully-resolved tag string after prefix expansion.
///
/// When a shorthand tag `!foo!bar` is resolved against its `%TAG` prefix, the
/// result is `prefix + suffix`.  This cap prevents the resolved string from
/// exceeding a safe bound even when the prefix and suffix are both at their
/// individual limits.  Reuses [`MAX_TAG_LEN`] so the bound is consistent with
/// verbatim tag limits.
///
/// The check is performed before allocation; exceeding this limit returns an
/// [`crate::Error`], not a panic.
pub const MAX_RESOLVED_TAG_LEN: usize = MAX_TAG_LEN;
