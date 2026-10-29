**Repository:** root
**Status:** NotStarted
**Created:** 2026-10-29

# Fast Schema Glob Matching

## Goal

The non-backtracking glob matcher that replaced the backtracking one
(963a198d, a security control against patterns that cost ~n^k) made a
full SchemaStore scan several times slower, paid on every validation of
a document without a workspace `schemas` match. Remove that cost without
giving up the bounded worst case or changing any match result: compile
the catalog globs once, match without heap allocation for every glob
within the stack bound, reject early on the glob's trailing literal,
stop cloning the catalog per validation, and give the user a benchmark
to measure before and after on bare metal.

## Context

- `rlsp-yaml/src/schema/association.rs`: `Glob` (compiled `Token`s) and
  `Glob::is_match`, a reached-set simulation whose cost is at most glob
  length × text length; it allocates two `Vec<bool>` per call.
  `glob_matches(pattern, text)` compiles `Glob::new(pattern)` on every
  call. `FilePatterns` (workspace `schemas`) stores compiled globs;
  `MAX_PATTERN_BYTES` bounds each of its patterns.
- `rlsp-yaml/src/schema.rs`: the public `SchemaStoreCatalog { entries }`
  and `SchemaStoreEntry { url, file_match: Vec<String> }`;
  `parse_schemastore_catalog` (private) keeps only YAML patterns;
  `match_schemastore(filename, &catalog)` calls `glob_matches` for every
  pattern of every entry and returns the first entry that matches.
- Tests in `association.rs` pin matching against the old backtracking
  matcher, kept as a reference model (`backtracking_glob_matches` and
  the `…agrees_with_the_backtracking_matcher` proptests).

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [ ] Task 1: a benchmark measures schema glob matching, and test
      catalogs are built in one place
- [ ] Task 2: the SchemaStore catalog holds compiled globs and is shared,
      not cloned
- [ ] Task 3: a match allocates nothing within the stack bound and
      rejects early on the trailing literal
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Benchmark schema glob matching; build test catalogs in one place

The benchmark lands before any matcher change, so the user measures the
baseline with the same benchmark source as every later point. Behavior
is unchanged. `rlsp-yaml/benches/hot_path.rs` holds the Criterion groups
for code hit on every validation; `benches/fixtures/mod.rs` holds the
shared generators.

- [ ] Every test that builds a `SchemaStoreCatalog` builds it through
      one helper, and the `SchemaStore` catalog and `match_schemastore`
      tests in `schema.rs` pass with their assertions unchanged
- [ ] `parse_schemastore_catalog` is public
- [ ] A Criterion group in `benches/hot_path.rs` measures, through
      `parse_schemastore_catalog`, `match_schemastore`,
      `FilePatterns::new`, and `match_schema_by_filename` only: catalog
      loading; a SchemaStore scan with no match, with a late match, and
      for a deep path of 200 bytes; and workspace `schemas` matching for
      a matching and a non-matching path
- [ ] The benchmark's catalog is a deterministic synthetic catalog
      generated in `benches/fixtures/mod.rs`, of 700 entries holding
      1500 YAML patterns in the shapes the real catalog uses
      (`**/.github/workflows/*.yml`, `**/<name>.yaml`, `**/*.<tool>.yml`,
      bare `<name>.yml`, `<dir>/**/<name>.yaml`, `*.<x>.yaml`) and 20
      non-YAML patterns, and paths are absolute like `uri.path()`
- [ ] `cargo bench --bench hot_path` runs the new group to completion
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 2: Compile the catalog once and share it

The catalog's patterns are compiled when it is parsed and the catalog
becomes opaque. `rlsp-yaml/src/server.rs`
(`get_or_fetch_schemastore_catalog`) keeps the catalog in a
`Mutex<Option<SchemaStoreCatalog>>` and clones it out for every
validation that reaches the SchemaStore fallback.

- [ ] A `SchemaStoreCatalog` holds each YAML `fileMatch` pattern
      compiled once, when it is parsed; `rlsp_yaml::schema` exposes no
      SchemaStore entry type and no catalog field, and
      `parse_schemastore_catalog` and `match_schemastore` keep their
      signatures
- [ ] No production code compiles a glob per match; `glob_matches`
      remains only as a test helper
- [ ] The server keeps one shared catalog and does not clone it per
      validation
- [ ] The matching tests in `association.rs` and `schema.rs`, the
      proptests against the backtracking reference model included, pass
      with their assertions unchanged; the catalog parse tests that
      compared stored pattern strings check the same patterns through
      matching
- [ ] The commit is marked breaking, and its `BREAKING CHANGE:` footer
      tells library users that `SchemaStoreEntry` and the catalog's
      `entries` field are removed and that `parse_schemastore_catalog`
      builds a catalog
- [ ] Build, tests, `cargo clippy --all-targets`, fmt, and
      `cargo bench --bench hot_path --no-run` pass

### Task 3: Match without allocating, and reject on the trailing literal

- [ ] Matching a glob with no more states than a `FilePatterns` pattern
      can have (`MAX_PATTERN_BYTES` plus the leading `**/` and the end
      state) allocates no heap memory; a longer catalog glob still
      matches correctly
- [ ] A glob whose trailing run of literal bytes does not end the text
      is rejected before the simulation runs
- [ ] The proptests against the backtracking reference model pass with
      their assertions unchanged; an added proptest with a fixed literal
      suffix exercises the case where the trailing literal does end the
      text; globs at one state below, at, and one state above the stack
      bound, and a `FilePatterns` pattern of exactly
      `MAX_PATTERN_BYTES`, are compared with the reference model for a
      match and a miss
- [ ] The tests that pin the bounded worst case
      (`matching_ends_for_a_glob_that_made_backtracking_explode`,
      `match_schemastore_ends_for_a_file_match_that_made_backtracking_explode`)
      use texts that end in the glob's trailing literal and assert that
      they do, so they still run the simulation
- [ ] Build, tests, `cargo clippy --all-targets`, fmt, and
      `cargo bench --bench hot_path --no-run` pass

## Non-Goals

- Matching SchemaStore patterns written as bare names (such as
  `docker-compose.yml`) below a directory, as upstream does (separate
  plan after this one)
- An index of catalog entries by file extension
- The per-validation clone of the workspace `schemas` map
  (`get_schemas` in `server.rs`)

## Decisions

- **No performance threshold is an acceptance criterion:** agents run
  in Docker, so timing numbers are not comparable; the user measures
  with the benchmark on bare metal.
- **Synthetic benchmark catalog:** no snapshot of the real catalog
  enters the repository, where it would age.
- **Every input computes the same result as today:** compiling at load
  is pure and keeps entry order and first-match-wins; the `Arc` holds a
  catalog written once per session; the stack and heap buffers run one
  simulation body; the trailing-literal check is a necessary condition,
  because a literal token consumes exactly one byte and the simulation
  skips only wildcards.
- **The catalog becomes opaque:** a public string field next to the
  compiled globs could drift and doubles memory; adding a private field
  breaks struct literals anyway. The breaking change rides in the same
  unreleased version as 963a198d.
- **Stack buffers sized from `MAX_PATTERN_BYTES`:** declined: a `u64`
  bitset (caps catalog glob length or needs a second path), caller
  scratch (threads a parameter through three functions and still
  allocates), a single in-place buffer (saves 258 bytes for a riskier
  step rewrite).
