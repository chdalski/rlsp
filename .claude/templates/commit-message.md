# Commit Message Template

Uses [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
See `practices/conventional-commits.md` for the full spec.

## Format

```
<type>(<scope>): <short summary>

<what changed and why>

<what was tested>
```

## Fields

**Subject line**: follows Conventional Commits format.
See `practices/conventional-commits.md` for types and rules.

**What changed and why** (2-3 lines max):
- What you did and the reasoning behind it.
- Mention trade-offs or alternatives considered if relevant.
- Skip this section for trivial changes where the subject
  line says it all.

**What was tested** (1-2 lines):
- Which tests were added or updated.
- Confirmation that existing tests still pass.
- Skip for changes that don't affect code (docs, config).

## Example

```
feat(auth): add token refresh on 401 responses

API clients now automatically retry with a fresh token when
the server returns 401. Refresh is attempted once per request
to avoid loops.

Added tests for refresh success, refresh failure, and loop
prevention. Existing auth tests pass.
```
