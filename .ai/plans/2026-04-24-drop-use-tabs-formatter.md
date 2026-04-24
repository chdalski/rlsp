**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-24

## Goal

Remove the `use_tabs` option from rlsp-yaml's YAML formatter because it violates YAML 1.2 §6.1, which forbids tab characters for indentation. The formatter currently emits tab-indented output when `use_tabs: true`, but the project's own parser rejects that output — a broken round-trip. RedHat yaml-language-server and Prettier both ignore `insertSpaces: false` for YAML; rlsp-yaml is the ecosystem outlier. After this change, the formatter always uses space indentation for YAML, and `insertSpaces: false` from the LSP client is silently ignored.

## Context

- **YAML 1.2 §6.1** forbids tab characters for indentation: "YAML recognizes the following ASCII line break characters: [...] Tab characters are used for presentation purposes. [...] Tab characters must not be used in indentation." The parser enforces this in `event_iter/step.rs:38-62`.
- **rlsp-fmt retains its `use_tabs`** — it is a generic Wadler-Lindig pretty-printer, not YAML-specific. The removal is at the YAML server layer only; rlsp-yaml pins `use_tabs: false` when constructing `rlsp_fmt::FormatOptions`.
- **`tab_to_spaces` code action** is unrelated — it is a text-replacement action for existing tab characters in source, not a formatting option. It is preserved.
- **Files involved:**
  - `rlsp-yaml/src/editing/formatter.rs` — `YamlFormatOptions` struct (line 321), default (line 344), two `FormatOptions` constructions (lines 375, 424)
  - `rlsp-yaml/src/server.rs` — `use_tabs: !insert_spaces` mapping at lines 1060 and 1148
  - `rlsp-yaml/tests/fixtures/formatter/interact-use-tabs-tab-width.md` — the only fixture testing `use_tabs: true`
  - `rlsp-yaml/tests/formatter_fixtures.rs` — fixture parser arm (lines 198-200), default assertion (line 348)
  - `rlsp-yaml/docs/configuration.md` — line 362 mentions `useTabs` in the indentation note; line 496 mentions "tabs vs spaces" behavior
  - `rlsp-yaml/docs/feature-log.md` — needs a new entry documenting the removal

## Non-Goals

- Modifying `rlsp-fmt`'s `FormatOptions.use_tabs` — it is a generic pretty-printer option retained for non-YAML consumers.
- Modifying the `tab_to_spaces` code action — it is an independent text-replacement feature.
- Adding `formatIndentSequences` or other new formatter settings — those are separate follow-up items.

## Steps

- [ ] Remove `use_tabs` from `YamlFormatOptions` and pin `use_tabs: false` in `FormatOptions` constructions
- [ ] Remove `insertSpaces → use_tabs` mapping from server.rs
- [ ] Delete `interact-use-tabs-tab-width.md` fixture
- [ ] Update fixture parser and default assertion in `formatter_fixtures.rs`
- [ ] Update `configuration.md` to document that `insertSpaces: false` is ignored for YAML
- [ ] Add `feature-log.md` entry documenting the removal with §6.1 rationale
- [ ] Update follow-up queue to remove the `use_tabs` item

## Tasks

### Task 1: Remove `use_tabs` from formatter, server, fixture, and docs

Remove the `use_tabs` field from `YamlFormatOptions`, update the server mapping to stop passing `insertSpaces` through, delete the interaction fixture, update the fixture test harness, update docs, and add a feature-log entry.

- [ ] Remove `pub use_tabs: bool` field and its doc comment from `YamlFormatOptions` in `formatter.rs:321`
- [ ] Remove `use_tabs: false` from the `Default` impl in `formatter.rs:344`
- [ ] Change `use_tabs: options.use_tabs` to `use_tabs: false` in both `FormatOptions` constructions (`formatter.rs:375` and `formatter.rs:424`)
- [ ] Remove `use_tabs: !insert_spaces,` lines from `server.rs:1060` and `server.rs:1148`
- [ ] Delete fixture file `rlsp-yaml/tests/fixtures/formatter/interact-use-tabs-tab-width.md`
- [ ] Remove the `"use_tabs"` match arm from the fixture parser in `formatter_fixtures.rs:198-200`
- [ ] Remove the `assert_eq!(opts.use_tabs, default.use_tabs)` line from `formatter_fixtures.rs:348`
- [ ] Update `configuration.md:362` — change the indentation note to explain that `insertSpaces: false` is silently ignored for YAML because YAML 1.2 §6.1 forbids tab indentation
- [ ] Update `configuration.md:496` — remove "tabs vs spaces" from the indentation description, replacing with a note that the formatter always uses spaces
- [ ] Add entry to `rlsp-yaml/docs/feature-log.md`: "Drop `use_tabs` formatter option" documenting the removal with §6.1 and ecosystem rationale (RedHat + Prettier both ignore `insertSpaces: false`)
- [ ] Remove the "Drop `use_tabs`..." bullet from `.ai/memory/project_followup_plans.md`
- [ ] Remove the "Formatter fixture gaps" note about excluding `use_tabs` pairs from `.ai/memory/project_followup_plans.md` (update the bullet to remove the `use_tabs` exclusion clause)
- [ ] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` all pass with zero warnings/failures

## Decisions

- **Single task** — all changes are tightly coupled (removing a struct field cascades to every consumer) and cannot be committed independently without compilation errors.
- **Pin `use_tabs: false` rather than removing the field from `FormatOptions` construction** — `rlsp_fmt::FormatOptions` still has the field; rlsp-yaml just always passes `false`.
- **Silent ignore, not error** — when the LSP client sends `insertSpaces: false`, the server silently uses spaces. This matches RedHat yaml-language-server and Prettier behavior. No diagnostic or warning is emitted.
- **Parser spec-violation audit already completed** — the auto-memory entry (`project_use_tabs_spec_violation.md`) mentioned auditing the parser for other spec violations alongside the `use_tabs` drop. That audit was completed in plan `2026-04-21-yaml-spec-conformance-audit.md` (commit 4a2e197). This plan covers only the `use_tabs` removal.
