**Repository:** root
**Status:** InProgress
**Created:** 2026-04-13

## Goal

Add YAML 1.1/1.2 compatibility diagnostics, quick fixes,
and schema-aware severity escalation to rlsp-yaml. Users
writing YAML consumed by tools with YAML 1.1 parsers
(Kubernetes, Ansible, GitLab CI, etc.) get zero warning
today when values like `yes`, `no`, `on`, `off`, or `0777`
will be interpreted differently. This feature bridges that
gap — the language server sees the source before any
downstream parser, making it the ideal place to catch
ambiguities that cause silent data corruption or hard
runtime errors.

## Context

### The ecosystem problem

~80% of YAML-consuming tools use YAML 1.1 parsers (go-yaml
v2, PyYAML, SnakeYAML, libyaml/Psych). Our parser is YAML
1.2 (spec-correct). Values like `yes`/`no`/`on`/`off` are
strings in 1.2 but booleans in 1.1. `0777` is a string in
1.2 but octal 511 in 1.1. Every major ecosystem (Ansible
Steering Committee, Kubernetes docs, Home Assistant style
guide) recommends `true`/`false` only, but enforcement is
spotty.

Red Hat's yaml-language-server has a `yamlVersion` toggle
that changes the parser's behavior (eemeli/yaml supports
both), but provides zero cross-version diagnostics — no
warnings for ambiguous values, no quick fixes, no
migration assistance. Their issue #532 (open since Aug
2021) requests exactly the quick fixes we're building.

### What exists in our codebase

- **Parser**: Pure YAML 1.2, emits scalars as strings
  (`rlsp-yaml-parser`)
- **Type inference**: `scalar_helpers.rs` —
  `classify_plain_scalar()` per YAML 1.2 Core Schema.
  `yes`/`no`/`on`/`off` classify as `String`.
- **Formatter**: `formatter.rs:445` `needs_quoting()` is
  version-aware — quotes 1.1 keywords when
  `yamlVersion=1.1`
- **Settings**: `yamlVersion` exists (default `1.2`,
  modeline override `$yamlVersion=1.1`), currently affects
  formatting only
- **Suppression**: `# rlsp-yaml-disable-next-line [codes]`
  and `# rlsp-yaml-disable-file [codes]` fully built
- **Schema validation**: Full JSON Schema support;
  `yaml_type_name()` uses `classify_plain_scalar()`;
  type mismatches produce `schemaType` errors
- **Code actions**: `code_actions.rs` handles
  diagnostic-driven and context-driven actions. Pattern:
  `diagnostic_code(diag)` match → generate `CodeAction`
  with `TextEdit`. Existing actions: flow→block,
  unused anchor delete, quoted bool→unquoted, tab→spaces,
  string→block scalar, block→flow.
- **VS Code extension**: `package.json` exposes 11
  settings. `yamlVersion` is NOT exposed. No
  diagnostic-level settings exposed.
- **Diagnostic pipeline**: `server.rs:370-466` —
  validators run, schema validation runs, then
  suppression filter applied.

### Key files

- `rlsp-yaml/src/scalar_helpers.rs` — type inference
- `rlsp-yaml/src/validation/validators.rs` — validators
- `rlsp-yaml/src/validation/suppression.rs` — suppression
- `rlsp-yaml/src/editing/code_actions.rs` — code actions
- `rlsp-yaml/src/schema_validation.rs` — schema validation
- `rlsp-yaml/src/server.rs` — diagnostic pipeline, settings
- `rlsp-yaml/src/editing/formatter.rs` — version-aware
  quoting
- `rlsp-yaml/integrations/vscode/package.json` — VS Code
  settings
- `rlsp-yaml/integrations/vscode/src/config.ts` — settings
  wiring

### Specifications

- [YAML 1.2.2 §10.3.2 Tag Resolution](https://yaml.org/spec/1.2.2/#tag-resolution)
- [YAML 1.1 Boolean Type](https://yaml.org/type/bool.html)
- [YAML 1.1 Integer Type](https://yaml.org/type/int.html)
  (C-style octal `0777`)

## Steps

- [x] Research YAML version usage across ecosystem
- [x] Analyze Red Hat yaml-language-server approach
- [x] Analyze existing codebase infrastructure
- [x] Design feature set and defaults
- [x] Add YAML 1.1 boolean detection helpers
- [x] Add `yaml11Boolean` validator and diagnostic
- [x] Add `yaml11Octal` validator and diagnostic
- [x] Add quick fixes for 1.1 booleans (quote + convert)
- [x] Add quick fixes for 1.1 octals (quote + convert)
- [x] Add schema-aware severity escalation for 1.1 values
- [x] Enhance `schemaType` message for 1.1 boolean in
  boolean-typed field
- [x] Wire `yamlVersion` setting to suppress/adjust
  diagnostics
- [x] Update VS Code extension settings
- [ ] Update documentation (feature-log, configuration)

## Tasks

### Task 1: Add YAML 1.1 boolean/octal detection helpers and `yaml11Boolean` validator

Add detection functions in `scalar_helpers.rs` for YAML 1.1
boolean forms (the 16 forms NOT in the 1.2 set: `yes`,
`Yes`, `YES`, `no`, `No`, `NO`, `on`, `On`, `ON`, `off`,
`Off`, `OFF`, `y`, `Y`, `n`, `N`) and YAML 1.1 C-style
octal patterns (`0[0-7]+`).

Add a new validator function in `validators.rs` that scans
the parsed AST for plain (unquoted) scalars matching the
1.1 boolean set and emits `yaml11Boolean` warnings. Add a
companion check for C-style octals emitting `yaml11Octal`
info diagnostics. Wire both into the diagnostic pipeline
in `server.rs` alongside existing validators. The
`yamlVersion` setting should suppress `yaml11Boolean` and
`yaml11Octal` when set to `1.1` — in that mode the user
has explicitly opted into 1.1 semantics, so these values
are intentional.

Diagnostic messages should explain the 1.1/1.2 difference:
- `yaml11Boolean`: `"yes" is a boolean in YAML 1.1 but a
  string in YAML 1.2. Most tools use 1.1 parsers and will
  interpret this as true. Quote it ("yes") or use true.`
- `yaml11Octal`: `"0777" is octal 511 in YAML 1.1 but the
  string "0777" in YAML 1.2. Quote it ("0777") or use
  0o777 (YAML 1.2 only).`

Include unit tests and integration tests exercising
diagnostics through the server handler.

- [x] `is_yaml11_bool(value) -> bool` in `scalar_helpers.rs`
- [x] `is_yaml11_octal(value) -> bool` in `scalar_helpers.rs`
- [x] `yaml11_bool_canonical(value) -> &str` mapping
  (`yes`→`true`, `no`→`false`, etc.)
- [x] New validator: `validate_yaml11_compat()` in
  `validators.rs`
- [x] Wire into `server.rs` diagnostic pipeline with
  `yamlVersion` gating
- [x] Unit tests for detection helpers
- [x] Unit tests for validator (plain vs quoted, 1.1 vs
  1.2 mode)
- [x] Integration test through server handler

Commit: a7a47a6

### Task 2: Add quick fixes for YAML 1.1 booleans and octals

Add diagnostic-driven code actions in `code_actions.rs`
for `yaml11Boolean` and `yaml11Octal` diagnostics.

For `yaml11Boolean`, two quick fixes:
1. **"Quote value"** (`CodeActionKind::QUICKFIX`) — wraps
   the value in double quotes: `yes` → `"yes"`. This is
   the universally safe fix (identical in all parsers).
   Listed first in the UI.
2. **"Convert to boolean"** (`CodeActionKind::QUICKFIX`) —
   converts to canonical form: `yes` → `true`,
   `no` → `false`, `on` → `true`, `off` → `false`, etc.

For `yaml11Octal`, two quick fixes:
1. **"Quote as string"** (`CodeActionKind::QUICKFIX`) —
   wraps in double quotes: `0777` → `"0777"`. The
   universally safe fix. Listed first.
2. **"Convert to YAML 1.2 octal"**
   (`CodeActionKind::QUICKFIX`) — converts to 1.2 syntax:
   `0777` → `0o777`. Note: only correct if the downstream
   consumer uses a 1.2 parser.

Follow the existing pattern in `code_actions.rs`:
diagnostic-driven actions matched via `diagnostic_code()`
in the `diag_actions` iterator.

- [x] `yaml11Boolean` → "Quote value" code action
- [x] `yaml11Boolean` → "Convert to boolean" code action
- [x] `yaml11Octal` → "Quote as string" code action
- [x] `yaml11Octal` → "Convert to YAML 1.2 octal" code
  action
- [x] Unit tests for all four code actions
- [x] Integration test through server handler

Commit: 2a0130d

### Task 3: Schema-aware severity escalation for YAML 1.1 values

Enhance schema validation to detect YAML 1.1 ambiguous
values in schema-typed fields and escalate/adjust severity.

Two scenarios:

**1. String-typed field with 1.1 boolean value:**
Schema expects `type: string`, user writes `yes` (plain).
Our 1.2 parser classifies `yes` as string → type check
passes → user gets zero warning today. But downstream 1.1
tools will interpret it as boolean → silent data corruption
(K8s ConfigMap stored as `"true"` instead of `"yes"`) or
hard error (K8s env value: `cannot unmarshal bool into
string`).

Add a post-type-check scan in `schema_validation.rs`: when
a plain scalar passes the string type check, test it
against `is_yaml11_bool()` and `is_yaml11_octal()`. If it
matches, emit a **warning** (not error — it IS valid in
1.2) with a message explaining the downstream risk. Use
new diagnostic codes `schemaYaml11Boolean` and
`schemaYaml11Octal` (distinct from the non-schema codes
in Task 1) so users can suppress schema-aware and
non-schema diagnostics independently.

**2. Boolean-typed field with 1.1 boolean value:**
Schema expects `type: boolean`, user writes `yes`. Our 1.2
parser classifies as string → existing `schemaType` error
fires: "expected boolean, got string". This is correct but
the message should explain WHY: `"yes" is not a boolean in
YAML 1.2. Use true instead. (In YAML 1.1, "yes" was a
boolean — your tool may expect 1.1 syntax.)` Add the same
quick fixes as Task 2 to the `schemaType` diagnostic when
the value matches `is_yaml11_bool()`.

Gate both on `yamlVersion != V1_1`.

- [x] Post-type-check scan for 1.1 values in string-typed
  fields
- [x] `schemaYaml11Boolean` diagnostic with warning severity
- [x] `schemaYaml11Octal` diagnostic with warning severity
- [x] Enhanced `schemaType` message for 1.1 bool in
  boolean-typed field
- [x] Quick fixes on `schemaYaml11Boolean`,
  `schemaYaml11Octal`, and enhanced `schemaType`
- [x] Unit tests for schema-aware diagnostics
- [x] Integration tests with K8s-style schemas (ConfigMap
  `.data` string field, boolean field)

Commit: ac0716d

### Task 4: Update VS Code extension settings

The VS Code extension (`integrations/vscode/`) currently
does not expose `yamlVersion` or any diagnostic-level
settings. Add the missing settings so users can configure
the new features from VS Code.

Settings to add to `package.json`
`contributes.configuration.properties`:

1. **`rlsp-yaml.yamlVersion`** — string enum `"1.1"` or
   `"1.2"`, default `"1.2"`. Description: "YAML
   specification version. Affects formatting quoting rules
   and YAML 1.1 compatibility diagnostics. When set to
   1.1, diagnostics for 1.1 boolean and octal values are
   suppressed."

2. **`rlsp-yaml.validate`** — boolean, default `true`.
   Description: "Enable or disable all YAML diagnostics."

Wire both through `ServerSettings` in `config.ts` and
`getConfig()`. Verify they propagate via
`initializationOptions` and `didChangeConfiguration`.

Run `pnpm run build`, `pnpm run lint`, `pnpm run test`
to verify the extension builds cleanly.

- [x] Add `rlsp-yaml.yamlVersion` to `package.json`
- [x] Add `rlsp-yaml.validate` to `package.json`
- [x] Update `ServerSettings` interface in `config.ts`
- [x] Update `getConfig()` in `config.ts`
- [x] `pnpm run build` passes
- [x] `pnpm run lint` passes
- [x] `pnpm run test` passes

Commit: 15b92fa

### Task 5: Update documentation

Update the project documentation to reflect the new
features.

- [ ] `docs/feature-log.md` — add entries for
  `yaml11Boolean`, `yaml11Octal`, schema-aware severity
  escalation, and the new quick fixes
- [ ] `docs/configuration.md` — document `yamlVersion`
  effect on diagnostics (currently only documents
  formatting effect); document new diagnostic codes and
  suppression
- [ ] `README.md` — add YAML 1.1 compatibility diagnostics
  to the features list if appropriate

## Decisions

- **Parse as 1.2, warn about 1.1 ambiguities** — our parser
  stays YAML 1.2. We don't add a 1.1 parsing mode. Instead,
  we warn when values would be interpreted differently by
  1.1 parsers. This is a novel approach — Red Hat's LS
  switches the parser, we bridge the gap.
- **Warning severity for 1.1 booleans, Info for 1.1 octals**
  — booleans cause more frequent and severe downstream bugs
  (Norway problem, ConfigMap type errors). Octals are rarer.
- **Quote-first quick fix ordering** — "Quote value"
  appears before "Convert to boolean/octal" because quoting
  is universally safe across all YAML versions. Converting
  to `0o777` only works with 1.2 consumers.
- **Separate diagnostic codes for schema vs non-schema** —
  `yaml11Boolean` (no schema context) vs
  `schemaYaml11Boolean` (schema says `type: string` but
  value is 1.1 bool). Users may want to suppress one
  without the other.
- **Suppress 1.1 diagnostics when `yamlVersion=1.1`** — the
  user has explicitly opted into 1.1 semantics. Warning
  about 1.1 behavior in 1.1 mode is noise.
- **No `%YAML` directive detection for suppression** — almost
  no real-world files include this directive, so the
  complexity is not justified. The modeline
  `$yamlVersion=1.1` serves the same purpose and is already
  built.
- **VS Code extension exposes `yamlVersion` and `validate`**
  — `yamlVersion` was already in the LS but not surfaced to
  VS Code users. `validate` is a common toggle that other
  language servers provide.
