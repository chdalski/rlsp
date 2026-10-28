**Repository:** root
**Status:** InProgress
**Created:** 2026-10-28

# A Wrongly Typed Setting Does Not Reset All Settings

## Goal

The server reads its workspace settings in one serde pass, so one
value of the wrong type — `kubernetesVersion: 1.29`,
`formatPrintWidth: "80"`, `hover: "yes"` — makes the whole read fail:
at initialization every setting falls back to its default, on a
configuration change the whole change is dropped, and nothing tells
the user. Read the settings field by field, so a wrongly typed field
falls back to its own default and a wrong entry in `customTags` or
`schemas` is dropped on its own, each with a Warning in the client
log, while every other setting applies.

## Context

- `rlsp-yaml/src/server.rs`: `Settings` (about 24 public fields,
  `#[derive(Default, Deserialize)]`,
  `#[serde(default, rename_all = "camelCase")]`) is read with
  `serde_json::from_value::<Settings>` in `initialize` (a failure
  becomes all defaults) and `did_change_configuration` (a failure
  drops the change).
- `Settings` is public as `rlsp_yaml::server::Settings`;
  `src/validation/settings.rs` (`ValidationSettings::from_settings`)
  is its other reader.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: every settings key name and every default is pinned by
      a test
- [ ] Task 2: `Settings` lives in its own module
- [ ] Task 3: settings are read field by field, with a Warning per
      rejected field or entry
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Pin every settings key name and default

Task 3 replaces the derived reader, whose `rename_all` produces the
camelCase key names, with hand-written key strings; four keys
(`formatValidation`, `formatBracketSpacing`, `formatIndentSequences`,
`formatRespectEditorconfig`) have no test today. Test-only apart from
an additive derive; behavior is unchanged.

- [x] `Settings` derives `PartialEq`
- [x] A unit test reads an object that sets every key to a non-default
      value and compares the result with a full `Settings` literal
      that names every field
- [x] A unit test reads `{}` and compares the result with
      `Settings::default()`
- [x] Both tests pass against the current derived reader
- [x] Build, tests, clippy, and fmt pass

### Task 2: Move `Settings` into its own module

Reading settings is a concern of its own beside the LSP handlers in
`server.rs`. Move `Settings` into a new module `src/settings.rs`;
`rlsp_yaml::server::Settings` keeps working through a re-export.
Behavior is unchanged.

- [ ] `Settings` is defined in `src/settings.rs`, and
      `rlsp_yaml::server::Settings` names it
- [ ] The unit tests that only read `Settings` from JSON live with it
      in `src/settings.rs`, with their assertions unchanged
- [ ] The tests of Task 1 and every other existing test pass with
      their assertions unchanged
- [ ] Build, tests, clippy, and fmt pass

### Task 3: Read settings field by field

`src/settings.rs` reads a JSON object one key at a time, each with
its field's own type, and returns the settings together with the
rejected fields and entries as data; a pure function turns the
rejections into Warning messages, and `store_settings` stores the
settings and logs the messages. `Deserialize for Settings` becomes a
hand-written implementation over this reader. `store_settings` in
`server.rs` is the single store path and already logs one Warning for
an invalid `kubernetesVersion` value through
`invalid_kubernetes_version_message`, `printable_log_message`, and
`MAX_LOGGED_SETTING_CHARS`.

- [ ] A key whose value has the wrong type for its field (including
      a number outside `usize` and a float for an integer field)
      leaves that field at its default and yields one rejection; a
      key that is absent or `null` leaves its field at its default
      and yields none; strings are not coerced to numbers or booleans
- [ ] In `customTags`, an entry that is not a string, `null`
      included, is dropped and yields one rejection while the other
      tags apply; in `schemas`, an entry whose value is not a string,
      `null` included, is dropped and yields one rejection while the
      other associations apply; a `customTags`
      value that is not an array, or a `schemas` value that is not an
      object, leaves the field at its default and yields one
      rejection
- [ ] Each rejection yields one Warning in the client log that names
      the setting, the entry where one is dropped (its index, or its
      key — for `schemas`, the schema URL passed through
      `loggable_url` so credentials are stripped), and the expected
      type, followed by the rejected value, truncated to
      `MAX_LOGGED_SETTING_CHARS` and sanitized with
      `printable_log_message` like the existing `kubernetesVersion`
      Warning; unit tests pin the messages without a client
- [ ] `kubernetesVersion` of the wrong type selects `master` with
      exactly one Warning, and an invalid `kubernetesVersion` string
      keeps its existing Warning
- [ ] At initialization, a settings object applies every valid field
      and sends the Warnings, and a missing or non-object payload
      applies the defaults without a Warning; on a configuration
      change, a settings object replaces the settings with every
      valid field and the defaults of the rejected ones and sends the
      Warnings, and a non-object payload, `null` included, changes
      nothing and sends no Warning
- [ ] Reading `Settings` through `Deserialize` gives the same settings
      as the field-by-field reader for an object and fails for any
      other JSON value
- [ ] The tests of Task 1 and every other existing test pass with
      their assertions unchanged
- [ ] Integration tests in `tests/lsp_lifecycle/` pin, through
      `initialize` and through `workspace/didChangeConfiguration`,
      that a wrongly typed field and a wrong `schemas` entry each
      send a Warning while the other settings take effect, and that
      `kubernetesVersion: 1.29` sends exactly one Warning and selects
      `master`
- [ ] The doc comment of `MAX_LOGGED_SETTING_CHARS` and of
      `store_settings` describe what they cover now, and
      `rlsp-yaml/Cargo.toml` enables no `serde` feature that nothing
      uses
- [ ] `rlsp-yaml/docs/configuration.md` tells the reader what happens
      to a wrongly typed setting, a wrong `customTags` or `schemas`
      entry, and a `null` value, and `rlsp-yaml/docs/feature-log.md`
      has an entry that tells the reader that a wrongly typed setting
      no longer resets the others
- [ ] Build, tests, clippy, and fmt pass

## Non-Goals

- Marking the values the server rejects in the VS Code settings schema
  (`formatPrintWidth` as a non-negative integer, `schemas` values as
  strings) (separate plan after this one)
- Accepting the upstream yaml-language-server array form of a
  `schemas` value (`{"url": ["a.yaml", "b.yaml"]}`)
- Warning about unknown keys or about values of the right type that a
  setting does not accept (`yamlVersion`, `flowStyle`,
  `duplicateKeys`, `httpProxy`)

## Decisions

- **A per-field reader typed by the field:** each value still goes
  through serde with its field's type, so "wrong type" means exactly
  what made the derived reader fail, and the rejections return as
  data. Declined: a `deserialize_with` helper per field (cannot report
  the field without a side effect), a wrapper type per field (changes
  every public field type), `serde_with` and `serde_path_to_error`
  (new dependencies; the first cannot report, the second reports one
  error), and probing each key with the derived reader (the expected
  type comes from serde's error text and names an element's type).
- **A rejected field takes its default, not its previous value:** each
  configuration change replaces the whole settings object, as
  `configuration.md` documents.
- **Entries of `customTags` and `schemas` are dropped one by one:** one
  wrong schema association does not cost the others.
- **`null` counts as absent, silently:** `configuration.md` documents
  "absent or `null`" for settings, and most fields read it so today.
- **A non-object payload stays silent:** clients send
  `settings: null` as a bare change signal, which must change
  nothing.
- **`Deserialize for Settings` stays, as a hand-written impl:** the
  public trait and the existing tests that read through it remain;
  the change is not marked breaking, since no type changes and every
  input that reads today reads to the same settings — except a JSON
  array as the whole payload, which the derive read by field position
  and no client sends.
- **`Settings` in its own module:** reading settings is separate from
  the LSP handlers; the public path stays through a re-export.
