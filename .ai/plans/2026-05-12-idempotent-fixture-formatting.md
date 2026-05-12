**Repository:** root
**Status:** InProgress
**Created:** 2026-05-12

## Goal

Strengthen the formatter fixture test harness so that
`idempotent: true` fixtures must have Test-Documents
already in formatted form (`format(input) == input`), and
fix all fixtures that violate this invariant. Also fix
code-action fixtures whose flow mapping YAML is
inconsistent with the formatter's default `bracket_spacing:
true` output.

## Context

The formatter fixture harness (`formatter_fixtures.rs`)
currently checks idempotent fixtures with
`format(format(input)) == format(input)` — it verifies
two-pass convergence, not that the input is already
formatted. This means fixtures marked `idempotent: true`
can have unformatted Test-Documents that silently pass.

Running the stricter assertion `format(input) == input`
on all `idempotent: true` fixtures reveals 8 failures:

| Fixture | Cause |
|---------|-------|
| `flow-mapping-idempotent` | `{a: 1, b: 2}` missing bracket spacing |
| `flow-mapping-idempotent-g2` | `{k: v}` missing bracket spacing |
| `flow-sequence-idempotent-long-broken` | single-line input exceeds print_width; formatter breaks it |
| `block-enforce-idempotent` | flow input with enforce_block_style; tests transformation, not idempotency |
| `ecosystem-docker-compose` | unnecessary quotes and blank lines between sections |
| `ecosystem-k8s-deployment` | unnecessary quotes in flow sequence items |
| `explicit-key-idempotent-block-scalar-key` | 2-space indent in block scalar; formatter uses 4-space |
| `explicit-key-idempotent-sequence-as-key` | value sequence on same line as `:`; formatter breaks to next line |

`block-enforce-idempotent` is redundant — the
transformation it tests is already covered by
`block-enforce-converts-flow-sequence`, and the stability
is covered by `block-enforce-block-sequence-unchanged`.

Separately, 4 code-action fixtures have flow mappings
without bracket spacing (`{key: val}` instead of
`{ key: val }`): 3 `omits-action` fixtures and 1
`applies-action` fixture. These are not caught by the
harness change (they're not formatter idempotent tests)
but are inconsistent with default formatter output. Two
other `omits-action` fixtures
(`block-scalar-flow-sequence-value-omits` and
`block-scalar-sequence-item-in-flow-sequence-omits`) use
flow sequences `[...]` which don't get bracket spacing —
those are already correct.

Key files:
- `rlsp-yaml/tests/formatter_fixtures.rs` — harness
- `rlsp-yaml/tests/fixtures/formatter/*.md` — formatter fixtures
- `rlsp-yaml/tests/fixtures/code_actions/*.md` — code-action fixtures

## Steps

- [x] Strengthen idempotent assertion in harness
- [x] Delete redundant fixture
- [x] Fix 7 idempotent fixtures to use formatted input
- [x] Fix 4 code-action fixtures with unspaced flow mappings
- [x] All tests pass (`cargo test --package rlsp-yaml`)

## Tasks

### Task 1: Strengthen harness and fix idempotent fixtures — `962e36e`

Change the `idempotent` branch in `formatter_fixture()`
(`formatter_fixtures.rs:276-298`) to assert
`format(input) == input` instead of
`format(format(input)) == format(input)`.

Then fix all 8 failing fixtures:

**Delete:**
- `formatter/block-enforce-idempotent.md` — redundant with
  `block-enforce-converts-flow-sequence` (transformation)
  and `block-enforce-block-sequence-unchanged` (stability)

**Update Test-Document to formatted form:**
- `flow-mapping-idempotent` — `{ a: 1, b: 2 }`
- `flow-mapping-idempotent-g2` — `{ k: v }`
- `flow-sequence-idempotent-long-broken` — broken across
  lines (the formatted form at print_width: 40)
- `ecosystem-docker-compose` — strip unnecessary quotes
  from flow sequence items, remove blank lines between
  top-level mapping entries
- `ecosystem-k8s-deployment` — strip unnecessary quotes
  from flow sequence items (`"python"` → `python`,
  `"http.server"` → `http.server`; keep `"-m"` and
  `"5000"` quoted)
- `explicit-key-idempotent-block-scalar-key` — 4-space
  indent for block scalar content
- `explicit-key-idempotent-sequence-as-key` — value
  sequence on next line after `:`

Acceptance criteria:
- [x] `cargo test --package rlsp-yaml --test formatter_fixtures` passes
- [x] The idempotent branch asserts `first == fixture.test_document`
- [x] The `idempotent` field doc comment in `FixtureSpec`
      (`formatter_fixtures.rs:53`) is updated to describe
      the new `format(input) == input` assertion
- [x] `block-enforce-idempotent.md` is deleted
- [x] 7 remaining idempotent fixtures have Test-Documents
      that match their formatted output

### Task 2: Fix code-action fixtures with unspaced flow mappings — `aa8928c`

Update flow mapping YAML in these code-action fixtures to
use bracket spacing (`{ key: val }` not `{key: val}`):

**omits-action (Test-Document only):**
- `block-scalar-flow-collection-omits.md` —
  `key: { aaaa...: 1 }`
- `block-scalar-scalar-in-flow-mapping-omits.md` —
  `{ key: "aaaa..." }`
- `block-scalar-scalar-in-nested-flow-mapping-omits.md` —
  `outer: { key: "aaaa..." }`

**applies-action (both Test-Document and Expected-Document):**
- `quoted-bool-flow-mapping-value-applies.md` —
  Test: `config: { enabled: "true" }`,
  Expected: `config: { enabled: true }`

Note: `block-scalar-flow-sequence-value-omits.md` and
`block-scalar-sequence-item-in-flow-sequence-omits.md`
use flow sequences `[...]` which do NOT get bracket
spacing — the formatter only applies bracket spacing to
flow mappings. These are already correct.

Acceptance criteria:
- [x] `cargo test --package rlsp-yaml --test code_action_fixtures` passes
- [x] All 4 updated fixtures use `{ ... }` with spaces
      inside braces

## Decisions

- **Delete vs fix `block-enforce-idempotent`:** delete —
  the transformation and stability are each tested by
  dedicated fixtures already. A fixed version would be
  nearly identical to `block-enforce-block-sequence-unchanged`.
- **Flow sequences don't get bracket spacing:** the
  formatter's `flow_sequence_to_doc()` does not apply
  `bracket_spacing`, only `flow_mapping_to_doc()` does.
  This matches standard YAML convention (`[a, b]` vs
  `{ a: 1 }`). Sequence fixtures with `[...]` are correct
  as-is.
- **Harness asserts `format(input) == input`:** this
  replaces the weaker two-pass convergence check. The old
  assertion only verified `format(format(x)) == format(x)`,
  which passes for any convergent formatter regardless of
  input quality. The new assertion ensures idempotent
  fixtures are genuine stability tests.
