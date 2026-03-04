# project-init skill

Scans a project and generates `CLAUDE.md` context files that
give agents accurate, project-specific grounding. Run it once
after copying `.claude/` into a new project, or re-run after
major structural changes.

## What it does

1. Reads the project's manifest files to detect languages,
   frameworks, and test tooling
2. Applies language-specific initialization (e.g. Cargo lints
   for Rust projects)
3. Detects mono-repo structure and git-submodule sub-projects
4. Fills in the project context template and writes `CLAUDE.md`
   at the project root — and at the root of each sub-project
   that has its own `.git/`
5. Reports which rule files are active and which sections still
   need human input
6. If files were modified, offers to fix any issues introduced
   (e.g. compiler warnings surfaced by new lint rules) by
   creating a task and routing it into the standard workflow

## Extending for a new language

Add a `<language>-init.md` file to this directory with
language-specific setup instructions (linter config, formatter
settings, etc.), then add a corresponding step to `SKILL.md`
that reads the file when that language is detected.

No other files need to change.

**Example:** `rust-init.md` applies required Clippy lints to
every `Cargo.toml` in the project. Adding `python-init.md`
with Ruff or mypy configuration would follow the same pattern.
