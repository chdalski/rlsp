# Rename Fixtures

Each `.md` file in this directory is a self-contained test case for
`rlsp_yaml::navigation::rename::rename(...)`.

## Frontmatter fields

All fields live between `---` delimiters.

| Field | Required | Description |
|-------|----------|-------------|
| `test-name` | yes | Kebab-case name (informational) |
| `category` | no | Short label (informational, e.g. `rename`) |
| `cursor` | yes | Zero-based `line:character` LSP `Position` of the rename trigger |
| `new-name` | yes | The proposed replacement anchor/alias name |
| `applies-rename` | see below | Set to `true`; asserts rename succeeds and applies edits |
| `omits-rename` | see below | Set to `true`; asserts rename returns `None` |

Exactly one of `applies-rename` or `omits-rename` must be present; they are mutually exclusive.

## What stays inline (Pattern C)

- All `prepare_rename` tests — `prepare_rename` returns `Option<Range>`, not a document
  transformation. These are Pattern C by construction.
- Rename tests asserting specific `Range` field values within `TextEdit`s (e.g.
  `should_produce_correct_edit_ranges`). The fixture format does not support range-field
  assertions — only output-document comparison.
- Existence-only assertions that don't validate output shape (e.g.
  `should_accept_new_name_at_exactly_max_length` — the 256-char name makes a readable
  Expected-Document impractical).
- Test inputs containing characters that cannot appear in YAML frontmatter — specifically
  newline (`\n`), tab (`\t`), and carriage return (`\r`). These cover security-relevant
  rejection cases (YAML-structure injection, CRLF injection, whitespace-semantics
  protection) and must be exercised with the actual control characters, not surrogates.
  They stay inline as Pattern C in `rename.rs`.

## Assertion modes

**`applies-rename: true`**

The harness calls `rename(docs, uri, cursor, new_name)`. On a `Some(WorkspaceEdit)` result,
it collects all `TextEdit`s from `changes[uri]`, sorts them in **reverse range-start order**
(highest line/character first), applies them to the `## Test-Document` string in that order,
then asserts the result equals `## Expected-Document`.

Applying edits in reverse order is necessary when rename produces multiple edits (anchor +
aliases): applying the edit at the later position first keeps the byte offsets of earlier
edits valid.

**`omits-rename: true`**

The harness asserts `rename(...)` returned `None`. `## Expected-Document` is not required
and should be omitted.

## Cursor convention

`cursor: line:character` uses zero-based line and character indices, matching the LSP
`Position` type.

## Sections

- `## Test-Document` — fenced YAML input (always required)
- `## Expected-Document` — fenced YAML output (required for `applies-rename`; omit for
  `omits-rename`)
