---
name: crates.io OIDC token is publish-only
description: The `CARGO_REGISTRY_TOKEN` provisioned by GitHub OIDC for crates.io publishing is scoped to publish only — yank operations are 403'd. Yanks require a personal crates.io API token.
type: project
---

The repo uses OIDC trusted publishing to crates.io
(`.github/workflows/release-plz.yml` requests
`id-token: write`). When a `CARGO_REGISTRY_TOKEN` env var
appears in a session, it is the OIDC-minted token and is
**publish-scoped only**. Calling `cargo yank` against it
returns:

```
status 403 Forbidden: this token does not have the
required permissions to perform this action
```

**Why:** OIDC trusted publishing tokens are minted with
`publish-new` / `publish-update` scopes by design — yank
is intentionally outside the automation surface to keep
yank a deliberate maintainer action.

**How to apply:** When a plan involves `cargo yank` and
relies on the in-session `CARGO_REGISTRY_TOKEN` to do
the yank, the assumption is wrong. Either route the yank
to the user (they have a personal token), or arrange a
separate yank-scoped token (e.g., `CRATES_IO_YANK_TOKEN`
secret + a `workflow_dispatch` job that uses it).
Document the token-scope distinction in any plan that
specifies a yank step.

**Discovered:** 2026-04-30 during plan
`2026-04-30-parser-0.9.0-milestone-and-version-convention.md`
— the lead asserted the in-env token could yank, ran
`cargo yank --version 0.8.0 rlsp-yaml-parser`, got 403,
and the user had to yank manually.
