**Repository:** root
**Status:** InProgress
**Created:** 2026-04-04

## Goal

Add feature toggle settings (enable/disable hover, completion, validation
individually) and a `maxItemsComputed` performance limit. These give users
granular control over language server behavior and protect against
performance degradation on very large files.

## Context

- RedHat's yaml-language-server supports per-feature toggles (`yaml.hover`,
  `yaml.completion`, `yaml.validate`) and `yaml.maxItemsComputed`
- Our Settings struct in `server.rs` (line 33) currently has no per-feature
  toggles — all features are always enabled
- Document symbols are computed in `src/symbols.rs`, folding ranges in
  `src/folding.rs` — these could be expensive on large files
- Completions in `src/completion.rs`, hover in `src/hover.rs`, validation
  in `server.rs::parse_and_publish()`

### Key files

- `rlsp-yaml/src/server.rs` — Settings struct, LSP method handlers
- `rlsp-yaml/src/symbols.rs` — document symbols
- `rlsp-yaml/src/folding.rs` — folding ranges
- `rlsp-yaml/src/completion.rs` — completions
- `rlsp-yaml/src/hover.rs` — hover
- `rlsp-yaml/docs/configuration.md` — settings documentation

## Steps

- [x] Add feature toggle settings (9ce8e80)
- [ ] Add maxItemsComputed setting
- [ ] Integrate toggles with LSP handlers
- [ ] Integrate maxItemsComputed with symbols and folding
- [ ] Add tests
- [ ] Update documentation

## Tasks

### Task 1: Add feature toggle settings (9ce8e80)

Add boolean settings to enable/disable individual LSP features.

- [x] Add `validate: Option<bool>` to Settings (default: true)
- [x] Add `hover: Option<bool>` to Settings (default: true)
- [x] Add `completion: Option<bool>` to Settings (default: true)
- [x] In `parse_and_publish()`: skip validation when `validate` is false
- [x] In `hover()` handler: return empty when `hover` is false
- [x] In `completion()` handler: return empty when `completion` is false
- [x] Tests for each toggle

### Task 2: Add maxItemsComputed setting

Add a performance limit for document symbols and folding ranges.

- [ ] Add `max_items_computed: Option<usize>` to Settings (default: 5000)
- [ ] In document symbols handler: truncate results at limit
- [ ] In folding ranges handler: truncate results at limit
- [ ] Consider: should the limit apply to other computed results?
      (completions, references, etc.)
- [ ] Tests for truncation behavior

### Task 3: Update documentation

- [ ] Add all new settings to `docs/configuration.md`
- [ ] Document default values and behavior when disabled

## Decisions

- **All features enabled by default** — toggling off is the exception, not
  the norm. Default behavior matches current (no breaking change).
- **maxItemsComputed defaults to 5000** — matches RedHat's default. Large
  enough for most files, small enough to prevent slowdown on huge manifests.
- **Validation toggle skips all validation** — including schema validation,
  not just syntax. When disabled, no diagnostics are published.
