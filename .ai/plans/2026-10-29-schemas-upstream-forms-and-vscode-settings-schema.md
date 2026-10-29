**Repository:** root
**Status:** NotStarted
**Created:** 2026-10-29

# `schemas` in the Upstream Forms, and a VS Code Settings Schema That Matches the Server

## Goal

Users copy `schemas` settings from yaml-language-server, where a schema
URL maps to one glob or a list of globs, `!` excludes files, and a
pattern matches anywhere in the path. The server accepts only one glob
per URL and matches it from the start of the absolute path, so copied
lists are dropped and copied patterns such as `.github/workflows/*.yml`
— our own documented examples included — never match. Accept the
upstream forms with the upstream matching, and make the VS Code
extension's settings schema flag exactly what the server rejects and
offer the three settings it reads but the extension leaves out
(`hover`, `completion`, `maxItemsComputed`).

## Context

- yaml-language-server (verified in its README and in
  `src/languageservice/services/yamlSchemaService.ts`,
  `FilePatternAssociation`) — the **upstream pattern rules**: a
  `yaml.schemas` value is one glob or an array of globs; per pattern, a
  leading `!` is stripped first and marks the pattern as negative, then
  one leading `/` is stripped, and the rest is matched as if prefixed
  with `**/`, so it matches anywhere in the path; a pattern empty after
  stripping is ignored; at least one positive pattern is required; a
  file matches when a positive pattern matches and no negative one does.
- `rlsp-yaml/src/settings.rs`: `Settings.schemas` is a public
  `HashMap<String, String>`, read entry by entry; an entry whose value
  is not a string is dropped with one Warning, and rejected `schemas`
  values are never logged (only their JSON type).
- `rlsp-yaml/src/schema/association.rs`: `match_schema_by_filename`
  returns the first `SchemaAssociation` whose glob matches the URI path,
  through `glob_matches`; `glob_matches` also serves SchemaStore
  `fileMatch` patterns. `rlsp-yaml/src/server.rs`
  (`get_schema_associations`) rebuilds the `SchemaAssociation` list from
  the settings map for every document, in `HashMap` order, so the winner
  among overlapping entries changes between runs.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [ ] Task 1: the server accepts and matches `schemas` in the upstream
      forms
- [ ] Task 2: the VS Code settings schema matches the server and offers
      `hover`, `completion`, and `maxItemsComputed`
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Accept and match `schemas` in the upstream forms

A schemas entry becomes a set of positive and negative patterns built by
one constructor that the settings reader and the matcher share, and the
matcher reads the settings map directly.

- [ ] A `schemas` value of one glob string or an array of glob strings
      is read and matched by the upstream pattern rules in Context
- [ ] An entry whose value is neither a string nor an array of strings,
      or that has no positive pattern (an empty array, only `!`
      patterns, only patterns empty after stripping), is dropped as a
      whole with one Warning that names the entry and the accepted
      forms; the other entries apply, and the rejected value is still
      never logged
- [ ] When several entries match a file, the entry whose schema URL
      sorts first wins, on every run
- [ ] No code or comment in `rlsp-yaml` refers to a `SchemaAssociation`
      type; schema association by file name reads the settings'
      `schemas` map
- [ ] SchemaStore `fileMatch` patterns are matched as before: a leading
      `!` stays literal there
- [ ] Unit tests pin the pattern rules, including `!!x`, `!/x`,
      `["x", "!"]`, `"/"`, `""`, and overlapping entries
- [ ] Integration tests in `tests/lsp_lifecycle/`, through `initialize`
      and through `workspace/didChangeConfiguration`, pin that an array
      entry whose positive pattern matches the document applies its
      schema, that the same entry with a matching `!` pattern does not
      and the document falls through to the next resolution source, and
      that a pattern written like `.github/workflows/*.yml` applies to a
      document below that directory
- [ ] Tests that pinned the single-string form or the old Warning text
      assert the new forms and text, and the tests that check withheld
      `schemas` values use a value that is still rejected
- [ ] With `rlsp-yaml/docs/configuration.md`, a user who copies a
      yaml-language-server `schemas` entry can tell whether it applies,
      why a document is or is not matched, and which entry wins when
      several match
- [ ] `rlsp-yaml/docs/configuration.md` gives `formatPrintWidth` and
      `maxItemsComputed` as non-negative integers without a decimal
      point
- [ ] `rlsp-yaml/docs/feature-log.md` has an entry that tells the reader
      that `schemas` accepts the yaml-language-server forms and that
      patterns now match anywhere in the path
- [ ] The commit is marked breaking, and its `BREAKING CHANGE:` footer
      tells library users that `Settings.schemas` has a new value type
      and that `SchemaAssociation` is removed
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 2: A VS Code settings schema that matches the server

The extension declares what the server accepts, so the settings editor
flags rejected values, and it offers and sends the three settings the
server reads but the extension leaves out.
`rlsp-yaml/integrations/vscode/package.json`
(`contributes.configuration`) declares `formatPrintWidth` as `number`
and `schemas` as an `object` with unconstrained values; `src/config.ts`
(`getConfig`) sends what the extension declares, except that it leaves
out `formatPrintWidth` when the user has not set it, so `.editorconfig`
decides the width. A test in `rlsp-yaml/src/schema/association.rs`
already reads `package.json` to check the `kubernetesVersion` pattern
against the server.

- [ ] In `package.json`, `formatPrintWidth` and `maxItemsComputed` are
      non-negative integers, `hover` and `completion` are booleans with
      default `true`, and `maxItemsComputed` has default `5000`
- [ ] In `package.json`, a `schemas` value is a glob string or an array
      of glob strings with at least one positive pattern — positive as
      in the upstream pattern rules of Task 1: not starting with `!`,
      and not empty once one leading `/` is stripped — in a form that
      the server's own JSON Schema validator can evaluate
- [ ] `getConfig()` sends `hover`, `completion`, and `maxItemsComputed`,
      and types `schemas` values as a string or an array of strings
- [ ] A Rust test pins that every key the settings reader reads is
      declared in `package.json` with the matching type: boolean,
      non-negative integer, string, array of strings for `customTags`,
      object for `schemas`
- [ ] A Rust test pins that the `schemas` value schema in `package.json`
      and the server's pattern rules accept and reject the same samples:
      every case of Task 1's unit tests, an empty array, an array with a
      non-string element, a lone string, and an array with positive and
      `!` patterns
- [ ] A TypeScript test pins that `getConfig()` sends every declared
      setting except `server.path` and `formatPrintWidth` with the
      manifest's default when the user sets nothing, and the existing
      tests that pin `formatPrintWidth` absent when unset pass with
      their assertions unchanged
- [ ] The VS Code integration test of configuration defaults covers
      `hover`, `completion`, and `maxItemsComputed`
- [ ] Typecheck, lint, format, unit tests, and integration tests pass

## Non-Goals

- Relative paths and the `kubernetes` keyword as `schemas` keys (a
  queued item of its own)
- Matching patterns against the full URI instead of the URI path, or
  through picomatch — only the upstream pattern rules in Context are
  adopted

## Decisions

- **Support the upstream forms in the server:** declined: only marking
  the array form invalid in the VS Code schema — copied configurations
  would still not work.
- **A faulty entry is dropped as a whole:** declined: dropping only the
  faulty element — a dropped `!` pattern would apply the schema to more
  files than meant.
- **One `FilePatterns` type, built by one constructor:** the reader and
  the matcher share it, so the pattern rules live in one place and no
  `Settings` value can hold an entry without a positive pattern.
  `glob_matches` is unchanged, because SchemaStore uses it too.
- **`schemas` is a `BTreeMap`:** the entry whose URL sorts first wins
  among overlapping entries; today the winner follows `HashMap` order.
- **`SchemaAssociation` is removed:** it is a per-document copy of the
  settings map.
- **Behavior that changes besides the new forms:**
  - Patterns that never matched an absolute path (`*.yaml`,
    `.github/workflows/*.yml`) now match anywhere in the path; patterns
    that matched before (absolute paths, `**/…`) still match.
  - A string starting with `!`, and an entry with only `""` patterns,
    were accepted silently and are now dropped with a Warning.
  - An array of strings, rejected today, now applies.
  - An excluded document gets no "no schema" decision: it goes on to
    the next resolution source.
  - VS Code values a user already set for `rlsp-yaml.hover`,
    `rlsp-yaml.completion`, or `rlsp-yaml.maxItemsComputed` were not
    sent and now take effect; a wrongly typed one now gets the server's
    Warning.
  - VS Code still sends a value its schema flags; the server's Warning
    stays the second line of defense.
- **The new VS Code settings take the server's defaults:** `hover` and
  `completion` `true`, `maxItemsComputed` `5000`.
