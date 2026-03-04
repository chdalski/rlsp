---
name: project-init
description: >
  Scan project to generate a project context document.
  Detects languages, frameworks, structure, and maps to
  the blueprint's conditional rules. Run after copying
  .claude/ to a new project, or to regenerate context
  after major changes.
---

# /project-init

Generate a project context document at `CLAUDE.md` (project
root) by scanning the project and filling in the template
from `.claude/templates/project-context.md`. Claude Code
loads root `CLAUDE.md` at cache level 3 alongside
`.claude/CLAUDE.md` — both are always in context without
conflicting, since `.claude/CLAUDE.md` contains lead
instructions and `CLAUDE.md` contains project context.

For projects with git-repository subdirectories (e.g., git
submodules), also generate a `CLAUDE.md` in each
subdirectory that has its own `.git/`. Each sub-project is
scanned independently so agents working there get context
specific to that sub-project's languages and structure.

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

### Framework Detection

Read manifest files to identify key dependencies. Common
frameworks to look for:

- **TypeScript/JavaScript**: React, Next.js, Vue, Angular,
  Express, Fastify, NestJS, Hono
- **Python**: FastAPI, Flask, Django, SQLAlchemy, Pydantic
- **Rust**: Actix-web, Axum, Tokio, Serde, Diesel,
  SQLx, Clap
- **Go**: Gin, Echo, Chi, GORM, Cobra

### Test Framework Detection

| Indicator | Test Framework |
|---|---|
| `jest.config.*` or `jest` in package.json | Jest |
| `vitest.config.*` or `vitest` in package.json | Vitest |
| `conftest.py` or `pytest.ini` or `[tool.pytest]` in pyproject.toml | pytest |
| `_test.go` files | Go testing |
| `[dev-dependencies]` with `test` crates (e.g., `rstest`, `proptest`) in Cargo.toml | Rust test crates |
| `#[cfg(test)]` in `.rs` files | Rust built-in tests |

### Mono-Repo Detection

A project is a mono-repo if any of these are true:

- `package.json` has a `workspaces` field
- `Cargo.toml` has a `[workspace]` section
- `pnpm-workspace.yaml` exists
- Multiple subdirectories contain their own manifest files
  (package.json, Cargo.toml, pyproject.toml, go.mod)
- Subdirectories contain `.git/` (git submodules)

For mono-repos, add a "Sub-Projects" section to the root
`CLAUDE.md` listing each component with its path, language,
and purpose. This is separate from per-sub-project
`CLAUDE.md` generation — workspace members without their
own `.git/` get listed in the root Sub-Projects section but
do not get their own `CLAUDE.md`. Subdirectories with their
own `.git/` get both: a listing in the root and their own
independently-scanned `CLAUDE.md`.

### Language-to-Rule Mapping

Map detected languages to the blueprint's conditional rule
files. These rules auto-load via `paths:` frontmatter but
listing them helps users understand what guidance is active.

| Language | Rule Files |
|---|---|
| TypeScript / TSX | `lang-typescript.md`, `functional-style.md` |
| Python | `lang-python.md`, `functional-style.md` |
| Rust | `lang-rust.md`, `functional-style.md` |
| Go | `lang-go.md` |
| All source code | `code-principles.md`, `code-mass.md` |

## Steps

1. **Read template** — read
   `.claude/templates/project-context.md` for the canonical
   structure.

2. **Check for existing context** — if `CLAUDE.md` exists
   at the project root, ask the user whether to overwrite
   or skip. User-curated content is valuable and should not
   be silently replaced.

3. **Scan for manifests** — search the project root and one
   level of subdirectories for manifest files (package.json,
   tsconfig.json, Cargo.toml, pyproject.toml, setup.py,
   requirements.txt, go.mod, pnpm-workspace.yaml).

4. **Read manifests** — read detected manifest files to
   extract language versions, dependencies, and framework
   information.

5. **Apply Cargo lints** (Rust projects only) — if Rust was
   detected, read `.claude/skills/project-init/rust-init.md`
   and follow its instructions to update all `Cargo.toml` files
   in the project.

6. **Detect mono-repo** — check for workspace fields,
   multiple manifest directories, or git submodules. If
   detected, catalog sub-projects with their paths,
   languages, and inferred purposes.

7. **Map to rules** — use the language-to-rule mapping
   table to determine which conditional rule files are
   active for this project.

8. **Fill template** — replace auto-detected sections
   (Languages and Frameworks, Project Structure, Active
   Rules, Build and Test, Sub-Projects if applicable) with
   actual findings. Keep `<!-- TODO: ... -->` placeholders
   for human-curated sections (Overview, Architecture, Code
   Exemplars, Anti-Patterns, Trusted Sources).

9. **Write context** — write the filled template to
   `CLAUDE.md` at the project root. Then check for
   subdirectories with their own `.git/` — for each one,
   scan it independently (repeat steps 3-8 scoped to that
   subdirectory) and write its own `CLAUDE.md`. Check for
   existing `CLAUDE.md` in each location before writing.

10. **Present summary** — report to the caller:
    - Languages and frameworks detected
    - Whether mono-repo structure was found
    - Which rule files are active
    - Which sections need human curation (the TODO sections)
    - Which `Cargo.toml` files were updated with lints (Rust
      projects only)
    - Whether any files beyond `CLAUDE.md` were modified
      (e.g. Cargo.toml lint updates) — the lead uses this
      to decide whether to offer cleanup during clarification
