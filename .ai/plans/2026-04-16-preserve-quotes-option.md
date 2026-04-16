**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-16

# Plan: `preserveQuotes` formatter option

## Goal

Stop the formatter from rewriting a user's YAML quote
choices. Driven by a concrete report: Kubernetes manifests
authored with `command: ["python", "-m", ...]` come out of
the formatter as `command: [python, "-m", ...]` — the user
deliberately quoted every command argument for type-safety
clarity, and the formatter silently dropped the quotes it
considered redundant.

Deliver a `preserveQuotes` option that, when `true`, keeps
each scalar's source style verbatim: double-quoted stays
double-quoted, single-quoted stays single-quoted, plain
stays plain. Spec-forced cases (values containing control
characters, backslash-escape-requiring content, NEL/LS/PS)
still emit as double-quoted regardless — the YAML 1.2 spec
overrules user preference. When `false` (the default),
current behavior is unchanged: the formatter strips quotes
that `needs_quoting` deems unnecessary.

This is the feature yaml-language-server users asked for
in issue #246 and Prettier users in discussion #16956 —
"just keep my quotes as I wrote them." Neither project has
delivered it; this is new ground, not a port.

## Context

### The observed behavior

User input to the formatter:

```yaml
command: ["python", "-m", "http.server", "5000"]
```

Formatter output:

```yaml
command: [python, "-m", http.server, "5000"]
```

`"python"` and `"http.server"` are safe as plain scalars
and lose their quotes; `"-m"` and `"5000"` keep their
quotes because `needs_quoting` flags them (`-` is a
reserved leading character, `5000` looks like a number).
The output is semantically equivalent to the input, but
the mixed style is unwelcome — the user made a
type-safety choice and the formatter quietly reversed it
where it thought the choice didn't matter.

### The current decision point

`ScalarStyle::SingleQuoted | DoubleQuoted` arm at
`formatter.rs:549-566`:

```rust
if requires_double_quoting(value) {
    text(format!("\"{}\"", escape_double_quoted(value)))
} else if needs_quoting(value, options.yaml_version) {
    // preserve original quote style
} else {
    string_to_doc(value, options, in_key) // strips quotes
}
```

The last branch — "safe-plain scalar, strip to plain" —
is the one that discards the user's choice. `preserveQuotes`
adds a conditional detour: if the option is on, emit in
the original style rather than routing through
`string_to_doc`.

### Proposed semantics

| Source scalar | Style | Output under `preserveQuotes: false` (default) | Output under `preserveQuotes: true` |
|---|---|---|---|
| `"python"` | DoubleQuoted | `python` | `"python"` |
| `'python'` | SingleQuoted | `python` | `'python'` |
| `python` | Plain | `python` | `python` |
| `"5000"` | DoubleQuoted | `"5000"` (forced) | `"5000"` (preserved, same) |
| `'5000'` | SingleQuoted | `'5000'` (forced) | `'5000'` (preserved, same) |
| `"foo\nbar"` | DoubleQuoted | `"foo\nbar"` (spec) | `"foo\nbar"` (spec overrides) |

Three invariants hold for `preserveQuotes: true`:

1. **Source style wins for safe-plain scalars.** The
   formatter no longer makes a stripping decision the user
   didn't ask for.
2. **Spec still wins on top.** `requires_double_quoting`
   values always emit as double-quoted — the spec bans
   them from plain and single-quoted forms.
3. **Block scalars are untouched.** Literal (`|`) and
   folded (`>`) styles are orthogonal to quote style and
   already handled by their own match arm.

### Interaction with `singleQuote`

`singleQuote` is a quote-character preference that applies
when the formatter has to *choose* a style. Under
`preserveQuotes: true` the formatter isn't choosing — it's
reproducing the source — so `singleQuote` rarely applies.
The one place both settings interact: a scalar the source
marked Plain but whose value happens to contain characters
that `requires_double_quoting` flags. Spec forces double
quotes there regardless of either setting. No other
interactions exist.

`singleQuote` continues to apply to safe-plain scalars
under `preserveQuotes: false` (current default behavior),
unchanged.

### Keys

Mapping keys follow the same preserve rule: a key that was
quoted in source stays quoted; a plain key stays plain.
The existing `in_key` suppression of `singleQuote` at
`formatter.rs:471-472` and `formatter.rs:736` still
applies — but since `preserveQuotes` reproduces original
style rather than choosing one, `in_key` does not gate
the new branch.

### Ecosystem positioning

This is the feature yaml-language-server (issue #246) and
Prettier (discussion #16956) users have been asking for
for years without delivery. No prior implementation exists
to model against — the design here is greenfield and
should be documented clearly so downstream integrators
understand the semantics.

### Files involved

- `rlsp-yaml/src/editing/formatter.rs` — add
  `preserve_quotes` field to `YamlFormatOptions`; add one
  new branch inside `ScalarStyle::SingleQuoted |
  DoubleQuoted` match arm at lines 549-566
- `rlsp-yaml/src/server.rs` — workspace settings
  deserialization and `YamlFormatOptions` construction at
  lines 62, 1041-1050, 1125-1134
- `rlsp-yaml/docs/configuration.md` — new
  `formatPreserveQuotes` section; cross-reference summary
  at line 477
- `rlsp-yaml/docs/feature-log.md` — line 227 lists
  `formatPrintWidth` and `formatSingleQuote`; append
  `formatPreserveQuotes`
- `rlsp-yaml/README.md` — three setup examples
  (Neovim/Lua line 39, Helix line 64, VS Code
  `settings.json` line 93) list `formatSingleQuote`; add
  `formatPreserveQuotes`
- `rlsp-yaml/tests/fixtures/formatter/` — new preserve
  fixtures only; no existing fixtures need updating
  because `singleQuote` semantics are unchanged
- `rlsp-yaml/integrations/vscode/package.json` — new
  `rlsp-yaml.formatPreserveQuotes` contribution
- `rlsp-yaml/integrations/vscode/src/config.ts` —
  `ServerSettings` field and reader
- `rlsp-yaml/integrations/vscode/src/test/integration/configuration.test.ts` —
  default-value test for the new setting
- `rlsp-yaml/integrations/vscode/README.md` — settings
  table entry
- `/workspace/.ai/memory/project_followup_plans.md` —
  remove the stale `preserve_quotes` follow-up entry
  (replaced by this plan) and the adjacent stale
  `single_quote` key-handling entry whose fix landed in
  the now-Completed `2026-04-14-formatter-bug-fixes.md`

### References

- YAML 1.2 spec, §7.3 Flow Scalar Styles — single-quoted
  scalars cannot contain escape sequences, double-quoted
  are required for control characters.
  <https://yaml.org/spec/1.2.2/#73-flow-scalar-styles>
- yaml-language-server issue #246 — "Disable replacing
  quotes" — open since 2020, unresolved.
  <https://github.com/redhat-developer/yaml-language-server/issues/246>
- Prettier discussion #16956 — same request, undelivered.
  <https://github.com/prettier/prettier/discussions/16956>

## Steps

- [x] Clarify requirements with the user (feature shape,
      default, keys, scope) — done
- [ ] Add `preserve_quotes` field to `YamlFormatOptions`
      and wire through workspace settings and VS Code
      extension
- [ ] Add the preserve branch to the scalar emission logic
- [ ] Add fixture coverage for the new option and its
      interactions with other formatter settings
- [ ] Update documentation (`docs/configuration.md`,
      `docs/feature-log.md`, `README.md`, VS Code
      extension README)
- [ ] Verify formatter round-trip on the Kubernetes
      manifest example from the user's report

## Tasks

### Task 1: Add `preserve_quotes` option and wire it through

Add the option to the Rust source-of-truth and every
consumer so the setting is visible end-to-end before any
behavior changes. Landing the plumbing first keeps the
second task focused on the emission branch.

- [ ] Add `pub preserve_quotes: bool` to `YamlFormatOptions`
      in `rlsp-yaml/src/editing/formatter.rs`, default
      `false` in the `Default` impl and the doc comment
- [ ] Add `pub format_preserve_quotes: Option<bool>` to
      the workspace settings struct in
      `rlsp-yaml/src/server.rs` with a doc comment
- [ ] Read `format_preserve_quotes` into
      `YamlFormatOptions` in both formatter entry points
      at `server.rs:1041-1050` and `server.rs:1125-1134`
- [ ] Add `rlsp-yaml.formatPreserveQuotes` contribution
      (boolean, default `false`, description explaining
      it keeps the source scalar style) to
      `rlsp-yaml/integrations/vscode/package.json`
- [ ] Add `formatPreserveQuotes: boolean` field and
      `cfg.get('formatPreserveQuotes', false)` reader to
      `rlsp-yaml/integrations/vscode/src/config.ts`
- [ ] Add a `formatPreserveQuotes defaults to false`
      integration test to
      `rlsp-yaml/integrations/vscode/src/test/integration/configuration.test.ts`
- [ ] Add settings deserialization test in `server.rs`
      (parallel to `settings_deserializes_format_single_quote`)
- [ ] `cargo fmt`, `cargo clippy --all-targets`,
      `cargo build`, `cargo test` all clean
- [ ] VS Code extension `pnpm run lint`,
      `pnpm run format`, `pnpm run build`,
      `pnpm run test`, `pnpm run test:integration` all
      clean

### Task 2: Add preserve branch to scalar emission logic

Honor the preserve option in the formatter. This is the
only task that changes output for existing users — and
only users who explicitly set `preserveQuotes: true`.

- [ ] In `formatter.rs`, inside the
      `ScalarStyle::SingleQuoted | DoubleQuoted` match arm
      at lines 549-566, add a conditional branch between
      the existing `needs_quoting` and `string_to_doc`
      branches: when `options.preserve_quotes` is `true`
      and neither `requires_double_quoting` nor
      `needs_quoting` applies, emit the value in its
      original style (`'...'` for SingleQuoted with
      embedded-single-quote doubling; `"..."` for
      DoubleQuoted with `escape_double_quoted`)
- [ ] Leave the `ScalarStyle::Plain` arm at
      `formatter.rs:567-580` untouched — plain scalars
      remain plain under preserve, which is already the
      behavior
- [ ] Leave `string_to_doc` at `formatter.rs:727-742`
      untouched — `singleQuote` semantics are unchanged
- [ ] Leave `requires_double_quoting` gate and the
      `needs_quoting=true` preservation branch untouched
      — spec-forced double quoting and the
      already-quoted-stays-original-style behavior both
      continue to work
- [ ] Consult the test-engineer before implementing for a
      test list covering: `preserveQuotes: true` × all
      three source `ScalarStyle` variants (SingleQuoted,
      DoubleQuoted, Plain) × safe-plain × needs-quoting ×
      requires-double × keys vs values × flow vs block
      containers
- [ ] `cargo fmt`, `cargo clippy --all-targets`,
      `cargo build`, `cargo test` all clean
- [ ] Get test-engineer output-gate sign-off before
      submitting to the reviewer

### Task 3: Fixture coverage

Add fixtures that cover the preserve option and its
interactions with other formatter settings. No existing
fixture needs modification — `singleQuote` semantics are
unchanged.

- [ ] Add new fixtures under
      `rlsp-yaml/tests/fixtures/formatter/`:
      - `preserve-quotes-double-safe-plain-kept.md` —
        `preserveQuotes: true`, source `key: "python"`,
        expected `key: "python"` (not stripped)
      - `preserve-quotes-single-safe-plain-kept.md` —
        `preserveQuotes: true`, source `key: 'python'`,
        expected `key: 'python'`
      - `preserve-quotes-plain-stays-plain.md` —
        `preserveQuotes: true`, source `key: python`,
        expected `key: python` (not wrapped)
      - `preserve-quotes-off-strips-as-before.md` —
        `preserveQuotes: false` (default), source
        `key: "python"`, expected `key: python` —
        confirms default behavior unchanged
      - `preserve-quotes-forced-double-still-wins.md` —
        `preserveQuotes: true`, source value containing
        `\n` as literal content, expected double-quoted
        output regardless of source style (spec)
      - `preserve-quotes-keys-kept-quoted.md` —
        `preserveQuotes: true`, source `"key": value`,
        expected `"key": value` (keys also preserved)
      - `preserve-quotes-flow-sequence-kubernetes.md` —
        the exact command-array case from the user
        report: source
        `command: ["python", "-m", "http.server", "5000"]`,
        settings `preserveQuotes: true`, expected output
        identical — demonstrates the fix
- [ ] Add interaction fixtures (per
      `rlsp-yaml/tests/fixtures/CLAUDE.md`):
      - `interact-preserve-quotes-single-quote.md` —
        `preserveQuotes: true` with `singleQuote: true`:
        source mix of `"a"` and `'b'` and `c` all stay
        as-is; `singleQuote` does not rewrite
        already-styled scalars
      - `interact-preserve-quotes-enforce-block-style.md`
        — `preserveQuotes: true` with
        `formatEnforceBlockStyle: true`: flow collections
        convert to block, but scalar quote styles inside
        them are preserved
      - `interact-preserve-quotes-yaml-version-v1-1.md` —
        YAML 1.1 reserved keywords (`yes`, `no`, etc.)
        with explicit quotes stay quoted under preserve
- [ ] `cargo test` passes all fixtures
- [ ] Confirm idempotence — each new fixture's
      Expected-Document formats back to itself unchanged

### Task 4: Documentation updates

Document the new option, its interaction with
`singleQuote`, and the spec-forced override.

- [ ] Add `### formatPreserveQuotes` section to
      `rlsp-yaml/docs/configuration.md` — type, default
      (`false`), description making clear that `true`
      reproduces the source scalar style (quoted stays
      quoted, plain stays plain) while spec-forced double
      quoting overrides
- [ ] Include in the new section the "Source scalar →
      output" table from this plan's Context so users
      can see concrete expectations
- [ ] Update the cross-reference summary at
      `docs/configuration.md:477` — append
      `formatPreserveQuotes` to the settings list
- [ ] Update `rlsp-yaml/docs/feature-log.md:227` — append
      `formatPreserveQuotes` to the configurable
      settings list
- [ ] Update `rlsp-yaml/README.md` — three setup examples
      at lines 39 (Neovim/Lua), 64 (Helix), and 93 (VS
      Code `settings.json`) list `formatSingleQuote`; add
      `formatPreserveQuotes` with its default value
      (`false`) to each example
- [ ] Update `rlsp-yaml/integrations/vscode/README.md`
      settings table — add `formatPreserveQuotes` row
- [ ] Remove the `preserve_quotes` follow-up entry from
      `/workspace/.ai/memory/project_followup_plans.md`
      — the item is delivered by this plan (preserve
      semantics, values and keys both preserved by
      reproducing source style)
- [ ] Remove the adjacent stale entry "Formatter:
      `single_quote` quotes keys unnecessarily — …Being
      fixed in current plan
      `2026-04-14-formatter-bug-fixes.md`" from the same
      memory file. That plan is now Completed
      (2026-04-14); cleaning both entries in one pass
      keeps the follow-up queue accurate
- [ ] `cargo test` (keep the gate even though no doctests
      here)

### Task 5: Round-trip verification on user's example

Close the loop — the plan was opened by a concrete
Kubernetes manifest; verify the fix applies end-to-end
through the VS Code integration path, not just unit-level
fixtures.

- [ ] Add an integration test that takes the full
      Deployment YAML from the user's report as input,
      formats it with `preserveQuotes: true`
      (`singleQuote` left at default `false`), and
      asserts the `command` array comes out as
      `["python", "-m", "http.server", "5000"]` — no
      quote removal, no mixed styles
- [ ] Confirm the rest of the Deployment (Namespace
      metadata, Deployment metadata, selector labels,
      probe fields) round-trips byte-for-byte identical
      when already correctly formatted — preserve must
      not regress unrelated output
- [ ] Confirm idempotence — formatting the output a
      second time produces the same text
- [ ] `cargo test` and VS Code integration tests pass

## Decisions

- **Feature is preserve, not force/enforce** — user's
  original request (message 3) and the ecosystem demand
  (yaml-ls #246, Prettier #16956) both describe preserve:
  "keep my quote choices as written." The mid-discussion
  description of "add the selected quote style to all
  string scalars" was a force variant; confirmed via
  `AskUserQuestion` that preserve is the intent.
- **`preserveQuotes` defaults to `false`** — confirmed
  via `AskUserQuestion`. Default behavior (minimal
  quoting, current stripping) stays the default. Users
  who care about quote preservation opt in.
- **Spec-forced double quoting overrides preserve** —
  values requiring double quotes per YAML 1.2 §7.3
  (control chars, backslash-escape content, NEL/LS/PS)
  emit as double-quoted regardless of source style or
  `preserveQuotes`. Attempting to honor the source style
  here would produce unparseable output.
- **Keys are preserved the same as values** — there's no
  value-only carve-out; a quoted key stays quoted, a
  plain key stays plain. This differs from `singleQuote`
  (which has a `in_key` suppression) because preserve
  doesn't *choose* a style — it reproduces the source —
  so the suppression rule doesn't apply.
- **`singleQuote` semantics are unchanged** — no
  backward-compat break. The eight fixtures that exercise
  `single_quote: true` keep passing without modification.
  `singleQuote` is a quote-character preference for
  cases where the formatter has to pick a style;
  `preserveQuotes: true` means the formatter isn't
  picking, so the two settings rarely interact.
- **Already-quoted-needs-quoting bug is still a separate
  plan** — the `formatter.rs:557-562` inconsistency
  (where `"-m"` stays double-quoted even under
  `singleQuote: true`) is orthogonal to preserve and
  tracked for a follow-up plan.
- **Scope is language-server + VS Code integration only**
  — `rlsp-yaml-parser` and `rlsp-fmt` are not touched.
  The parser already preserves `ScalarStyle` natively;
  the emission decision is entirely in the formatter.
