# Conventional Commits

Based on the [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) specification.

## Format

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

## Types

- **feat** — a new feature (SemVer MINOR)
- **fix** — a bug fix (SemVer PATCH)
- **refactor** — code change that neither fixes a bug nor
  adds a feature
- **test** — adding or updating tests only
- **docs** — documentation changes only
- **chore** — maintenance tasks (dependencies, config, CI)
- **perf** — performance improvement
- **style** — formatting, whitespace, no code change
- **build** — build system or external dependency changes
- **ci** — CI/CD configuration changes

## Rules

- **type** is required. **scope** is optional but
  recommended — use the module, component, or area affected.
- **description** must immediately follow the colon and
  space. Use imperative mood, lowercase, no period at end.
  Max 70 characters.
- **body** is optional. Separate from description with a
  blank line. Explain what changed and why, not how.
- **footer(s)** are optional. Separate from body with a
  blank line. Use for metadata like `Reviewed-by:` or
  `Refs:`.

## Breaking Changes

Indicate a breaking change (SemVer MAJOR) in either of
two ways:

1. Append `!` after type/scope:
   `feat(api)!: remove legacy auth endpoint`
2. Add a footer:
   `BREAKING CHANGE: removed /v1/auth, use /v2/auth`

`BREAKING CHANGE` must be uppercase. Both methods can be
used together for clarity.

## Examples

```
feat(auth): add token refresh on 401 responses

API clients now automatically retry with a fresh token when
the server returns 401. Refresh is attempted once per
request to avoid loops.
```

```
fix(parser): handle empty input without panic

Return an empty result instead of unwrapping None.
```

```
refactor(db): extract connection pooling into module
```

```
feat(api)!: change response format to envelope

BREAKING CHANGE: all API responses now wrapped in
{ data, meta } envelope. Clients must update parsers.
```
