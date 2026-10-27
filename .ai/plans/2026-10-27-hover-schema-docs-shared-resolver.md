**Repository:** root
**Status:** InProgress
**Created:** 2026-10-27

# Hover Schema Docs Through the Shared Path Resolver

## Goal

Hover finds a key's schema docs with its own resolver, which
searches composition branches only one level deep, never descends
into array items, and re-splits the displayed path at `.` and `[`,
so hover shows no schema docs for array items, for keys reached
through nested `allOf`/`anyOf`/`oneOf`, and for keys such as
`app.kubernetes.io/name`. Make completion's resolver the one
resolver both features use, so hover shows these docs; mark array
items in the path with their own type instead of the string
`"[]"`; and let array items be found through composition branches
too, so items of a schema such as
`anyOf: [{type: array, items: …}, {type: "null"}]` get hover docs
and schema completion.

## Context

- `rlsp-yaml/src/hover.rs`: `hover_at` walks the AST to a path of
  `PathSegment::{Key, Index}` and formats it for display
  (`format_path`); `build_schema_key_path`, `resolve_schema_path`,
  and `find_in_branches` are hover's own schema lookup.
- `rlsp-yaml/src/completion/schema_completions.rs`:
  `resolve_schema_path` is completion's resolver; it descends
  `items` for the path segment `"[]"`, which completion's path
  builders in `cursor_location.rs`, `completion.rs`, and
  `completion_drivers.rs` push.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: the path resolver lives in the schema layer
- [x] Task 2: array items are a typed path segment
- [ ] Task 3: hover resolves schema docs through the shared
      resolver
- [ ] Task 4: array items are found through composition branches
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Move the path resolver into the schema layer

Move completion's `resolve_schema_path` and the
`composition_branches` helper it shares with completion's property
walk into a new crate-internal module `src/schema/path.rs`, so that
hover can use the resolver without depending on completion.
Behavior is unchanged.

- [x] `resolve_schema_path` and `composition_branches` are defined
      in `src/schema/path.rs` and visible only inside the crate;
      completion uses them from there and keeps no copy
- [x] Tests that exercise only the resolver live with it in
      `src/schema/path.rs`; no test is deleted or has its
      assertions weakened
- [x] The moved tests and every completion test — in
      `src/completion.rs`, `src/completion/`, and
      `tests/lsp_lifecycle/completion.rs` — pass with their
      assertions unchanged; only import paths and the location of
      shared test builders change
- [x] Build, tests, clippy, and fmt pass

### Task 2: Mark array items with a typed path segment

The resolver takes a path of `Key(String)` and `Item` segments
instead of strings with the `"[]"` sentinel, so a YAML key named
`[]` is no longer taken for an array item.

- [x] The resolver and every completion path builder and path
      reader use the typed segment; no `"[]"` sentinel string
      remains in production code
- [x] With a schema that declares a property `"[]"` whose
      sub-schema has properties, `complete_at` under the document
      key `"[]"` offers those properties and not the parent's
      `items` properties, pinned by a test
- [x] Every other completion test passes with its assertions
      unchanged; only path literals change to the typed segment
- [x] Build, tests, clippy, and fmt pass

### Task 3: Hover resolves schema docs through the shared resolver

Hover maps its AST path to the typed schema path —
`Key` to `Key`, `Index` to `Item` — and resolves it with the
shared resolver; the displayed path keeps its indices.
`tests/lsp_lifecycle/hover.rs` has no schema-based test yet;
`tests/lsp_lifecycle/completion.rs` serves schemas through
`seed_schema_cache` and a `$schema` modeline.

- [ ] Hover's own schema resolver and its string-splitting path
      builder are removed
- [ ] Hover shows the schema docs of: a key below an array item
      (`items[0].name`); an array item itself (`items[0]`); an item
      of a nested sequence (`a[0][1]`) and a key below an item of
      a root sequence (`[0].name`); a key found only in a branch of
      a branch; a key whose first matching branch dead-ends deeper
      while a later branch resolves the path; a key containing `.`
      (`app.kubernetes.io/name`) or `[` (`foo[bar]`); the quoted
      key `"[]"`; the empty key `""` at the root and below another
      key, resolved as the property `""` and not its parent; and a
      document that is only a scalar, which shows the root
      schema's docs; each pinned by a unit test
- [ ] Every existing hover test in `src/hover.rs` and
      `tests/lsp_lifecycle/hover.rs` passes with its assertions
      unchanged
- [ ] An integration test in `tests/lsp_lifecycle/hover.rs` sends
      hover requests against a seeded schema for an array item's
      key, a key reached through nested composition, and a key
      containing `.`, and asserts that each response contains the
      key's schema description
- [ ] `rlsp-yaml/docs/feature-log.md` has an entry that tells the
      reader for which positions hover now shows schema docs
- [ ] Build, tests, clippy, and fmt pass

### Task 4: Find array items through composition branches

When a schema node has no own `items`, the resolver looks for
`items` in its `allOf`, `anyOf`, and `oneOf` branches in the same
order and with the same fallback as for a key; an own `items`
takes precedence over the branches.

- [ ] Hover shows the schema docs of an item, and of a key below
      an item, of a sequence whose schema is
      `anyOf: [{type: array, items: …}, {type: "null"}]`, and of a
      sequence whose `items` is reached only through an `allOf`
      branch of a `$ref` target, pinned by unit tests
- [ ] For a sequence whose `items` is reached only through a
      branch, completion offers the item schema's keys in an
      existing item, on a blank line in the sequence, and on a key
      below an item, and the item schema's enum values for a value
      below an item, pinned by unit tests through `complete_at`
- [ ] A schema node with an own `items` resolves to it even when a
      branch also declares `items`, and a self-referencing schema
      without any `items` still resolves an item path to nothing,
      pinned by tests
- [ ] Integration tests in `tests/lsp_lifecycle/hover.rs` and
      `tests/lsp_lifecycle/completion.rs` pin hover docs and key
      completion for an item of a sequence whose `items` is
      reached only through an `anyOf` branch
- [ ] The feature-log entry for this change also tells the reader
      that hover and completion find the items of an array schema
      declared in a composition branch
- [ ] Build, tests, clippy, and fmt pass

## Non-Goals

- Following `patternProperties`, `additionalProperties`, and
  `prefixItems` in the resolver (separate plan after this one)
- Showing docs from more than one branch in one hover
- `find_node_at_path` reading only the first document, and the
  per-request copy of the parsed documents in `server.rs`

## Decisions

- **One resolver, completion's walk:** it already covers every
  composition depth, fallback between branches, and
  self-referencing schemas.
- **First hit wins, depth-first:** a key or `items` is taken from
  the schema's own `properties` or `items` first, then from the
  `allOf`, `anyOf`, and `oneOf` branches in that order, depth
  first, so a key in a branch of `allOf[0]` wins over the same key
  in `allOf[1]` or `anyOf[0]`; hover shows that one schema's docs.
- **An own property or own `items` shadows the branches:** when
  the path dead-ends below it, the branches are not tried, as in
  completion today; fallback happens only among branches.
- **Resolver in the schema layer, crate-internal, free function:**
  hover and completion both depend on `schema`; `rlsp_yaml::schema`
  is a public module of the published crate, and a crate-only item
  adds no published API; the free function keeps the
  `(parsed, schema, …)` shape of completion's schema helpers.
- **Typed segment without an index:** completion's blank-line case
  has no item to index; the `prefixItems` plan adds the index it
  needs.
- **Hover's schema path comes from its AST path:** the display
  string loses which `.` and `[` belong to a key.
- **A scalar-only document shows the root schema's docs:** the
  resolver returns the root for an empty path; with an object root
  schema the hover shows `Schema type: object` on the scalar, which
  is what the schema declares.
- **Completion under a quoted `"[]"` key changes:** it follows the
  property `"[]"` instead of the parent's `items`; in a file with
  several documents, key completion inside a mapping under that
  key takes structural suggestions from that mapping instead of a
  sequence at the parent's path in the first document.
- **Array items through composition branches change completion:**
  item and below-item positions whose `items` is reached only
  through a branch get schema suggestions where they got only
  structural ones.
