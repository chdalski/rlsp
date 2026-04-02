---
name: project-init
description: >
  Scan project to generate CLAUDE.md context files.
  Synthesizes overview from README, detects build commands,
  conventions, and references. Confirms findings with the
  user before writing. Re-running preserves user-confirmed
  and agent-discovered content.
---

# /project-init

Generate `CLAUDE.md` files by scanning the project and
synthesizing context that agents cannot infer from code
alone. The output format is defined in
`.claude/skills/project-init/project-context.md`.

For projects with git-repository subdirectories (e.g., git
submodules), also generate a `CLAUDE.md` in each
subdirectory that has its own `.git/`. Each sub-project is
scanned independently so agents working there get context
specific to that sub-project.

## Detection Tables

### Language Detection

Detect languages from manifest files at the project root
and one level of subdirectories. A project may use multiple
languages.

| Manifest File | Condition | Language |
|---|---|---|
| `package.json` + `tsconfig.json` | Both present | TypeScript |
| `package.json` (no `tsconfig.json`) | tsconfig absent | JavaScript |
| `Cargo.toml` | Present | Rust |
| `pyproject.toml` | Present | Python |
| `setup.py` | Present | Python |
| `requirements.txt` | Present | Python |
| `go.mod` | Present | Go |

### Test Framework Detection

| Indicator | Test Framework |
|---|---|
| `jest.config.*` or `jest` in package.json | Jest |
| `vitest.config.*` or `vitest` in package.json | Vitest |
| `conftest.py` or `pytest.ini` or `[tool.pytest]` in pyproject.toml | pytest |
| `_test.go` files | Go testing |
| `[dev-dependencies]` with test crates in Cargo.toml | Rust test crates |
| `#[cfg(test)]` in `.rs` files | Rust built-in tests |

### Mono-Repo Detection

A project is a mono-repo if any of these are true:

- `package.json` has a `workspaces` field
- `Cargo.toml` has a `[workspace]` section
- `pnpm-workspace.yaml` exists
- Multiple subdirectories contain their own manifest files
- Subdirectories contain `.git/` (git submodules)

### Convention Detection

Scan for files that indicate non-obvious project
conventions — things agents need to know but cannot infer
from code structure alone.

| Signal | Convention |
|---|---|
| Root `Cargo.toml` with `[workspace.lints]` | Workspace lint inheritance — crates use `lints.workspace = true` |
| Root `tsconfig.json` extending `@tsconfig/strictest` | Maximum TypeScript compiler strictness |
| Root `eslint.config.*` with `strictTypeChecked` | Strict type-aware linting |
| `.pre-commit-config.yaml` or `.husky/` | Pre-commit hooks enforce checks before commit |
| `release-plz.toml`, `.goreleaser.yml`, or `semantic-release` config | Automated release pipeline |
| `cliff.toml` or `commitlint.config.*` | Conventional commits required |
| `renovate.json` or `dependabot.yml` | Automated dependency updates |
| `turbo.json` or `nx.json` | Monorepo task orchestration |
| `Makefile`, `justfile`, or `Taskfile.yml` | Custom task runner with project-specific commands |
| `.editorconfig` | Editor config enforced across contributors |

This table is not exhaustive — also look for patterns in
CI configs, README badges, and manifest scripts that
reveal conventions not listed here.

### Reference Detection

Scan these locations for authoritative URLs:

- README.md — links in the first few sections
- `docs/` directory — links in documentation files
- Manifest files — `repository`, `homepage` fields
- Configuration comments — URLs referencing specs

Filter for authoritative sources: specifications, API
docs, RFCs, design documents. Skip generic links (GitHub
homepages, npm/crates.io package pages).

## Steps

1. **Read output format** — read
   `.claude/skills/project-init/project-context.md` for
   the output structure.

2. **Check for existing CLAUDE.md** — if `CLAUDE.md`
   exists at the project root, read it. If it has
   Conventions or References sections, extract their
   entries for preservation. Ask the user whether to
   regenerate (refreshes Overview, Build and Test,
   Components while preserving Conventions and References)
   or skip.

3. **Scan for manifests** — search the project root and
   one level of subdirectories for manifest files
   (package.json, tsconfig.json, Cargo.toml, pyproject.toml,
   setup.py, requirements.txt, go.mod, pnpm-workspace.yaml).

4. **Read manifests** — extract build, test, lint, format,
   and clean commands. Also extract project descriptions
   and dependency information.

5. **Apply Cargo lints** (Rust only) — if Rust was
   detected, read `.claude/skills/project-init/rust-init.md`
   and follow its instructions to update all `Cargo.toml`
   files in the project.

6. **Apply TypeScript strictness** (TypeScript only) — if
   TypeScript was detected, read
   `.claude/skills/project-init/typescript-init.md` and
   follow its instructions to update `tsconfig.json`,
   `eslint.config.mjs`, and `package.json` in each
   TypeScript project root.

7. **Detect mono-repo** — check for workspace fields,
   multiple manifest directories, or git submodules. If
   detected, catalog components with their paths and
   purposes (from component README.md or manifest
   descriptions).

8. **Synthesize overview** — read README.md (first few
   paragraphs) and manifest `description` fields. Write
   2-4 sentences: what the project is, who it serves,
   why it exists. If no README exists, infer from code
   structure and manifest metadata.

9. **Detect conventions** — scan for convention signals
   using the Convention Detection table. Note each finding
   as a one-line entry.

10. **Detect references** — scan for authoritative URLs
    using the Reference Detection guidance. Note each
    finding.

11. **Confirm with user** — present detected conventions
    and references to the user via `AskUserQuestion`:
    - "I detected these conventions — are they correct?
      Anything to add or remove?" (list detected
      conventions plus any preserved from existing
      CLAUDE.md)
    - "These look like authoritative references — should
      I include them? Any to add?" (list detected
      references plus any preserved)

    Merge user feedback with preserved entries. If the
    user adds new entries, include them.

12. **Write CLAUDE.md** — assemble the output following the
    format in `.claude/skills/project-init/project-context.md`.
    Write to the project root. Then check for subdirectories
    with their own `.git/` — for each one, scan it
    independently (repeat steps 3-11 scoped to that
    subdirectory) and write its own `CLAUDE.md`. Check for
    existing `CLAUDE.md` in each location before writing.

13. **Present summary** — report to the caller:
    - Overview synthesized (brief description of what was
      written)
    - Build and test commands detected
    - Whether mono-repo structure was found
    - Conventions and references included
    - Which `Cargo.toml` files were updated with lints
      (Rust projects only)
    - Which TypeScript config files were updated
      (TypeScript projects only)
    - Whether any files beyond `CLAUDE.md` were modified
