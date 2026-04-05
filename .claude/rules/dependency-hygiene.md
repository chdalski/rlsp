---
paths:
  - "**/*.ts"
  - "**/*.tsx"
  - "**/*.py"
  - "**/*.rs"
  - "**/*.go"
---

# Dependency Hygiene

When source code stops using a dependency — whether by
switching to an alternative or by removing the feature that
needed it — the dependency must also be removed from the
manifest. Stale dependencies increase build times, expand
the attack surface for supply-chain vulnerabilities, and
confuse future readers about what the project actually
uses. Unlike dead code, unused dependencies rarely cause
test failures, so they persist indefinitely unless
explicitly checked.

## The Rule

After removing or replacing imports from a dependency,
check whether any remaining source file still imports it.
If none do, remove the dependency from the project
manifest (`Cargo.toml`, `package.json`, `pyproject.toml`,
`go.mod`, or equivalent).

## When to Check

- **Replacing a library** — you switch from library A to
  library B. After the switch, search for remaining
  imports of A. If none exist, remove A from the manifest.
- **Removing a feature** — you delete code that was the
  only consumer of a library. The library stays in the
  manifest unless you explicitly remove it.
- **Refactoring to reduce dependencies** — you inline
  functionality that previously came from an external
  package. Same check: any remaining imports? If not,
  remove it.

## How to Check

Search the codebase for imports of the package you
suspect is unused:

- **Rust:** `use <crate_name>` or `<crate_name>::` in
  source files (not `[dev-dependencies]` tests unless
  the dep is dev-only)
- **TypeScript/JavaScript:** `from '<package>'` or
  `require('<package>')` in source files
- **Python:** `import <package>` or `from <package>` in
  source files
- **Go:** `"<module-path>"` in import blocks

If the search returns no results outside of test files,
and the dependency is not a dev/test-only dependency, it
is unused and should be removed. If it returns no results
at all — including test files — remove it regardless of
which dependency section it is in.

## Stale References

When removing a dependency, also search the crate for
**comments and documentation** that reference it by name.
A comment saying "saphyr compatibility" after saphyr has
been removed confuses future readers and agents into
thinking the dependency still exists. Search for the
dependency name across all source files — not just import
statements — and update or remove stale references.

## What This Does NOT Mean

This rule does not require auditing all project
dependencies on every task. It applies only when your
changes remove or replace imports — a targeted check at
the point where staleness is introduced. Project-wide
dependency audits are a separate concern handled by
tooling or dedicated maintenance tasks.
