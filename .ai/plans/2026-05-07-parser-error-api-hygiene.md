**Repository:** root
**Status:** InProgress
**Created:** 2026-05-07

# Parser Error API Hygiene

## Goal

Harden the `rlsp-yaml-parser` public error API so any
consumer — Python/WASM bindings, CLI tools, validators, or
the rlsp-yaml language server — can map parse-error
categories without substring-matching error messages, and
so the parser can evolve its error taxonomy internally
without semver-breaking external consumers. Two specific
changes:

1. Mark `Error` (struct), `LoadError` (enum), and
   `LoadError::Parse` (variant) `#[non_exhaustive]` so
   adding fields or variants in the future does not break
   external destructuring or matching.

2. Introduce a typed `ErrorKind` enum (also
   `#[non_exhaustive]`) on `Error` and propagate it
   through `LoadError::Parse`, with the minimum-meaningful
   starting taxonomy `InvalidCharacter` and `Syntax`
   (matching what the parser already structurally
   distinguishes at error-construction sites today).

This is consumer-agnostic forward-compat hygiene —
motivated by the principle that the parser API should only
change when the change is meaningful for any consumer, not
when one specific consumer needs a particular feature. The
language server's planned `invalidCharacter` diagnostic
code is one of several future consumers that will benefit;
it is not the driver.

## Context

### Current state

- `rlsp-yaml-parser/src/error.rs:6-13` defines
  `pub struct Error { pub pos: Pos, pub message: String }`.
  Public, exhaustive, no kind discriminator.
- `rlsp-yaml-parser/src/loader.rs:60-136` defines
  `pub enum LoadError` with 8 variants (`Parse`,
  `UnexpectedEndOfStream`, four `*LimitExceeded`,
  `CircularAlias`, `UndefinedAlias`, `UnresolvedScalar`).
  Public, exhaustive. `LoadError::Parse` is
  `{ pos: Pos, message: String }`.
- Both types are exported from `rlsp-yaml-parser/src/lib.rs:24,27`.

### Categorization is encoded only in message text

The parser internally distinguishes character-set
violations from grammar violations, but that taxonomy is
destroyed at the API boundary — it survives only as a
substring of `Error.message`. The 4+ distinct wordings
that all describe character violations:

- `chars.rs:163-171` — `non_printable_error_message(ch, context)`
  produces `"non-printable character U+XXXX is not allowed in <context>"`
  (the central helper).
- `lexer/quoted.rs:707` — `"escape produces non-printable character U+XXXX"`.
- `event_iter/directives.rs:102, 126, 169, 247` —
  `"directive name contains non-printable character U+XXXX"` and
  `"directive parameter contains non-printable character U+XXXX"`.
- `lexer/plain.rs:1156` — non-printable detection in plain scalars.
- `lexer/comment.rs` — non-printable in comments via the central helper.

Every consumer that needs to discriminate today must
substring-match `"non-printable character U+"`.

### Construction-site count

`grep -E "Error \{" /workspace/rlsp-yaml-parser/src/`
returns 167 sites (excluding the struct definition).
Distribution:

| File | Sites |
|------|-------|
| `lexer/quoted.rs` | 29 |
| `event_iter/flow.rs` | 27 |
| `event_iter/directives.rs` | 23 |
| `event_iter/block/mapping.rs` | 18 |
| `event_iter/step.rs` | 17 |
| `lexer/block.rs` | 14 |
| `event_iter/properties.rs` | 12 |
| `event_iter/directive_scope.rs` | 7 |
| `event_iter/block/sequence.rs` | 6 |
| `lexer/plain.rs` | 5 |
| `lexer.rs` | 4 |
| `loader/stream.rs` | 3 |
| `event_iter/state.rs` | 3 |
| `lexer/comment.rs` | 2 |
| `event_iter/base.rs` | 2 |

### External readers of the affected types

Verified by grep across the workspace:

- `rlsp-yaml/src/parser.rs:35-52` exhaustively matches
  `LoadError` and destructures `Parse { pos, message }`
  without `..`. Both patterns must be relaxed once the
  types become `#[non_exhaustive]`.
- `rlsp-yaml-parser/tests/error_reporting.rs` uses
  `matches!(load(input), Err(LoadError::Parse { .. }))`
  (already uses `..`) and reads `err.message.is_empty()`.
  Compatible after the change.
- `rlsp-yaml-parser/tests/loader.rs` uses `matches!`
  patterns with `..` for several variants; constructs
  `LoadError::CircularAlias { name, pos }` at line 1816.
  Variant construction stays valid because we mark only
  `LoadError::Parse` as variant-level `#[non_exhaustive]`,
  not the other variants. The enum-level marker affects
  matching, not construction.
- 12+ internal tests in `lexer/block.rs`,
  `lexer/comment.rs`, `lexer/plain.rs` assert
  `e.message.contains("non-printable")`. These are
  internal to the parser crate (same-crate tests) and
  unaffected by `#[non_exhaustive]`, but should be
  migrated to assert `e.kind == ErrorKind::InvalidCharacter`
  to remove the message-substring contract.
- 4 internal sites in `loader.rs:402, 568, 683` and
  `loader/stream.rs:34` already destructure
  `e.message` when forwarding `Error` to
  `LoadError::Parse`. These need `kind: e.kind` added —
  done in Task 3, not Task 2 (the field they would
  forward to does not exist on `LoadError::Parse` until
  Task 3 adds it).
- `rlsp-yaml-parser/docs/feature-log.md:93` carries an
  incidental parenthetical reference
  `(LoadError::Parse { pos: Pos })` inside the
  `Span`-shrink entry. The entry documents `Span`, not
  `LoadError::Parse`, but the parenthetical lists the
  variant's fields and goes stale after Task 3.

No other external readers of `Error.message`,
`Error.pos`, or `LoadError::Parse` fields.

### External constructions of `LoadError::Parse`

A grep across `rlsp-yaml-parser/tests/`,
`rlsp-yaml-parser/benches/`, `rlsp-yaml/src/`, and
`rlsp-yaml/tests/` confirms zero external `LoadError::Parse { ... }`
struct-literal constructions — every external use is a
pattern-match (with `..` rest already in two of three
test sites; the LSP destructure is the only exhaustive
one and is fixed in Task 1). Variant-level
`#[non_exhaustive]` on `LoadError::Parse` therefore
breaks no existing external site.

### Specifications and reference implementations

- Rust language reference,
  [`#[non_exhaustive]`](https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute)
  — applied to enum: external matches must include a
  wildcard arm but variant construction is unaffected;
  applied to variant: external code cannot construct it
  with a struct expression and must use `..` when
  matching; applied to struct: external code cannot
  construct it with a struct expression.

### Decision principle

The user's principle: the parser API may change when the
change is meaningful for any consumer, not just one. Both
proposed changes pass that test:

- `#[non_exhaustive]` is universal forward-compat hygiene
  — every consumer benefits from being insulated against
  internal evolution.
- `ErrorKind` is typed discrimination — every
  error-surfacing consumer benefits (Python bindings →
  exception types, WASM bindings → categorized error
  objects, CLI → exit codes by class, validators →
  category filters, language servers → diagnostic codes).

The starting taxonomy (`InvalidCharacter`, `Syntax`) comes
from what the parser already structurally distinguishes
at construction sites — not from any one consumer's
wishlist.

## Steps

- [x] Mark `Error`, `LoadError`, and `LoadError::Parse`
  `#[non_exhaustive]`; add LSP-side compile-fix
  (`..` rest pattern + `_ =>` wildcard arm)
- [x] Introduce `pub enum ErrorKind` and `kind: ErrorKind`
  field on `Error`; add `Error::syntax(...)` and
  `Error::invalid_character(...)` constructors; migrate
  all internal `Error { pos, message }` constructions to
  the constructors; populate `InvalidCharacter` at the
  character-violation sites
- [ ] Add `kind: ErrorKind` field on `LoadError::Parse`;
  forward `kind: e.kind` at the four internal conversion
  sites; migrate internal substring-message tests to
  typed `kind` assertions; add an integration test on
  the `load()` API asserting kind propagation

## Tasks

### Task 1: Add `#[non_exhaustive]` markers and LSP compile-fix

Mark `Error` (struct), `LoadError` (enum), and
`LoadError::Parse` (variant) `#[non_exhaustive]` so future
field additions (e.g., the `kind` field added in Task 2)
and future variant additions do not break external
consumers' destructure or match patterns. Update the only
known external consumer's match (`rlsp-yaml/src/parser.rs`)
to use `..` rest pattern on the `Parse` destructure and a
`_ =>` wildcard fallback on the enum match.

This task ships independent value: forward-compat hygiene.
It requires no semantic changes — adding the markers
alone does not change runtime behavior, only future
flexibility. Tasks 2 and 3 then add fields without
breaking anyone.

- [x] Add `#[non_exhaustive]` to `Error` in
  `rlsp-yaml-parser/src/error.rs`
- [x] Add `#[non_exhaustive]` to `LoadError` enum and to
  `LoadError::Parse` variant in
  `rlsp-yaml-parser/src/loader.rs`
- [x] Update `rlsp-yaml/src/parser.rs:36` from
  `Parse { pos, message }` to `Parse { pos, message, .. }`
- [x] Add `_ => (rlsp_yaml_parser::Pos::ORIGIN, err.to_string())`
  wildcard arm at the end of the match in
  `rlsp-yaml/src/parser.rs`
- [x] `cargo fmt`, `cargo clippy --all-targets --workspace`,
  `cargo build --workspace`, `cargo test --workspace`
  all pass with zero warnings

Acceptance: `Error`, `LoadError`, and `LoadError::Parse`
in `rlsp-yaml-parser/src/{error,loader}.rs` each carry
the `#[non_exhaustive]` attribute; the
`rlsp-yaml/src/parser.rs` match compiles with the new
attributes applied; the convention this task encodes is
already documented in `rlsp-yaml-parser/CLAUDE.md` under
the existing `## Conventions` heading and is the rule
this task implements.

**Commit:** dbade5b

### Task 2: Introduce ErrorKind on the event-stream API

Add `pub enum ErrorKind` with `InvalidCharacter` and
`Syntax` variants (also `#[non_exhaustive]`), add
`kind: ErrorKind` field to `Error`, add helper
constructors `Error::syntax(pos, message)` and
`Error::invalid_character(pos, message)`. Migrate all 167
internal `Error { pos, message }` constructions to use
the appropriate constructor — `Error::invalid_character`
at the character-violation sites listed in Context, and
`Error::syntax` everywhere else.

After this task, external consumers of the `parse_events()`
event-stream API can match on `e.kind` to discriminate
parse-error categories without substring-matching
`e.message`. The loader API still exposes only the legacy
`{ pos, message }` view of `LoadError::Parse` — that ships
in Task 3.

The four internal `Error → LoadError::Parse` conversion
sites (`loader.rs:402, 568, 683` and
`loader/stream.rs:34`) are NOT migrated in this task —
they continue to construct `LoadError::Parse { pos, message }`
without forwarding the new `kind` field. This is
intentional: `LoadError::Parse` does not yet have a
`kind` field to receive it. Task 3 adds the field and
wires the four sites in one step. Workspace builds
remain green at end of Task 2 because no external
`LoadError::Parse` shape change has happened yet.

- [x] Add `pub enum ErrorKind` (`#[non_exhaustive]`,
  derives `Debug, Clone, PartialEq, Eq`) with variants
  `InvalidCharacter` and `Syntax` in
  `rlsp-yaml-parser/src/error.rs`
- [x] Add `kind: ErrorKind` field to `Error`
- [x] Add `pub use error::ErrorKind;` to
  `rlsp-yaml-parser/src/lib.rs`
- [x] Add `Error::syntax(pos: Pos, message: String) -> Error`
  and `Error::invalid_character(pos: Pos, message: String) -> Error`
  constructor helpers
- [x] Migrate every `Error { pos, message }` literal
  construction in `rlsp-yaml-parser/src/` to one of the
  two constructors
- [x] Use `Error::invalid_character` at the
  character-violation sites:
  - `chars.rs::non_printable_error_message` callers
    (`lexer/comment.rs`, `lexer/plain.rs`,
    `lexer/block.rs`, and any other site invoking
    `non_printable_error_message`)
  - `lexer/quoted.rs:707` ("escape produces non-printable
    character")
  - `event_iter/directives.rs:102, 126, 169, 247`
    (directive name and parameter non-printable)
- [x] Use `Error::syntax` at all other sites
- [x] Add inline tests in `rlsp-yaml-parser/src/error.rs`
  asserting representative inputs produce the expected
  `kind`: comment with NUL → `InvalidCharacter`,
  directive parameter with non-printable →
  `InvalidCharacter`, escape `\x07` →
  `InvalidCharacter`, unclosed flow sequence → `Syntax`,
  bad indent → `Syntax`
- [x] `cargo fmt`, `cargo clippy --all-targets --workspace`,
  `cargo build --workspace`, `cargo test --workspace`
  all pass with zero warnings

Acceptance: `Error` carries a `kind: ErrorKind` field
populated at every construction site; the character
violations enumerated in Context all produce
`ErrorKind::InvalidCharacter`; all other parse failures
produce `ErrorKind::Syntax`; new tests prove the kind is
set correctly for at least one representative input per
character-violation site listed in Context.

**Commit:** 6f4f003

### Task 3: Propagate kind through `LoadError::Parse`

Add `kind: ErrorKind` to `LoadError::Parse`. At the four
internal conversion sites where parser `Error` becomes
`LoadError::Parse` (`loader.rs:402, 568, 683` and
`loader/stream.rs:34`), forward `kind: e.kind`. Migrate
the 12+ internal tests that assert
`e.message.contains("non-printable")` to assert on
`e.kind == ErrorKind::InvalidCharacter` — this removes a
fragile substring contract from the parser's own test
suite. Add a single integration test on the `load()` API
proving the kind propagates through to `LoadError::Parse`.

After this task, both parser APIs (`parse_events()` and
`load()`) expose typed error discrimination. The LSP
layer's destructure (already updated to use `..` in
Task 1) absorbs the new field automatically.

- [ ] Add `kind: ErrorKind` field to `LoadError::Parse`
  variant in `rlsp-yaml-parser/src/loader.rs`
- [ ] Update conversion site at `loader.rs:402` to forward
  `kind: e.kind`
- [ ] Update conversion site at `loader.rs:568` to forward
  `kind: e.kind`
- [ ] Update conversion site at `loader.rs:683` to forward
  `kind: e.kind`
- [ ] Update conversion site at `loader/stream.rs:34`
  (and the surrounding match) to forward `kind: e.kind`
- [ ] Migrate internal tests in `lexer/block.rs`,
  `lexer/comment.rs`, `lexer/plain.rs` from
  `e.message.contains("non-printable")` to
  `e.kind == ErrorKind::InvalidCharacter` — every
  occurrence in those three files
- [ ] Add an integration test in
  `rlsp-yaml-parser/tests/error_reporting.rs`: feed a
  comment containing U+0080 to `load()`, assert
  `Err(LoadError::Parse { kind: ErrorKind::InvalidCharacter, .. })`
- [ ] Update the parenthetical
  `(LoadError::Parse { pos: Pos })` in
  `rlsp-yaml-parser/docs/feature-log.md:93` to
  `(LoadError::Parse.pos)` — keeps the entry's intent
  (noting `Pos` is retained for error reporting) without
  inlining a struct shape that re-stales every time the
  variant gains a field
- [ ] Verify `rlsp-yaml/src/parser.rs` continues to
  compile without further edit (the `..` from Task 1
  already absorbs the new field)
- [ ] `cargo fmt`, `cargo clippy --all-targets --workspace`,
  `cargo build --workspace`, `cargo test --workspace`
  all pass with zero warnings

Acceptance: `LoadError::Parse` carries the `kind` field;
all four conversion sites forward `e.kind` from the
underlying `Error`; the internal substring-message
assertions in `lexer/block.rs`, `lexer/comment.rs`,
`lexer/plain.rs` are replaced with typed `kind`
assertions; the new `tests/error_reporting.rs` integration
test passes; the rlsp-yaml workspace builds without
modification beyond Task 1's compile-fix.

## Decisions

- **Keep `String message` alongside `kind: ErrorKind`** —
  the message provides human-readable detail (the
  specific U+XXXX, the context "comment" vs "directive
  name") that the kind does not encode. Both are useful:
  `kind` for routing, `message` for display.
- **Variant-level `#[non_exhaustive]` only on `Parse`** —
  it is the only variant gaining a field in this plan.
  Other variants (`*LimitExceeded`, `CircularAlias`,
  `UndefinedAlias`, `UnresolvedScalar`,
  `UnexpectedEndOfStream`) keep stable shapes; marking
  them proactively adds noise without value. New
  field-bearing variants in the future will be marked at
  the time they gain extensible fields.
- **Minimum kind taxonomy: `InvalidCharacter` and
  `Syntax`** — these are the categories the parser
  already structurally distinguishes at error sites.
  Adding finer kinds (Indent, Tag, Schema, Directive)
  before the parser's internal structure justifies the
  separation creates artificial categorization. Future
  kinds added when parser changes structurally separate
  them.
- **LSP layer changes are compile-fix only** — actual
  diagnostic-code mapping (`ErrorKind::InvalidCharacter`
  → `"invalidCharacter"`) is the language-server
  follow-up tracked in
  `.ai/memory/project_followup_plans.md` and is
  out of scope here.
- **No retrofit of other public types** — the
  `#[non_exhaustive]` convention added to
  `rlsp-yaml-parser/CLAUDE.md` will drive future
  retrofits of `Schema`, `ResolvedTag`, `Event`, `Node`,
  and other public types as they are touched. Doing them
  all in this plan would balloon scope and mix
  unrelated changes.

## Non-Goals

- Mapping `ErrorKind::InvalidCharacter` to a distinct LSP
  diagnostic code (e.g., `"invalidCharacter"`) in
  `rlsp-yaml/src/parser.rs` — that is the follow-up plan.
  This plan only ensures the parser API supports it.
- Retrofitting `#[non_exhaustive]` to other public parser
  types (`Schema`, `ResolvedTag`, `Encoding`,
  `EncodingError`, `Event`, `EventMeta`, `ScalarStyle`,
  `Chomp`, `CollectionStyle`, `Document`, `NodeMeta`,
  `Node`, `Pos`, `LineIndex`, `Span`, `LoadMode`,
  `LoaderOptions`, `BreakType`, `Line`, `LineBuffer`).
  The convention added here will drive these as they are
  touched.
- Adding `ErrorKind` variants beyond `InvalidCharacter`
  and `Syntax` (e.g., `Indent`, `Tag`, `Schema`,
  `Directive`).
- Changing the message text or wording at any error
  construction site. Wordings stay byte-identical to
  preserve the existing message-substring tests outside
  the three lexer test files migrated in Task 3.
- Touching `EncodingError` in `encoding.rs` — it is a
  separate error type with its own consumers and is out
  of scope.
- Editing any `CLAUDE.md` file — the
  `rlsp-yaml-parser/CLAUDE.md` Conventions section and
  the `#[non_exhaustive]` convention entry were added
  ahead of this plan as a separate lead-side change;
  this plan only implements that convention against the
  parser's error types.
