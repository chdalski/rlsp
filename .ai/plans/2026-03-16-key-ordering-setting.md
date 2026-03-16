**Repository:** root
**Status:** Completed (2026-03-16)
**Created:** 2026-03-16

## Goal

Make the `validate_key_ordering` diagnostic opt-in via a
`keyOrdering` setting, matching the TypeScript upstream
where `yaml.keyOrdering` defaults to `false`. Currently
the Rust server always enforces alphabetical key ordering,
which produces false-positive warnings on files like
GitHub Actions workflows where conventional key order is
not alphabetical.

## Context

- `Settings` struct at `server.rs:28` already handles
  deserialization from `initializationOptions` and
  `workspace/didChangeConfiguration` — adding a field is
  straightforward
- `validate_key_ordering` is called unconditionally in
  `parse_and_publish` at `server.rs:85`
- TypeScript upstream: `yaml.keyOrdering` defaults to
  `false` (see `yaml-language-server/README.md:57`)
- Helix sends `[language-server.rlsp-yaml.config]` as
  `initializationOptions`, so users configure via
  `keyOrdering = true` in `languages.toml`
- Low risk: pure settings plumbing, no trust boundaries,
  no new public API

## Steps

- [x] Clarify requirements with user
- [x] Add `key_ordering` field to `Settings`
- [x] Gate `validate_key_ordering` call on the setting
- [x] Update existing tests if needed
- [x] Add a test for the default (disabled) behavior

## Tasks

### Task 1: Add keyOrdering setting and gate the validator (6a97dcc)

- [x] Add `pub key_ordering: bool` to `Settings` (serde
  default is `false`)
- [x] In `parse_and_publish`, only call
  `validate_key_ordering` when `self.settings.lock()`
  reports `key_ordering == true`
- [x] Add test: `Settings` deserialized from JSON with
  `keyOrdering: true` has the field set
- [x] Add test: `Settings` deserialized from JSON without
  `keyOrdering` defaults to `false`
- [x] Verify existing key-ordering validator tests still
  pass (they test the validator directly, not the setting)

## Decisions

- **Single task** — the change is small enough that
  splitting into multiple commits adds overhead without
  benefit
- **No advisor consultation** — pure settings plumbing,
  low risk, follows existing patterns exactly
