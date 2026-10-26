**Repository:** root
**Status:** NotStarted
**Created:** 2026-10-26

# Share `$ref` Targets and Bound Schema Size at Parse Time

## Goal

A YAML document whose schema references itself — the official
draft-07 JSON Schema meta-schema, or any recursive schema with
fan-out — makes the server exhaust memory and crash, because the
schema parser copies every `$ref` target and cuts recursion only
at depth 32. Parse each `$ref` target once and share it, so that a
parsed schema grows linearly with its source documents and
recursive schemas are no longer cut at a fixed depth, and reject a
schema whose parsed size exceeds a fixed budget with the existing
schema-failure Warning instead of applying part of it.

## Context

- `rlsp-yaml/src/schema.rs` parses a schema into `JsonSchema`, an
  owned tree (children are `Box<Self>`, `Vec<Self>`,
  `HashMap<String, Self>`). `parse_schema_with_root` replaces each
  `$ref` with a fresh parse of its target (`resolve_ref`: `#`,
  JSON pointers, anchors via `find_anchor_in_value`, remote
  URLs). `MAX_REF_DEPTH = 32`, counted across nesting and `$ref`
  hops, is the only bound, so a schema that references itself
  several times expands exponentially.
- Remote references go through `ParseContext` (cache, proxy,
  visited URLs): at most `MAX_REMOTE_FETCH_COUNT = 20` fetches per
  parse, the SSRF guard `validate_and_normalize_url`, and a
  depth-1 rule — references inside a fetched document that point
  to further remote documents stay unresolved. `fetch_schema_raw`
  caps a response at `MAX_SCHEMA_BYTES` (5 MiB) and JSON nesting at
  `MAX_JSON_DEPTH = 50`. `SchemaCache` maps a URL to its raw
  `Value` and parsed `JsonSchema`, first insertion wins.
- Consumers of the parsed tree: validation
  (`rlsp-yaml/src/schema_validation.rs` and
  `src/schema_validation/`, bounded by `MAX_VALIDATION_DEPTH = 64`
  and a per-run step budget), completion (`src/completion.rs`,
  `src/completion/`), hover (`src/hover.rs`), and code lens
  (`src/decorators/code_lens.rs`). `Backend::process_schema` in
  `src/server.rs` fetches, caches, and validates, and logs
  failures through `log_schema_failure`.
- `rlsp_yaml::schema` (`JsonSchema`, `parse_schema`, `SchemaCache`)
  and the consumer entry points `validate_schema`, `complete_at`,
  `hover_at`, `code_lenses` are public API of the published crate.
- Vendored real schemas used by the step-budget tests
  (`src/schema_validation/budget_tests.rs`) live in
  `rlsp-yaml/tests/fixtures/schemas/` with sources and licences in
  its `README.md`.
- Specification: JSON Schema draft-07 and 2020-12 `$ref`, `$id`,
  `$anchor`, `$dynamicRef` (https://json-schema.org/specification).

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [ ] Task 1: parse results are shared, not copied, by the cache
      and every consumer
- [ ] Task 2: one parser struct carries per-parse state
- [ ] Task 3: `definitions`/`$defs` are no longer parsed into the
      tree
- [ ] Task 4: consumers follow reference nodes
- [ ] Task 5: remote documents are cached raw and parsed only as
      targets
- [ ] Task 6: each `$ref` target is parsed once and shared
- [ ] Task 7: a schema over the size budget is rejected with a
      Warning
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Share parse results through the cache and every consumer

Introduce `ParsedSchema` as the result of parsing a schema — for
now a wrapper holding only the root `JsonSchema` — and make the
cache and every consumer entry point use it. Behavior is
unchanged; this separates the mechanical churn at several hundred
call sites from the design change.

- [ ] Parsing returns a `ParsedSchema`, and the cache and the
      consumer entry points take it; the cache hands it out
      shared, and no request copies a cached schema
- [ ] `SchemaCache::get_or_fetch`, which nothing calls, is
      removed
- [ ] The unit tests of validation, completion, hover, code lens,
      and `src/schema.rs`, the `tests/lsp_lifecycle/` suites, and
      the benches pass with their assertions unchanged — only
      mechanical edits that wrap a `JsonSchema` into a
      `ParsedSchema` or adjust cache access
- [ ] Build, tests, clippy, and fmt pass

### Task 2: Carry per-parse state in one parser struct

The parse functions in `src/schema.rs` thread the root document,
base URI, optional `ParseContext`, and depth through every call.
Replace them with one parser struct, always present, that holds
the state constant for one parse, so that later tasks add state
as fields. Behavior is unchanged: local `$ref` targets are still
parsed without remote resolution and without a base URI, and the
depth limits still apply as today.

- [ ] Parse state lives in one parser struct; no parse function
      threads the root document, base URI, remote context, and
      depth as separate arguments
- [ ] The `src/schema.rs` tests for `$ref`, `$id` and base URIs,
      anchors, `$dynamicRef`, remote references, the SSRF guard,
      and the cache pass with their assertions unchanged — only
      call-shape edits in tests that call parse internals
      directly
- [ ] Build, tests, clippy, and fmt pass

### Task 3: Stop parsing `definitions`/`$defs` into the tree

The parser stores every `definitions`/`$defs` entry inline in the
`definitions` field, which only tests read, and parses each entry
again at every `$ref` to it. `$ref`s keep resolving against the
raw document.

- [ ] `JsonSchema` has no `definitions` field and the parser no
      longer parses `definitions`/`$defs` entries except as `$ref`
      targets
- [ ] The tests that only assert the `definitions` field are
      removed; every other `src/schema.rs` test — among them the
      local `$ref` resolution tests — and the step-budget fixture
      tests pass with their assertions unchanged
- [ ] Build, tests, clippy, and fmt pass

### Task 4: Follow reference nodes in every consumer

A `$ref` becomes a node that carries a reference — resolved to a
target in the parsed schema's target table, or unresolved with
its text — in place of `ref_path`. Validation, completion, hover,
and code lens follow references wherever they read a sub-schema.
The parser still inlines every target, so users see no change;
tests build `ParsedSchema` values with shared and cyclic targets
directly. The parser starts producing references in Task 6.
Validation, completion, and hover read child-schema fields
directly, and a reference node reads as an empty schema to any
reader that does not follow it. Completion's property walk has no
depth or branch limit and today relies on the parser's depth
bound.

- [ ] `ParsedSchema` holds a target table and resolves a
      reference node in one step to a target that is never itself
      a reference node; an unresolved reference keeps its `$ref`
      text and validates permissively, as an unresolved `$ref`
      does today; `ref_path` no longer exists
- [ ] For each consumer — validation, completion, hover, code
      lens — and each child-schema position of `JsonSchema` that
      the consumer reads, the root included, a test makes the
      schema at that position a reference node and gives the same
      diagnostics, completion items, hover text, or lens title as
      the same schema written inline
- [ ] Validation of a document against a cyclic schema
      terminates, and following a reference spends no validation
      step
- [ ] Key completion terminates on a cyclic schema and on a chain
      of 10 000 distinct targets each referencing the next,
      without a stack overflow; a target reached through two
      composition paths contributes its keys once; completion
      path resolution and hover terminate on a cyclic schema
- [ ] Apart from replacing `ref_path`, the existing unit tests of
      the four consumers and the step-budget fixture tests pass
      with their assertions unchanged
- [ ] Build, tests, clippy, and fmt pass

### Task 5: Cache remote documents raw and parse them only as targets

A remote `$ref` today fetches the referenced document and parses
the whole document standalone before resolving the target, with
no bound; the result is also what a later document using that URL
as its own schema receives. The cache keeps fetched documents
separately from parsed schemas, and a remote document is parsed
only through the targets a parse references.

- [ ] The cache holds raw documents and parsed schemas
      separately, each first-insertion-wins; a remote `$ref`
      parses only its target, never the whole fetched document
      on its own
- [ ] A URL first fetched as a remote `$ref` target and later
      used as a document's own schema is parsed as that
      document's schema, with remote resolution
- [ ] No comment describes a cache entry as one raw value with
      its parsed schema, or a cached schema as resolved when its
      document was first fetched
- [ ] The existing `src/schema.rs` cache, `$id`-spoofing, and
      remote-reference tests and the `tests/lsp_lifecycle/`
      schema suites pass with their assertions unchanged — only
      edits for the cache's new shape
- [ ] Build, tests, clippy, and fmt pass

### Task 6: Parse each `$ref` target once and share it

Each `$ref` names a location — a document and a JSON pointer;
anchors and `$id`-based URLs that name the same object name the
same location. The parser parses each location once per parse,
from a worklist, and emits reference nodes to it, so parse
recursion never goes deeper than the source nesting. Recursive
schemas are then no longer cut at a fixed depth, which gives deep
documents new, correct diagnostics. The vendored draft-07
meta-schema comes from
https://raw.githubusercontent.com/json-schema-org/json-schema-spec/20a3fee852db88519dcb3bb329b7a872ebad8953/schema.json,
licensed AFL or BSD by its repository; it is used under
BSD-3-Clause.

- [ ] For the vendored draft-07 meta-schema and each vendored
      fixture, the parsed schema has no more nodes than its
      source documents have JSON values
- [ ] A schema with a chain of 10 000 distinct definitions, each
      referencing the next, parses without a stack overflow
- [ ] An integration test in `rlsp-yaml/tests/lsp_lifecycle/`
      opens a document whose schema is the draft-07 meta-schema
      and receives its schema diagnostics, including one for a
      value nested deeper than 32 schema levels
- [ ] Remote `$ref`s inside local targets resolve, against the
      nearest enclosing `$id` or the document's URL, within the
      fetch limit and the SSRF guard; references inside a fetched
      document that point to further remote documents stay
      unresolved
- [ ] A pure reference cycle such as a target whose only keyword
      is `$ref` to itself is permissive, `$dynamicRef` keeps its
      static resolution, and keywords next to `$ref` stay
      ignored
- [ ] `MAX_REF_DEPTH` no longer exists, and no comment describes
      `$ref` targets as copied or recursion as cut at a depth
- [ ] `parse_schema` returns `Result<ParsedSchema, SchemaError>`:
      `ParseFailed` for a root that is boolean `false` or not an
      object, `TooDeep` for JSON nested deeper than
      `MAX_JSON_DEPTH`; `fetch_schema_raw` reports the same errors
      for the documents it parses
- [ ] The step-budget fixture tests pass with their assertions
      unchanged
- [ ] `tests/fixtures/schemas/README.md` lists the draft-07
      meta-schema with its pinned source, its licence, and the
      copyright notice of the json-schema-spec repository, and
      describes what the directory's schemas are used for
- [ ] `rlsp-yaml/docs/configuration.md` no longer names a `$ref`
      resolution depth and tells the reader that recursive
      schemas are followed without a fixed `$ref` depth, within
      the limits its Schema Validation Limits section describes
- [ ] `rlsp-yaml/docs/feature-log.md` records that `$ref` targets
      are shared, so self-referencing schemas such as the JSON
      Schema meta-schemas load and are no longer cut at a fixed
      `$ref` depth
- [ ] Build, tests, clippy, and fmt pass

### Task 7: Reject a schema over the size budget

One counter per parse covers the root document and every target
in every document fetched for it: one unit per schema node, and
one unit per JSON value copied into `enum`, `const`, `default`, or
`examples`, counting every nested array element and object member
value; a shared target counts once. A parse that exceeds the
budget fails, and the schema is not applied at all.

- [ ] `parse_schema` and `fetch_schema_raw` return a new
      `SchemaError` variant, distinct from `ResponseTooLarge`, for
      a parse over the budget; a schema at the budget parses, and
      one unit over it is rejected
- [ ] The budget is exactly twice the units a schema of
      `MAX_SCHEMA_BYTES` needs at the highest units-per-byte ratio
      among the vendored fixtures and the draft-07 meta-schema,
      rounded up; the handoff reports each one's measured units
      and ratio
- [ ] When a schema is rejected, the client receives the existing
      schema-failure Warning naming the URL and stating that the
      schema is too large, and no schema diagnostic is published;
      Kubernetes auto-detection stays silent
- [ ] An integration test in `rlsp-yaml/tests/lsp_lifecycle/`
      opens a document whose schema exceeds the budget and
      asserts the Warning and the absence of schema diagnostics
- [ ] `rlsp-yaml/docs/configuration.md` tells the reader that a
      schema's parsed size is bounded at twice what a 5 MiB schema
      of the densest measured real schemas needs, and what they
      see when a schema exceeds it
- [ ] The `rlsp-yaml/docs/feature-log.md` entry for shared
      `$ref` targets also records the size bound
- [ ] Build, tests, clippy, and fmt pass

## Non-Goals

- Remembering rejected schemas across edits
- One path resolver for hover and completion, with hover
  documentation for array items and nested composition (separate
  plan after this one)
- Resolving remote references inside fetched documents that point
  to further remote documents
- Dynamic-scope `$dynamicRef` semantics
- The lost boolean `false` sub-schema in `parse_schema_with_root`
  (`unevaluatedItems: false` and similar keywords)

## Decisions

- **Share instead of copy, plus a budget:** sharing makes parsed
  size linear in the source and applies recursive schemas fully,
  as established validators do; a budget alone would reject every
  recursive schema with fan-out, and sharing alone leaves 20
  remote documents of 5 MiB each unbounded.
- **Reject, not truncate:** a truncated schema can lose a `oneOf`
  or `anyOf` branch and report errors the full schema would not.
- **Reference nodes with a target table:** consumers keep
  `&JsonSchema` internals and existing literals without `$ref`
  keep compiling. Declined: a full arena with node IDs for every
  child (every use and every test literal changes); `Arc`
  children with `Weak` back-edges (borrowed results such as
  completion path resolution stop compiling, and strong cycles
  leak).
- **Targets keyed by location and parsed from a worklist:** keying
  by `$ref` text parses one object several times, and parsing at
  the first reference recurses as deep as the reference chain.
- **Budget counts copied JSON values too:** `enum`, `const`,
  `default`, and `examples` copy their values, so a node count
  alone does not bound memory.
- **Remote `$ref`s inside local targets resolve:** one resolution
  path without a per-target flag; the fetch limit and SSRF guard
  still apply.
- **A shared target contributes its completion keys once:** the
  completion walk remembers entered targets so that shared and
  cyclic schemas terminate; this can change which items survive
  the `MAX_COMPLETION_ITEMS` cut. Keys from two distinct branches
  still appear twice.
- **Validation keeps its limits:** `MAX_VALIDATION_DEPTH` and the
  step budget already bound recursion; following a reference
  spends no step, so the step counts of existing documents stay
  the same.
- **One nesting rule:** with a worklist, parse recursion equals
  source nesting, so `MAX_JSON_DEPTH` replaces `MAX_REF_DEPTH`.
- **Remote references only inside unused definitions are no
  longer fetched:** a side effect of not parsing
  `definitions`/`$defs` inline; it frees fetch-limit slots for
  references that are used.
- **No memory of rejected schemas:** a rejected schema is fetched
  and parsed up to the budget again on every edit, as fetch
  failures are today; the cost per run is bounded by the budget,
  and remembering them needs its own invalidation policy.
- **Kubernetes auto-detection stays silent on rejection:** its
  URL is a guess, as for fetch failures.
- **Breaking API:** a task whose commit changes or removes a
  public item of `rlsp_yaml::schema` or a consumer entry point
  signature — a new variant of the exhaustive `SchemaError`
  included — carries the breaking marker.
