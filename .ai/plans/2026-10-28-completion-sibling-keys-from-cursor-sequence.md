**Repository:** root
**Status:** InProgress
**Created:** 2026-10-28

# Completion Takes Sibling Item Keys From the Cursor's Sequence

## Goal

When the cursor is on a key inside a sequence item, structural
completion offers the keys of the sequence's other items, but it
looks the sequence up in the first document of the file, so in the
second and later documents of a multi-document file it offers
another document's keys or none, and in a sequence nested inside
another sequence's item it offers only the current item's own keys.
Take the sibling items from the sequence the cursor is in, in every
document and at every nesting level.

## Context

- `rlsp-yaml/src/completion/completion_drivers.rs`, `complete_on_key`:
  when `enclosing_path` ends in an item segment, it looks the sequence
  up with `find_node_at_path` (`src/completion/navigation.rs`), which
  walks `docs.first()` and returns nothing for any item segment on the
  way; without a sequence it falls back to the mapping's own keys.
- `rlsp-yaml/src/completion/cursor_location.rs`: `locate_cursor` and
  `locate_in_node` walk the cursor's own document.
  `CursorLocation::OnKey { key, enclosing_path, mapping }` does not
  carry the enclosing sequence; `InSequenceItem` and `InBlankSequence`
  carry theirs as `sequence`.
- Structural keys of a sequence are the union of all its items' keys
  (`collect_sequence_sibling_keys`).

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: completion integration tests share one setup helper
- [x] Task 2: sibling item keys come from the cursor's sequence
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: One setup helper for the completion integration tests

`tests/lsp_lifecycle/completion.rs` repeats the server setup,
document open, and completion request in each test and in its three
schema helpers. One helper serves them all, so the tests of Task 2
stay short. Test-only; behavior is unchanged.

- [x] One async helper initializes the server, opens a document,
      requests completion at a position, and returns the result; the
      schemaless tests and the schema helpers
      (`completion_with_all_of`, `completion_with_servers_in_any_of`,
      `completion_with_schema`) use it, and no other copy of that
      sequence remains in the file
- [x] Every test in the file keeps its document, position, and
      assertions
- [x] Build, tests, clippy, and fmt pass

### Task 2: Take sibling item keys from the cursor's sequence

`CursorLocation::OnKey` carries the sequence whose item the mapping
is, and the key driver takes its structural keys from it; the locator
passes that sequence down when it descends into an item.

- [x] `OnKey` carries the enclosing sequence when its mapping is an
      item of one, and none otherwise; `complete_on_key` takes its
      structural keys from it; `find_node_at_path` and its tests and
      imports are removed
- [x] Locator tests pin the carried sequence: present for a key in an
      item of a root sequence, of a sequence under a key, of a
      sequence under a `"[]"` key, of a sequence directly inside an
      item, and of a sequence under a key inside an item; absent for
      a key in a mapping under a key inside an item and for keys
      outside sequences; for a key in the second document it is that
      document's sequence; and wherever present, the mapping is one
      of its items
- [x] Unit tests through `complete_at` pin, comparing labels as sets
      (the structural keys come from a `HashSet` without a stable
      order):
      a key in a later document offers that document's sibling item
      keys and none of the first document's, when the first document
      has a sequence at the same key path, when it has none (a mapping
      root, an empty first document, a `"[]"` key), and in a third
      document; a key in a sequence directly inside an item and in a
      sequence under a key inside an item offers the sibling items'
      keys; the enclosing sequence decides, not an ancestor sequence;
      a mapping under a key inside an item offers its own keys only;
      with duplicate keys or a complex key along the path, the keys of
      the cursor's own sequence are offered, not those of the first
      match by key text; and with a
      schema in the second document, a sibling key the schema does not
      declare appears and a first-document key does not
- [x] `on_a_key_in_a_nested_sequence_item_the_mapping_siblings_apply`
      is renamed to state the new rule and uses a document whose
      result differs between the old and the new rule
- [x] Every other existing test passes with its assertions unchanged
- [x] Integration tests in `tests/lsp_lifecycle/completion.rs` pin
      completion of sibling item keys in the second document of a
      multi-document file, without the first document's keys, and in
      a sequence under a key inside an item (`containers[].ports[]`)
- [x] `rlsp-yaml/docs/feature-log.md` has an entry that tells the
      reader that key completion in a sequence item offers the keys of
      the other items of that sequence, in every document of a file
      and in nested sequences
- [x] Build, tests, clippy, and fmt pass

## Non-Goals

- Schema association (Kubernetes detection, modelines) that looks at
  the first document only

## Decisions

- **The locator hands over the sequence:** the locator already walks
  the cursor's document and knows the sequence, so no path is looked
  up again. Declined: rebuilding `OnKey` after the fact by node
  identity, and a separate `OnKey` variant for keys in items, which
  would duplicate the schema branch of the driver.
- **Nested sequences offer sibling item keys:** the same rule as a
  top-level sequence.
- **A later document without a matching first-document sequence now
  offers sibling keys:** where the first document has no sequence at
  the cursor's key path, completion offered the mapping's own keys
  only; it now offers the cursor's sequence's sibling keys.
- **Duplicate and complex keys along the path follow the cursor:**
  with a key repeated along the path, or a non-scalar key (which the
  locator records as an empty key), the cursor's own sequence decides
  instead of the first match by key text.
- **Other first-document reads stay:** completion's other uses of
  `docs.first()` read only `line_index()`, which is one index shared
  by every document of the input.
