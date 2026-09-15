**Repository:** root
**Status:** Completed (2026-03-16)
**Created:** 2026-03-16
**Author:** Architect

## Goal

Add custom tag support to rlsp-yaml so that YAML files using
tags like `!include`, `!ref`, `!Ref`, etc. can opt in to those
tags and avoid unknown-tag warnings. Currently saphyr parses
tags silently (no errors), so the first step is adding a
validator that flags unknown tags, then providing two
configuration mechanisms (modeline and workspace settings) to
suppress those warnings for declared tags.

## Context

- saphyr 0.0.6 represents tags via `YamlOwned::Tagged(String, Box<YamlOwned>)`.
  Tags are parsed successfully — they never produce errors today.
- The validator module (`src/validators.rs`) already has three validators
  (`validate_unused_anchors`, `validate_flow_style`, `validate_key_ordering`)
  that are called from `Backend::parse_and_publish` in `server.rs`.
- Schema modeline parsing exists in `schema.rs::extract_schema_url` — scans
  first 10 lines for `# yaml-language-server: $schema=<url>`. The custom tags
  modeline will follow the same pattern.
- The TypeScript reference uses workspace settings (`customTags: string[]`)
  and validates tags against that allowlist.
- No workspace settings infrastructure exists in rlsp-yaml — the
  `InitializeParams` are currently ignored. We need to add
  `didChangeConfiguration` handling.
- The `!include` special handling (file path as document link) is deferred
  to a future task.

### Key files

- `src/validators.rs` — add `validate_custom_tags` function
- `src/server.rs` — wire up validator, add settings, handle `didChangeConfiguration`
- `src/schema.rs` — add `extract_custom_tags` modeline parser (adjacent to `extract_schema_url`)

### Patterns to follow

- Pure function design: `validate_custom_tags(text, docs, allowed_tags) -> Vec<Diagnostic>`
- Modeline scanning: same first-10-lines approach as `extract_schema_url`
- Diagnostic codes: string codes (e.g., `"unknownTag"`) matching existing conventions
  (`"flowMap"`, `"flowSeq"`, `"unusedAnchor"`)

## Steps

- [x] Clarify requirements with user
- [x] Analyze codebase for tag handling patterns
- [x] Task 1: Add `validate_custom_tags` validator (d6936c3)
- [x] Task 2: Add modeline parser and workspace settings for custom tags (b3d65e1)
- [x] Task 3: Wire everything into the server (5a860d8)

## Tasks

### Task 1: Add custom tag validator

Add a pure function `validate_custom_tags` in `src/validators.rs` that
walks parsed YAML documents and flags any tag not in the allowed set.

**What to implement:**
- `validate_custom_tags(text: &str, docs: &[YamlOwned], allowed_tags: &HashSet<String>) -> Vec<Diagnostic>`
- Walk documents recursively, find all `YamlOwned::Tagged(tag, _)` nodes
- For each tag not in `allowed_tags`, produce a warning diagnostic with:
  - Code: `"unknownTag"`
  - Severity: Warning
  - Message: `"Unknown tag: !tagname"`
  - Range: the line/column of the tag in the source text (scan `text` for `!tagname`)
- When `allowed_tags` is empty, skip validation entirely (no tags configured = no warnings)
- Tests: unknown tag produces warning, allowed tag produces none, empty allowed set
  produces no diagnostics, multiple tags in one document, tags in multi-doc YAML

**Files:** `src/validators.rs`

### Task 2: Add modeline parser and workspace settings

Add two configuration sources for custom tags:

**A) Modeline parser** — add `extract_custom_tags(text: &str) -> Vec<String>` in `src/schema.rs`
(adjacent to `extract_schema_url`):
- Scans first 10 lines for `# yaml-language-server: $tags=!include,!ref`
- Comma-separated tag names after `$tags=`
- Each tag should start with `!` — strip or normalize as needed
- Returns empty vec if no modeline found
- Tests: parse single tag, multiple tags, whitespace handling, missing modeline,
  tags on line beyond 10

**B) Workspace settings** — add settings infrastructure to `src/server.rs`:
- Define a `Settings` struct: `custom_tags: Vec<String>`
- Deserialize from `InitializeParams.initialization_options` (serde_json)
- Handle `didChangeConfiguration` notification to update settings at runtime
- Store settings in `Backend` behind a `Mutex<Settings>`
- Tests: deserialize settings from JSON, handle missing field with defaults

**Files:** `src/schema.rs` (modeline), `src/server.rs` (settings)

### Task 3: Wire into server

Connect the validator and configuration sources in `Backend::parse_and_publish`:

- Merge custom tags from three sources (priority: modeline > settings):
  1. Modeline `$tags=...` from the document text
  2. Workspace settings `custom_tags` from initialization/configuration
- Build a `HashSet<String>` of allowed tags
- Call `validate_custom_tags(text, &result.documents, &allowed_tags)` and
  extend diagnostics
- Tests: integration test showing unknown tag produces diagnostic,
  modeline suppresses it, settings suppress it

**Files:** `src/server.rs`

## Decisions

- **Modeline syntax:** `# yaml-language-server: $tags=!include,!ref` — follows
  the existing `$schema=` convention from the same modeline prefix.
- **Empty allowed set = no validation:** When no custom tags are configured
  (no modeline, no settings), the validator produces no diagnostics. This
  avoids breaking existing users who use tags without configuration.
- **Warning severity:** Unknown tags are warnings, not errors — they don't
  prevent YAML from being valid.
- **Modeline priority:** Modeline tags override (merge with) workspace settings,
  giving per-file control on top of workspace defaults.
- **`!include` link handling:** Deferred to a future task per user request.
