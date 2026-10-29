**Repository:** root
**Status:** InProgress
**Created:** 2026-10-29

# SchemaStore Patterns Follow the Upstream Rules

## Goal

SchemaStore `fileMatch` patterns are matched literally from the start of
the absolute document path, so every pattern without a leading `**/` —
1226 of the catalog's 1730 YAML patterns today, including
`docker-compose.yml`, `.gitlab-ci.yml`, and `action.yml` — never
applies. Match catalog patterns by the same upstream rules the workspace
`schemas` setting already follows, keep the catalog's patterns the way
yaml-language-server does (drop only JSON-family patterns), and remove
the glob mode that nothing uses afterwards.

## Context

- yaml-language-server (verified in its source): SchemaStore entries
  join the same association list as `yaml.schemas` with `fileMatch`
  unchanged and are matched through `FilePatternAssociation` — one
  leading `!` marks an exclusion, one leading `/` is stripped, `**/` is
  prepended, a pattern empty after stripping is ignored. Its catalog
  filter drops a pattern when
  `['.json', '.jsonc', '.json5'].includes(path.posix.extname(pattern))`.
- `rlsp-yaml/src/schema/association.rs`: `FilePatterns` implements those
  rules for the `schemas` setting (`**/` anchored at path segment
  starts) together with the settings bounds (`MAX_PATTERNS_PER_ENTRY`,
  `MAX_PATTERN_BYTES` in `FilePatterns::new`, `MAX_PATH_BYTES` in
  `FilePatterns::matches`).
- `rlsp-yaml/src/schema.rs`: the opaque `SchemaStoreCatalog` holds, per
  entry, globs compiled in a plain mode where `**/` may end at any byte;
  `parse_schemastore_catalog` keeps only `.yml`/`.yaml` patterns;
  `match_schemastore` returns the first entry in catalog order with a
  matching pattern.
- `rlsp-yaml/benches/hot_path.rs` (`schema_glob_matching` group) asserts
  match results on its synthetic catalog before it measures; the
  catalog and its paths come from `benches/fixtures/mod.rs`.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: `FilePatterns` separates its pattern rules from the
      settings bounds
- [x] Task 2: the benchmark catalog contains the shapes the new rules
      make costly
- [x] Task 3: SchemaStore patterns follow the upstream rules
- [ ] Task 4: the unused plain glob mode is gone
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Separate the pattern rules from the settings bounds

A bound-free way to build `FilePatterns` lets the catalog reuse the
pattern rules without the bounds that guard the user's settings.
Behavior is unchanged.

- [x] `FilePatterns` can be built without any bound by a constructor
      that takes the patterns as `&str` items; `FilePatterns::new` keeps
      its count and length bounds and refuses exactly what it refuses
      today
- [x] The path-length bound applies in `match_schema_by_filename`, and
      `FilePatterns::matches` applies no bound
- [x] The `FilePatterns` tests in `association.rs`, the settings bound
      tests in `settings.rs`, and `tests/schema_memory.rs` pass with
      their assertions unchanged;
      `file_patterns_match_a_path_up_to_the_limit_and_no_longer` and
      `file_patterns_match_a_huge_path_to_nothing_without_work` assert
      the same results through `match_schema_by_filename`
- [x] No doc comment on `FilePatterns`, `MAX_PATH_BYTES`, or
      `MAX_PATTERN_BYTES` misstates how a `FilePatterns` can be built or
      where the path bound applies
- [x] Build, tests, `cargo clippy --all-targets`, fmt, and
      `cargo bench --bench hot_path --no-run` pass

### Task 2: Give the benchmark catalog the costly shapes

The benchmark catalog's patterns all end in literals that the trailing-
literal check rejects at once, so it would not show what the new rules
cost. The matcher is unchanged.

- [x] The synthetic catalog in `benches/fixtures/mod.rs` also contains
      patterns of the shape `<dir>/*.yml`, patterns that are neither
      JSON-family nor `.yml`/`.yaml` (such as `.<tool>rc` or `*.toml`),
      one `!` pattern, and one pattern with a leading `/`, and its shape
      check covers them
- [x] A new benchmark in the `schema_glob_matching` group scans the
      catalog for a `.yml` path that no entry matches
- [x] The fixture path `schemastore_non_yaml_only_path` becomes a path
      that only JSON-family patterns name, and its name, its doc
      comment, and the assertion message that uses it say so
- [x] `cargo bench --bench hot_path` runs the `schema_glob_matching`
      group to completion, its setup assertions holding
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 3: Match SchemaStore patterns by the upstream rules

Bare-name catalog patterns such as `docker-compose.yml` start to apply:
catalog entries hold the bound-free `FilePatterns` of Task 1, so the
catalog follows the same rules as the `schemas` setting. Tests build
catalogs through a `catalog_of` helper today.

- [x] A catalog entry's patterns are matched by the upstream rules: one
      leading `!` excludes, one leading `/` is stripped, a pattern
      matches anywhere in the path with `**/` anchored at segment starts
      (an explicit `**/` too), and a pattern empty after stripping is
      ignored; the first matching entry in catalog order still wins
- [x] The catalog drops a pattern exactly when its POSIX extension —
      from the last `.` of its last `/`-separated segment, unless that
      `.` starts the segment — is `.json`, `.jsonc`, or `.json5`,
      compared case-sensitively, and keeps every other pattern, the same
      on every platform
- [x] An entry with no positive pattern after filtering is dropped
      without a message; a non-string `fileMatch` item is skipped and
      the entry kept, as today
- [x] The catalog applies none of the settings bounds
- [x] Every test catalog is built through `parse_schemastore_catalog`;
      `match_schemastore_reads_file_match_patterns_literally` and
      `parsed_catalog_reads_file_match_patterns_literally` are one table
      that pins the upstream rules on absolute paths, including a bare
      name matched below a directory, `*.yaml` below a directory, an
      exclusion, a stripped leading `/`, and an explicit `**/` that does
      not match in the middle of a file name
- [x] The parse tests pin the new filter (`.jsonc` and `.json5` dropped,
      `*.JSON` and a bare `.json` name kept, a pattern that is neither
      JSON-family nor YAML kept) and the dropped entries without a
      positive pattern; the catalog tests without a bound
      (`parsed_catalog_matches_a_file_match_beyond_what_a_schemas_pattern_can_be`,
      `match_schemastore_has_no_path_length_limit`, the heap test in
      `tests/schema_memory.rs`) pass with their assertions unchanged
- [x] An integration test in `tests/lsp_lifecycle/` seeds a catalog and
      its schema through a hidden server entry point and pins that
      opening `/p/docker-compose.yml` applies the schema while
      `/p/legacy/docker-compose.yml`, excluded by a `!` pattern, does
      not
- [x] With `rlsp-yaml/docs/configuration.md`, a user can tell how a
      SchemaStore `fileMatch` pattern applies to a document, and which
      entry wins when several match
- [x] `rlsp-yaml/docs/feature-log.md` has an entry that tells the reader
      that SchemaStore now assigns schemas to files such as
      `docker-compose.yml` and `.gitlab-ci.yml` in any directory
- [x] No doc comment or code comment describes the catalog as filtered
      to YAML patterns, catalog patterns as matched literally, or
      `FilePatterns` as used only by the `schemas` setting
- [x] `cargo bench --bench hot_path` runs the `schema_glob_matching`
      group to completion, its setup assertions holding, and the
      fixture's doc comments and assertion messages describe the new
      filter
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 4: Remove the plain glob mode

After Task 3 nothing in production builds a plain glob (`Glob::new`,
the plain mode of `tokenize`).

- [ ] `Glob::new`, `glob_matches`, and the plain mode of `tokenize` are
      gone; `**` followed by `/` always ends at a path segment start,
      and the supported glob syntax stays documented on what remains
- [ ] Every behavior the plain-mode tests pinned is pinned in the
      anchored mode: the tests compare `FilePatterns` with a reference
      model in the test module that anchors `**/` at segment starts;
      the two proptests that compared the plain mode and `FilePatterns`
      with reference models are one proptest with `**` in its alphabet,
      which also covers a literal suffix that ends the text;
      `glob_matches_follows_the_documented_syntax` becomes anchored-mode
      cases checked against both the reference model and the
      implementation; the stack-bound and worst-case-bound tests run on
      anchored globs
- [ ] No comment, doc comment, or test name refers to a plain or
      non-anchored glob mode
- [ ] `cargo bench --bench hot_path` runs the `schema_glob_matching`
      group to completion, its setup assertions holding
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- Combining several matching catalog schemas with `allOf`, as upstream
  does
- picomatch syntax beyond `*` and `**` (`?`, `[…]`, `{a,b}`, a leading
  `./`); the 11 catalog patterns that use `?` stay literal
- Relative paths and the `kubernetes` keyword as `schemas` keys

## Decisions

- **The upstream rules, `!` included, through `FilePatterns`:** one
  implementation of the rules for both sources. Declined: keeping the
  literal matching; `**/` without `!`.
- **The first matching entry wins:** one schema per document; a
  workspace `schemas` entry or a modeline overrides it. Declined:
  `allOf` over all matches.
- **Drop only JSON-family patterns, by upstream's exact test:** YAML
  files without a YAML extension, such as `.clang-format`, get their
  schema; the POSIX-extension test, case-sensitive, is upstream's
  `path.posix.extname` check, so `*.JSON` and a bare `.json` name are
  kept as upstream keeps them. Declined: keeping only `.yml`/`.yaml`.
- **No bounds for the catalog:** the catalog never had them, upstream
  has none, and they would drop a future entry silently; the real
  catalog (fetched 2026-10-29, 1550 entries) has at most 14 kept
  patterns per entry and none longer than 49 bytes, so the bounds would
  drop nothing today. The settings keep all bounds.
- **Entries without a positive pattern are dropped silently:** the user
  cannot change the catalog, so a message would be noise.
- **The benchmark changes before the switch, in its own commit:** the
  switch then compares on the same input; no performance threshold is
  an acceptance criterion, the user measures on bare metal.
- **Behavior that changes besides the new rules:**
  - An explicit `**/` in a catalog pattern no longer matches in the
    middle of a file name (`**/docker-compose.yml` on
    `/p/my-docker-compose.yml`).
  - A document matched today by a later entry can resolve to an earlier
    entry whose pattern now matches; 6 patterns of the real catalog,
    such as `plugin.yml`, are shared by more than one entry.
  - An entry with only `!` patterns or only empty patterns is dropped;
    today such patterns matched no document anyway.
