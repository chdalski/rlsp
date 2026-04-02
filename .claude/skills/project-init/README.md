# project-init skill

Scans a project and generates `CLAUDE.md` context files that
give agents accurate, project-specific grounding. Run it once
after copying `.claude/` into a new project, or re-run after
major structural changes.

## What it does

1. Reads manifest files to detect languages, frameworks, and
   test tooling
2. Applies language-specific initialization (e.g. Cargo lints
   for Rust projects, TypeScript strictness config)
3. Detects mono-repo structure and git-submodule sub-projects
4. Synthesizes an overview from README.md and manifest
   descriptions
5. Detects non-obvious conventions (lint inheritance, release
   automation, pre-commit hooks, etc.) and authoritative
   references (spec URLs, API docs)
6. Presents detected conventions and references to the user
   for confirmation before writing
7. Writes `CLAUDE.md` at the project root — and at the root
   of each sub-project that has its own `.git/`
8. Re-running preserves user-confirmed and agent-discovered
   Conventions and References entries while refreshing
   auto-detected sections

## Extending for a new language

Add a `<language>-init.md` file to this directory with
language-specific setup instructions (linter config, formatter
settings, etc.), then add a corresponding step to `SKILL.md`
that reads the file when that language is detected.

No other files need to change.

**Example:** `rust-init.md` applies required Clippy lints to
every `Cargo.toml` in the project. Adding `python-init.md`
with Ruff or mypy configuration would follow the same pattern.
